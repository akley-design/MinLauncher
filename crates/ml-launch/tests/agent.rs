//! The profiling agent, end to end.
//!
//! The unit tests of the agent check that it counts frames; this test
//! checks that the launcher and the agent fit together: the launcher
//! builds the command line, the JVM accepts it, the agent writes its
//! report, and the launcher reads it back.
//!
//! The test needs a JVM and the jar of the agent. Both are optional:
//! when they are not there the test says so and passes, because a
//! machine without a JDK is a machine on which the launcher has to
//! work anyway — it just cannot be measured from the inside.
//!
//! The agent is built by Gradle, in `apps/java`. The jar is looked for
//! in the build directory of the Gradle build, and in the place the
//! launcher puts it.

use std::path::{Path, PathBuf};
use std::process::Command;

use ml_core::{FeatureSet, OsArch, OsName, Platform};
use ml_launch::args::Session;
use ml_launch::command::{LaunchRequest, build_command};
use ml_launch::jvm::{JavaInstallation, JavaOrigin};

/// The name of the property that tells the agent where to write.
const OUTPUT_PROPERTY: &str = "minlaucher.agent.output";

fn platform() -> Platform {
    Platform::new(OsName::Windows, OsArch::X86_64)
}

/// A JVM to run the test with, when there is one.
fn java() -> Option<PathBuf> {
    // The Java home of the environment comes first, because that is
    // where the machine of the test puts its JDK.
    if let Ok(home) = std::env::var("JAVA_HOME")
        && !home.is_empty()
    {
        let candidate = Path::new(&home).join("bin").join("java.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    for name in ["java.exe", "java"] {
        let candidate = Path::new("C:\\Program Files\\Java")
            .join("jdk-26")
            .join("bin")
            .join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

/// The jar of the agent, when it has been built.
fn agent_jar() -> Option<PathBuf> {
    let roots = [
        PathBuf::from("C:/Users/Public/minlaucher-java-build/agent/libs"),
        PathBuf::from("../../apps/java/agent/build/libs"),
    ];

    for root in roots {
        let candidate = root.join("minlaucher-agent-0.1.0.jar");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// A stand-in for the game, written into the directory of the test.
///
/// The class has the same name as the class of LWJGL that puts the
/// frame on the screen, because that is the class the agent weaves.
fn write_fake_game(directory: &Path) -> PathBuf {
    let package = directory.join("org").join("lwjgl").join("glfw");
    std::fs::create_dir_all(&package).unwrap();

    let source = directory.join("org/lwjgl/glfw/GLFW.java");
    std::fs::write(
        &source,
        r#"package org.lwjgl.glfw;

public final class GLFW {
    public static void glfwSwapBuffers(long window) {
        long sum = 0L;
        for (int i = 0; i < 1000; i++) {
            sum += i;
        }
        if (sum == Long.MIN_VALUE) {
            throw new IllegalStateException("never");
        }
    }
}
"#,
    )
    .unwrap();

    let game = directory.join("fakegame");
    std::fs::create_dir_all(&game).unwrap();
    let game_source = game.join("FakeGame.java");
    std::fs::write(
        &game_source,
        r#"package fakegame;

import org.lwjgl.glfw.GLFW;

public final class FakeGame {
    public static void main(String[] args) throws Exception {
        for (int i = 0; i < 30; i++) {
            GLFW.glfwSwapBuffers(0L);
            Thread.sleep(16L);
        }
    }
}
"#,
    )
    .unwrap();

    source.parent().unwrap().to_path_buf()
}

#[test]
fn the_launcher_and_the_agent_work_together() {
    let Some(java) = java() else {
        eprintln!("there is no JVM on this machine, the test is skipped");
        return;
    };
    let Some(agent) = agent_jar() else {
        eprintln!("the agent has not been built, the test is skipped");
        return;
    };

    let dir = tempfile::tempdir().unwrap();
    let report = dir.path().join("report.json");

    // The launcher builds the command line, exactly as it does for a
    // real game.
    let request = LaunchRequest {
        version: ml_minecraft::version::VersionJson::parse(
            br#"{"id":"26.3","type":"release","mainClass":"fakegame.FakeGame"}"#,
        )
        .unwrap(),
        libraries: Vec::new(),
        java: JavaInstallation {
            executable: java.clone(),
            windowless_executable: None,
            major_version: 25,
            version: "25.0.1".into(),
            origin: JavaOrigin::System,
        },
        session: Session::default(),
        client_jar: PathBuf::from("C:/v/client.jar"),
        library_directory: PathBuf::from("C:/l"),
        extra_jvm_args: Vec::new(),
        agent_jar: Some(agent.clone()),
        agent_report: Some(report.clone()),
        loader: None,
        mod_jars: Vec::new(),
    };

    let command = build_command(&request, platform(), &FeatureSet::none(), "release").unwrap();

    // The argument of the agent and the property that says where the
    // report goes are both there.
    assert!(
        command
            .arguments
            .iter()
            .any(|argument| argument.starts_with("-javaagent:")),
        "the agent is not in the command line: {:?}",
        command.arguments
    );
    assert!(
        command
            .arguments
            .iter()
            .any(|argument| argument.starts_with(&format!("-D{OUTPUT_PROPERTY}="))),
        "the property of the report is not in the command line: {:?}",
        command.arguments
    );

    // The game is written and compiled.
    let classes = dir.path().join("classes");
    std::fs::create_dir_all(&classes).unwrap();
    write_fake_game(dir.path());

    let javac = java.with_file_name("javac.exe");
    let compile = Command::new(&javac)
        .arg("-d")
        .arg(&classes)
        .arg(dir.path().join("org/lwjgl/glfw/GLFW.java"))
        .arg(dir.path().join("fakegame/FakeGame.java"))
        .output()
        .expect("the JDK cannot compile the fake game");
    assert!(
        compile.status.success(),
        "the fake game did not compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    // And the command is run. The arguments of the launcher are the
    // arguments of the JVM up to the main class, and the main class
    // comes with the arguments of the game after it. The class path
    // of the fake game replaces the one of the launcher, because the
    // libraries of a real game are not here.
    let main_class = command
        .arguments
        .iter()
        .position(|argument| argument == "fakegame.FakeGame")
        .expect("the main class is not in the command line");

    let mut arguments: Vec<String> = command.arguments[..main_class].to_vec();
    arguments.retain(|argument| !argument.starts_with("-javaagent:"));
    arguments.retain(|argument| !argument.starts_with("-D"));

    // The agent and the property of the report are the arguments of
    // the JVM, and they have to come before the main class.
    arguments.push(format!("-D{OUTPUT_PROPERTY}={}", report.display()));
    arguments.push(format!("-javaagent:{}", agent.display()));

    arguments.push("-cp".to_string());
    arguments.push(classes.to_string_lossy().into_owned());
    arguments.push("fakegame.FakeGame".to_string());

    let run = Command::new(&java)
        .args(&arguments)
        .current_dir(dir.path())
        .output()
        .expect("the JVM cannot be started");

    assert!(
        run.status.success(),
        "the game did not run: {}\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );

    // The agent wrote its report, and the launcher reads it back.
    let report = ml_launch::read_report(&report).expect("the report was not written");
    assert!(
        report.is_usable(),
        "the report holds no measurement: {report:?}"
    );
    assert!(report.frames > 0, "no frame was counted: {report:?}");
    assert!(
        report.average_fps > 0.0,
        "the frame rate is not a number: {report:?}"
    );
    // Thirty frames, sixteen milliseconds apart: about sixty a second.
    assert!(
        report.average_frametime_ms > 10.0 && report.average_frametime_ms < 40.0,
        "the frame time is not what it should be: {report:?}"
    );
}
