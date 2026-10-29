//! Turning a version into a runnable command line.
//!
//! This is the step that turns the *description* of a launch — the
//! version JSON, the resolved libraries, the session — into the exact
//! argument vector the game is started with.
//!
//! The order of the arguments matters, and it is the order the version
//! JSON prescribes:
//!
//! ```text
//! <java> <version user args> <player args> <version jvm args> <main class> <game args>
//! ```
//!
//! The arguments of the player come after the arguments of the version,
//! so that they win where both of them say something about the same
//! thing: a player who passes `-Xmx8G` gets `8G`, not the `4G` of the
//! version. The arguments of the version itself come last, because they
//! carry the paths of the natives, and a player cannot break those.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use ml_core::{Error, FeatureSet, Platform, Result};
use ml_minecraft::fabric::FabricLaunch;
use ml_minecraft::library::{NativeFamily, ResolvedLibrary};
use ml_minecraft::version::VersionJson;

use crate::args::{Session, build_classpath, native_directories, substitute_all};
use crate::jvm::JavaInstallation;
use crate::natives::{NativeJar, NativeLayout};

/// Everything the launcher needs to start the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchRequest {
    /// The version to start.
    pub version: VersionJson,
    /// The libraries the rules allowed, in the order of the version JSON.
    pub libraries: Vec<ResolvedLibrary>,
    /// The Java runtime to use.
    pub java: JavaInstallation,
    /// The session values.
    pub session: Session,
    /// The client jar of the version.
    pub client_jar: PathBuf,
    /// The root of the library store.
    pub library_directory: PathBuf,
    /// Extra JVM arguments the user configured.
    pub extra_jvm_args: Vec<String>,
    /// The jar of the profiling agent, when it is enabled.
    pub agent_jar: Option<PathBuf>,
    /// The file the agent writes its report to.
    ///
    /// The path travels in a system property and not in the options of
    /// the agent: the JVM splits `-javaagent:jar=options` at the first
    /// `=`, so a path that has an `=` in it would cut the jar path in
    /// half and the agent would not load at all.
    pub agent_report: Option<PathBuf>,
    /// The loader that wraps the game, when there is one.
    ///
    /// The loader does not replace the game, it goes in front of it: the
    /// game is still the game of Mojang, and the loader starts it.
    pub loader: Option<FabricLaunch>,
    /// The jars of the mods.
    ///
    /// They do not go on the classpath: the loader finds them in the
    /// directory of the mods and loads them itself, so that it can
    /// transform them first.
    pub mod_jars: Vec<PathBuf>,
}

impl LaunchRequest {
    /// The library jars that go on the classpath, in order.
    ///
    /// Every library that resolved contributes its artifact, the natives
    /// jars included: LWJGL finds its native inside the jar on the
    /// classpath and unpacks it itself.
    ///
    /// The libraries of the loader come first, when there is a loader:
    /// the loader reads the game out of the classpath, and it has to
    /// find its own classes before the game is touched at all.
    ///
    /// A library of the loader *replaces* the library of the game with
    /// the same identity, instead of standing next to it. The loader
    /// refuses to start when two jars on the classpath carry the same
    /// class, and the game and the loader do not agree on the version of
    /// ASM: the game of 1.21.4 carries ASM 9.6 and the loader carries
    /// ASM 9.10.1. Both of them have to be on the classpath, and only
    /// one of them may be there, so the one of the loader wins — it is
    /// the newer one, and it is the one the loader needs.
    #[must_use]
    pub fn classpath_entries(&self) -> Vec<PathBuf> {
        let mut entries = Vec::new();
        let mut taken = HashSet::new();

        if let Some(loader) = &self.loader {
            for library in &loader.libraries {
                if library.artifact.is_none() {
                    continue;
                }
                taken.insert(library.coordinate.identity());
                entries.push(
                    self.library_directory
                        .join(library.coordinate.artifact_path()),
                );
            }
        }

        for library in &self.libraries {
            if library.artifact.is_none() {
                continue;
            }
            if !taken.insert(library.coordinate.identity()) {
                continue;
            }
            entries.push(
                self.library_directory
                    .join(library.coordinate.artifact_path()),
            );
        }

        entries
    }

    /// The native jars that have to be unpacked before the game starts.
    #[must_use]
    pub fn native_jars(&self, platform: Platform) -> Vec<NativeJar> {
        self.libraries
            .iter()
            .filter_map(|library| {
                // A library only contributes a native jar when the
                // platform's classifier is the one it ships. The path is
                // the one the document downloaded: for the old documents
                // it is the classifier jar, not the main artifact.
                library.native_jar(platform)?;
                let path = library.native_path_for_platform(platform)?;
                let family = NativeFamily::of_group(&library.coordinate.group);
                Some(NativeJar {
                    path: self.library_directory.join(path),
                    family,
                })
            })
            .collect()
    }
}

/// The command line the game is started with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCommand {
    /// The executable, usually `java.exe` or `javaw.exe`.
    pub executable: PathBuf,
    /// The arguments, without the executable.
    pub arguments: Vec<String>,
    /// The working directory of the process.
    pub working_directory: PathBuf,
    /// The environment of the process.
    pub environment: Vec<(String, String)>,
    /// The natives layout the command was built for.
    pub natives_layout: NativeLayout,
}

impl LaunchCommand {
    /// The command line safe to show in logs and the user interface.
    #[must_use]
    pub fn display(&self) -> String {
        let mut parts = vec![self.executable.to_string_lossy().into_owned()];
        let mut redact_next = false;
        for argument in &self.arguments {
            if redact_next {
                parts.push("<redacted>".to_string());
                redact_next = false;
                continue;
            }
            if matches!(argument.as_str(), "--accessToken" | "--session") {
                parts.push(argument.clone());
                redact_next = true;
                continue;
            }
            if let Some((name, _)) = argument.split_once('=')
                && matches!(name, "--accessToken" | "--session")
            {
                parts.push(format!("{name}=<redacted>"));
                continue;
            }
            parts.push(argument.clone());
        }
        parts.join(" ")
    }
}

/// Build the command line of a launch.
///
/// # Errors
/// Fails when the version carries no entry point class, or when a
/// required path is missing.
pub fn build_command(
    request: &LaunchRequest,
    platform: Platform,
    features: &FeatureSet,
    version_type: &str,
) -> Result<LaunchCommand> {
    // The loader goes in front of the game, so it is the loader's class
    // that is started, not the class of the version.
    let main_class = if let Some(loader) = &request.loader {
        loader.main_class.clone()
    } else {
        let Some(main_class) = request.version.main_class.as_ref() else {
            return Err(Error::InvalidInput(format!(
                "version `{}` has no main class",
                request.version.id
            )));
        };
        main_class.clone()
    };

    let mut session = request.session.clone();
    session.version_type = version_type.to_string();

    // The classpath has to be known before the arguments are resolved,
    // because `${classpath}` is one of them.
    let mut values: BTreeMap<&str, String> = session.placeholders();
    values.insert(
        "classpath",
        build_classpath(&request.client_jar, &request.classpath_entries()),
    );
    values.insert(
        "classpath_separator",
        if platform.os.as_str() == "windows" {
            ";"
        } else {
            ":"
        }
        .to_string(),
    );
    let classpath = values.get("classpath").map_or("", String::as_str);

    let version_id = Some(request.version.id.as_str());
    let jvm_arguments =
        build_jvm_arguments(request, platform, features, &session, &values, classpath);

    // The game arguments.
    //
    // The versions before 1.13 spell their arguments as one string, and
    // the versions from 1.13 on use the `arguments` block; no document
    // carries both. The document knows which form it uses, so the
    // launcher reads the one that is there.
    let mut game_arguments = substitute_all(
        &request
            .version
            .prescribed_game_arguments(platform, version_id, features),
        &values,
    );
    game_arguments.insert(0, main_class.clone());

    let mut arguments = jvm_arguments;
    arguments.extend(game_arguments);

    // The natives layout follows the arguments the version asks for.
    let natives_layout = NativeLayout {
        natives_directory: session.natives_directory.clone(),
        directories: native_directories(&arguments),
    };

    let executable = request
        .java
        .windowless_executable
        .clone()
        .unwrap_or_else(|| request.java.executable.clone());

    Ok(LaunchCommand {
        executable,
        arguments,
        working_directory: session.game_directory.clone(),
        environment: Vec::new(),
        natives_layout,
    })
}

fn build_jvm_arguments(
    request: &LaunchRequest,
    platform: Platform,
    features: &FeatureSet,
    session: &Session,
    values: &BTreeMap<&str, String>,
    classpath: &str,
) -> Vec<String> {
    let version_id = Some(request.version.id.as_str());
    let mut version_arguments = substitute_all(
        &request
            .version
            .arguments
            .resolve_default_user_jvm(platform, version_id, features),
        values,
    );

    // The JVM refuses two collectors, so a player choice replaces the
    // collector that came from the version document.
    if request
        .extra_jvm_args
        .iter()
        .any(|argument| ml_core::config::is_collector_flag(argument))
    {
        version_arguments.retain(|argument| !ml_core::config::is_collector_flag(argument));
    }

    // A player who names a heap size owns both of them. Only one half of
    // the pair of the version would be replaced, and a virtual machine
    // refuses to start when the initial heap outgrows the maximum
    // ("Initial heap size set to a larger value than the maximum heap
    // size"), which is what a 2G player setting met by the 4G of a
    // snapshot would have meant.
    let player_names_heap = request
        .extra_jvm_args
        .iter()
        .any(|argument| is_heap_flag(argument));
    if player_names_heap {
        version_arguments.retain(|argument| !is_heap_flag(argument));
    }

    let mut jvm_arguments = version_arguments;
    // Player arguments come after the version defaults, so their values win.
    jvm_arguments.extend(request.extra_jvm_args.iter().cloned());

    let mut version_jvm_arguments = request
        .version
        .arguments
        .resolve_jvm(platform, version_id, features);
    if player_names_heap {
        version_jvm_arguments.retain(|argument| !is_heap_flag(argument));
    }
    jvm_arguments.extend(substitute_all(&version_jvm_arguments, values));
    jvm_arguments.extend(launcher_jvm_arguments(
        &version_jvm_arguments,
        session,
        classpath,
    ));

    // Loader arguments follow the game arguments so the game's values win.
    if let Some(loader) = &request.loader {
        jvm_arguments.extend(loader.jvm_arguments.iter().cloned());
        // Tell the loader which client jar the launcher put on the classpath.
        jvm_arguments.push(format!(
            "-Dfabric.gameJarPath.client={}",
            request.client_jar.display()
        ));
        // The loader reads mods from its own directory and transforms them.
        if !request.mod_jars.is_empty() {
            let separator = crate::args::classpath_separator();
            let paths: Vec<String> = request
                .mod_jars
                .iter()
                .map(|jar| jar.display().to_string())
                .collect();
            jvm_arguments.push(format!(
                "-Dfabric.addMods={}",
                paths.join(&separator.to_string())
            ));
        }
    }

    if let Some(agent) = &request.agent_jar
        && let Some(argument) = crate::args::java_agent(agent)
    {
        // Set the report location before premain reads it.
        if let Some(report) = &request.agent_report {
            jvm_arguments.push(crate::args::agent_output_property(report));
        }
        jvm_arguments.push(argument);
    }

    jvm_arguments
}

/// The argument names a heap size: `-Xmx4G` or `-Xms1G`.
fn is_heap_flag(argument: &str) -> bool {
    argument.starts_with("-Xmx") || argument.starts_with("-Xms")
}

/// The JVM arguments the launcher supplies for the versions that
/// prescribe none.
///
/// The versions before 1.13 have no `arguments` block at all: the
/// document names the arguments of the game and nothing else. Two of
/// them the JVM cannot do without — the classpath, without which the
/// game is not found on it, and the directories the natives were
/// unpacked into — and the launcher of Mojang supplied both.
///
/// The arguments a version prescribes are never touched: a modern
/// version names the classpath and all four directories itself, and this
/// function then adds nothing.
fn launcher_jvm_arguments(
    version_jvm_arguments: &[String],
    session: &Session,
    classpath: &str,
) -> Vec<String> {
    let mut arguments = Vec::new();

    // The classpath, unless the version names its own.
    if !version_jvm_arguments
        .iter()
        .any(|argument| argument == "-cp" || argument.starts_with("-cp="))
    {
        arguments.push("-cp".to_string());
        arguments.push(classpath.to_string());
    }

    // The directories of the natives, and only for a version that names
    // no directory of its own. The LWJGL of 1.14 to 1.19 is handed
    // `java.library.path` alone and searches nothing else: a launcher that
    // adds its own extract paths next to it unpacks the libraries into
    // directories the game never looks at, and the game answers with
    // “Failed to locate library: lwjgl.dll”. Such a version keeps its own
    // model, and the natives follow it (see `NativeLayout::directory_for`).
    if !version_jvm_arguments
        .iter()
        .any(|argument| argument.starts_with("-Djava.library.path="))
    {
        for (family, property) in [
            (NativeFamily::System, "java.library.path"),
            (NativeFamily::Jna, "jna.tmpdir"),
            (
                NativeFamily::Lwjgl,
                "org.lwjgl.system.SharedLibraryExtractPath",
            ),
            (NativeFamily::Netty, "io.netty.native.workdir"),
        ] {
            let prefix = format!("-D{property}=");
            if version_jvm_arguments
                .iter()
                .any(|argument| argument.starts_with(&prefix))
            {
                continue;
            }
            arguments.push(format!(
                "{prefix}{}",
                crate::args::path_to_string(&session.natives_directory.join(family.directory()))
            ));
        }
    }

    arguments
}

/// The classpath of a request, without building the whole command.
#[must_use]
pub fn request_classpath(request: &LaunchRequest) -> String {
    build_classpath(&request.client_jar, &request.classpath_entries())
}

/// The directory the game writes its files into.
#[must_use]
pub fn game_directory(session: &Session) -> &Path {
    &session.game_directory
}

#[cfg(test)]
mod tests {
    use super::*;
    use ml_core::{OsArch, OsName};
    use ml_minecraft::version::Library;

    const VERSION: &str = r#"{
        "id": "26.3",
        "type": "release",
        "mainClass": "net.minecraft.client.main.Main",
        "assets": "34",
        "arguments": {
            "jvm": [
                { "rules": [ { "action": "allow", "os": { "name": "windows" } } ], "value": "-XX:HeapDumpPath=heapdump" },
                "-XX:StackShadowPages=32",
                "--enable-native-access=ALL-UNNAMED",
                "-Djava.library.path=${natives_directory}/java",
                "-Djna.tmpdir=${natives_directory}/jna",
                "-Dorg.lwjgl.system.SharedLibraryExtractPath=${natives_directory}/lwjgl",
                "-Dio.netty.native.workdir=${natives_directory}/netty",
                "-Dminecraft.launcher.brand=${launcher_name}",
                "-Dminecraft.launcher.version=${launcher_version}",
                "-cp",
                "${classpath}"
            ],
            "game": [
                "--username", "${auth_player_name}",
                "--version", "${version_name}",
                "--gameDir", "${game_directory}",
                "--assetsDir", "${assets_root}",
                "--assetIndex", "${assets_index_name}",
                "--uuid", "${auth_uuid}",
                "--accessToken", "${auth_access_token}",
                "--versionType", "${version_type}"
            ],
            "default-user-jvm": [
                { "value": [ "-Xms2G", "-Xmx4G" ] },
                { "rules": [ { "action": "allow", "os": { "name": "windows", "versionRange": { "min": "10.0.17134" } } } ], "value": [ "-XX:+UseZGC" ] }
            ]
        }
    }"#;

    fn platform() -> Platform {
        Platform::new(OsName::Windows, OsArch::X86_64).with_os_version(Some("10.0.26300"))
    }

    fn request() -> LaunchRequest {
        let version = VersionJson::parse(VERSION.as_bytes()).unwrap();
        let libraries = version.resolve_libraries(platform(), Some("26.3"));
        let session = Session {
            player_name: "Steve".into(),
            uuid: "1234-5678-9abc-def0-1234-56789abc0000".into(),
            uuid_prefix: "123456789abcdef0123456789abc0000".into(),
            access_token: "token".into(),
            version_name: "26.3".into(),
            game_directory: PathBuf::from("C:/games/mc"),
            assets_root: PathBuf::from("C:/cache/assets"),
            assets_index_name: "34".into(),
            library_directory: PathBuf::from("C:/cache/libraries"),
            natives_directory: PathBuf::from("C:/games/mc/natives"),
            launcher_name: "MinLauncher".into(),
            launcher_version: "0.1.0".into(),
            ..Session::default()
        };

        LaunchRequest {
            version,
            libraries,
            java: JavaInstallation {
                executable: PathBuf::from("C:/jre/bin/java.exe"),
                windowless_executable: Some(PathBuf::from("C:/jre/bin/javaw.exe")),
                major_version: 25,
                version: "25.0.1".into(),
                origin: crate::jvm::JavaOrigin::Mojang,
            },
            session,
            client_jar: PathBuf::from("C:/versions/26.3/client.jar"),
            library_directory: PathBuf::from("C:/cache/libraries"),
            extra_jvm_args: Vec::new(),
            agent_jar: None,
            agent_report: None,
            loader: None,
            mod_jars: Vec::new(),
        }
    }

    #[test]
    fn displayed_command_never_exposes_minecraft_session_tokens() {
        let mut command =
            build_command(&request(), platform(), &FeatureSet::none(), "release").unwrap();
        command.arguments.extend([
            "--session".into(),
            "token:legacy-secret:player-id:Steve".into(),
            "--accessToken=inline-secret".into(),
        ]);

        let displayed = command.display();
        assert!(!displayed.contains("legacy-secret"));
        assert!(!displayed.contains("inline-secret"));
        assert!(!displayed.contains("token"));
        assert!(displayed.contains("--session <redacted>"));
        assert!(displayed.contains("--accessToken <redacted>"));
    }

    #[test]
    fn the_command_has_the_expected_shape() {
        let command =
            build_command(&request(), platform(), &FeatureSet::none(), "release").unwrap();

        assert_eq!(command.executable, PathBuf::from("C:/jre/bin/javaw.exe"));

        // The main class separates the JVM arguments from the game ones.
        let main = command
            .arguments
            .iter()
            .position(|argument| argument == "net.minecraft.client.main.Main")
            .unwrap();

        assert!(command.arguments[0..main].contains(&"-Xmx4G".to_string()));
        assert!(command.arguments[0..main].contains(&"-XX:+UseZGC".to_string()));
        assert!(command.arguments[0..main].contains(&"-XX:StackShadowPages=32".to_string()));
        assert_eq!(command.arguments[main + 1], "--username");
        assert_eq!(command.arguments[main + 2], "Steve");
    }

    #[test]
    fn the_choice_of_the_player_replaces_the_collector_of_the_version() {
        let mut request = request();
        request.extra_jvm_args = vec!["-XX:+UseG1GC".to_string()];
        let command = build_command(&request, platform(), &FeatureSet::none(), "release").unwrap();

        // The JVM refuses to start when two collectors are named at once.
        let collectors: Vec<&String> = command
            .arguments
            .iter()
            .filter(|argument| ml_core::config::is_collector_flag(argument))
            .collect();
        assert_eq!(collectors, vec!["-XX:+UseG1GC"]);
    }

    #[test]
    fn the_heap_of_the_player_wins_over_the_heap_of_the_version() {
        let mut request = request();
        request.extra_jvm_args = vec!["-Xmx8G".to_string()];
        let command = build_command(&request, platform(), &FeatureSet::none(), "release").unwrap();

        // A player who names one half of the pair owns both halves: the
        // initial heap of the version would outgrow the maximum the player
        // chose, and the virtual machine refuses such a combination.
        let heaps: Vec<&str> = command
            .arguments
            .iter()
            .map(String::as_str)
            .filter(|argument| argument.starts_with("-Xm"))
            .collect();
        assert_eq!(heaps, vec!["-Xmx8G"]);
    }

    #[test]
    fn the_heap_of_the_version_survives_when_the_player_names_none() {
        let mut request = request();
        request.extra_jvm_args = vec!["-XX:+UseG1GC".to_string()];
        let command = build_command(&request, platform(), &FeatureSet::none(), "release").unwrap();

        let heaps: Vec<&str> = command
            .arguments
            .iter()
            .map(String::as_str)
            .filter(|argument| argument.starts_with("-Xm"))
            .collect();
        assert_eq!(heaps, vec!["-Xms2G", "-Xmx4G"]);
    }

    #[test]
    fn a_version_that_names_its_own_library_path_gets_no_other_directory() {
        // The document of 1.14 to 1.19 names one directory for the natives
        // and no extract path: the game searches `java.library.path` and
        // nothing else. A launcher that adds its own directories unpacks
        // the libraries where the game will never look for them.
        let old = r#"{
            "id": "1.18.2", "type": "release",
            "mainClass": "net.minecraft.client.main.Main", "assets": "8",
            "arguments": {
                "jvm": [ "-Djava.library.path=${natives_directory}", "-cp", "${classpath}" ],
                "game": [ "--username", "${auth_player_name}" ]
            }
        }"#;
        let mut request = request();
        request.version = VersionJson::parse(old.as_bytes()).unwrap();
        let command = build_command(&request, platform(), &FeatureSet::none(), "release").unwrap();

        let directories: Vec<&str> = command
            .arguments
            .iter()
            .map(String::as_str)
            .filter(|argument| {
                argument.starts_with("-D")
                    && (argument.contains("library.path")
                        || argument.contains("ExtractPath")
                        || argument.contains("tmpdir")
                        || argument.contains("workdir"))
            })
            .collect();
        assert_eq!(directories.len(), 1, "{directories:?}");
        assert!(
            directories[0].starts_with("-Djava.library.path="),
            "{directories:?}"
        );
    }

    #[test]
    fn the_placeholders_are_replaced() {
        let command =
            build_command(&request(), platform(), &FeatureSet::none(), "release").unwrap();
        let joined = command.arguments.join(" ");

        assert!(!joined.contains("${auth_player_name}"));
        assert!(!joined.contains("${classpath}"));
        assert!(!joined.contains("${game_directory}"));
        assert!(joined.contains("--assetIndex 34"), "{joined}");
        assert!(joined.contains("--versionType release"));
    }

    #[test]
    fn the_classpath_holds_the_client_jar() {
        let command =
            build_command(&request(), platform(), &FeatureSet::none(), "release").unwrap();
        let index = command.arguments.iter().position(|a| a == "-cp").unwrap();
        let classpath = &command.arguments[index + 1];
        assert!(classpath.starts_with("C:/versions/26.3/client.jar"));
    }

    #[test]
    fn the_natives_layout_follows_the_arguments() {
        let command =
            build_command(&request(), platform(), &FeatureSet::none(), "release").unwrap();

        assert_eq!(
            command.natives_layout.directory_for(NativeFamily::System),
            PathBuf::from("C:/games/mc/natives/java")
        );
        assert_eq!(
            command.natives_layout.directory_for(NativeFamily::Lwjgl),
            PathBuf::from("C:/games/mc/natives/lwjgl")
        );
        assert_eq!(
            command.natives_layout.directory_for(NativeFamily::Jna),
            PathBuf::from("C:/games/mc/natives/jna")
        );
        assert_eq!(
            command.natives_layout.directory_for(NativeFamily::Netty),
            PathBuf::from("C:/games/mc/natives/netty")
        );
    }

    #[test]
    fn the_agent_is_added_when_it_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let agent = dir.path().join("agent.jar");
        std::fs::write(&agent, b"x").unwrap();

        let mut request = request();
        request.agent_jar = Some(agent.clone());

        let command = build_command(&request, platform(), &FeatureSet::none(), "release").unwrap();
        assert!(
            command
                .arguments
                .iter()
                .any(|argument| argument.starts_with("-javaagent:"))
        );
    }

    #[test]
    fn the_path_of_the_report_travels_in_a_property() {
        let dir = tempfile::tempdir().unwrap();
        let agent = dir.path().join("agent.jar");
        std::fs::write(&agent, b"x").unwrap();

        let mut request = request();
        request.agent_jar = Some(agent);
        request.agent_report = Some(PathBuf::from("C:/logs/a=b.json"));

        let command = build_command(&request, platform(), &FeatureSet::none(), "release").unwrap();

        // The path of the report has an '=' in it. It must not travel
        // in the options of the agent: the JVM would cut the jar path
        // in half at that sign and the agent would not load.
        let property = command
            .arguments
            .iter()
            .find(|argument| argument.starts_with("-Dminlaucher.agent.output="))
            .expect("the property is not there");
        assert_eq!(property, "-Dminlaucher.agent.output=C:/logs/a=b.json");

        // And the argument of the agent itself carries nothing but the
        // path of the jar.
        let agent_argument = command
            .arguments
            .iter()
            .find(|argument| argument.starts_with("-javaagent:"))
            .expect("the agent is not there");
        assert!(!agent_argument.contains('='), "{agent_argument}");
    }

    #[test]
    fn a_version_without_a_main_class_is_rejected() {
        let mut request = request();
        request.version.main_class = None;
        let error =
            build_command(&request, platform(), &FeatureSet::none(), "release").unwrap_err();
        assert!(matches!(error, Error::InvalidInput(_)));
    }

    #[test]
    fn the_libraries_reach_the_classpath() {
        let mut request = request();
        request.version.libraries = vec![Library {
            name: "com.google.code.gson:gson:2.14.0".into(),
            artifact: None,
            downloads: ml_minecraft::version::LibraryDownloads {
                artifact: Some(ml_minecraft::version::FileDownload {
                    path: String::new(),
                    sha1: "abc".into(),
                    size: 1,
                    url: "https://example.invalid/gson.jar".into(),
                }),
                classifiers: BTreeMap::default(),
            },
            rules: Vec::new(),
        }];
        request.libraries = request.version.resolve_libraries(platform(), Some("26.3"));

        let entries = request.classpath_entries();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].ends_with("gson-2.14.0.jar"), "{entries:?}");
    }

    #[test]
    fn the_natives_jars_are_collected() {
        let mut request = request();
        request.version.libraries = vec![Library {
            name: "org.lwjgl:lwjgl:3.4.3:natives-windows".into(),
            artifact: None,
            downloads: ml_minecraft::version::LibraryDownloads {
                artifact: Some(ml_minecraft::version::FileDownload {
                    path: String::new(),
                    sha1: "abc".into(),
                    size: 1,
                    url: "https://example.invalid/lwjgl.jar".into(),
                }),
                classifiers: BTreeMap::default(),
            },
            rules: Vec::new(),
        }];
        request.libraries = request.version.resolve_libraries(platform(), Some("26.3"));

        let jars = request.native_jars(platform());
        assert_eq!(jars.len(), 1);
        assert_eq!(jars[0].family, NativeFamily::Lwjgl);
        assert!(jars[0].path.ends_with("lwjgl-3.4.3-natives-windows.jar"));
    }

    /// A request with the Fabric loader in front of the game.
    fn loader_request() -> LaunchRequest {
        let profile = ml_minecraft::fabric::FabricProfile::parse(
            br#"{
              "id": "fabric-loader-0.19.5-1.21.4",
              "inheritsFrom": "1.21.4",
              "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
              "arguments": { "jvm": ["-DFabricMcEmu= net.minecraft.client.main.Main "] },
              "libraries": [
                { "name": "net.fabricmc:fabric-loader:0.19.5", "url": "https://maven.fabricmc.net/" },
                { "name": "net.fabricmc:intermediary:1.21.4", "url": "https://maven.fabricmc.net/" }
              ]
            }"#,
        )
        .unwrap();

        let mut request = request();
        request.loader = Some(ml_minecraft::fabric::FabricLaunch::from_profile(
            "1.21.4", "0.19.5", &profile,
        ));
        request.mod_jars = vec![PathBuf::from("C:/games/mc/mods/minlaucher.jar")];
        request
    }

    #[test]
    fn the_loader_is_the_class_the_game_is_started_with() {
        let command = build_command(
            &loader_request(),
            platform(),
            &FeatureSet::none(),
            "release",
        )
        .unwrap();

        let main = command
            .arguments
            .iter()
            .position(|argument| argument == "net.fabricmc.loader.impl.launch.knot.KnotClient")
            .expect("the class of the loader is not there");

        // The class of the game is not started directly any more.
        assert!(
            !command
                .arguments
                .iter()
                .any(|argument| argument == "net.minecraft.client.main.Main")
        );
        assert!(
            command.arguments[main + 1..]
                .iter()
                .any(|a| a == "--username")
        );
    }

    #[test]
    fn the_libraries_of_the_loader_are_on_the_classpath() {
        let request = loader_request();
        let entries = request.classpath_entries();

        assert!(
            entries
                .iter()
                .any(|entry| entry.ends_with("fabric-loader-0.19.5.jar")),
            "{entries:?}"
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.ends_with("intermediary-1.21.4.jar")),
            "{entries:?}"
        );

        // The libraries of the loader come before the libraries of the
        // game, so that the loader finds its own classes first.
        let loader = entries
            .iter()
            .position(|entry| entry.ends_with("fabric-loader-0.19.5.jar"))
            .unwrap();
        assert_eq!(loader, 0);
    }

    #[test]
    fn the_library_of_the_loader_replaces_the_one_of_the_game() {
        // The game of 1.21.4 carries ASM 9.6 and the loader carries
        // ASM 9.10.1. Both of them have to be on the classpath, and
        // only one of them may be there: the loader refuses to start
        // when two jars carry the same class.
        let mut request = loader_request();
        request.version.libraries = vec![
            Library {
                name: "org.ow2.asm:asm:9.6".into(),
                artifact: None,
                downloads: ml_minecraft::version::LibraryDownloads {
                    artifact: Some(ml_minecraft::version::FileDownload {
                        path: String::new(),
                        sha1: "abc".into(),
                        size: 1,
                        url: "https://example.invalid/asm-9.6.jar".into(),
                    }),
                    classifiers: BTreeMap::default(),
                },
                rules: Vec::new(),
            },
            Library {
                name: "org.ow2.asm:asm:9.10.1".into(),
                artifact: None,
                downloads: ml_minecraft::version::LibraryDownloads {
                    artifact: Some(ml_minecraft::version::FileDownload {
                        path: String::new(),
                        sha1: "abc".into(),
                        size: 1,
                        url: "https://example.invalid/asm-9.10.1.jar".into(),
                    }),
                    classifiers: BTreeMap::default(),
                },
                rules: Vec::new(),
            },
        ];
        request.libraries = request.version.resolve_libraries(platform(), Some("26.3"));

        // The loader carries ASM too, and it has to be the one that
        // stays: the game does not know about it.
        let profile = ml_minecraft::fabric::FabricProfile::parse(
            br#"{
              "id": "fabric-loader-0.19.5-1.21.4",
              "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
              "libraries": [
                { "name": "org.ow2.asm:asm:9.10.1", "url": "https://maven.fabricmc.net/" }
              ]
            }"#,
        )
        .unwrap();
        request.loader = Some(ml_minecraft::fabric::FabricLaunch::from_profile(
            "1.21.4", "0.19.5", &profile,
        ));

        let entries = request.classpath_entries();

        // Exactly one ASM on the classpath, and it is the one of the
        // loader.
        let asm: Vec<&PathBuf> = entries
            .iter()
            .filter(|entry| entry.to_string_lossy().contains("/asm/"))
            .collect();
        assert_eq!(asm.len(), 1, "{entries:?}");
        assert!(asm[0].ends_with("asm-9.10.1.jar"), "{asm:?}");
    }

    #[test]
    fn the_jar_of_the_game_is_pointed_at() {
        let command = build_command(
            &loader_request(),
            platform(),
            &FeatureSet::none(),
            "release",
        )
        .unwrap();

        assert!(
            command
                .arguments
                .iter()
                .any(|argument| argument
                    == "-Dfabric.gameJarPath.client=C:/versions/26.3/client.jar"),
            "{:?}",
            command.arguments
        );
    }

    #[test]
    fn the_mods_are_handed_to_the_loader() {
        let command = build_command(
            &loader_request(),
            platform(),
            &FeatureSet::none(),
            "release",
        )
        .unwrap();

        assert!(
            command
                .arguments
                .iter()
                .any(|argument| argument == "-Dfabric.addMods=C:/games/mc/mods/minlaucher.jar"),
            "{:?}",
            command.arguments
        );
    }

    #[test]
    fn the_arguments_of_the_loader_are_added() {
        let command = build_command(
            &loader_request(),
            platform(),
            &FeatureSet::none(),
            "release",
        )
        .unwrap();

        assert!(
            command
                .arguments
                .iter()
                .any(|argument| argument.starts_with("-DFabricMcEmu=")),
            "{:?}",
            command.arguments
        );
    }

    /// A version of the times before 1.13.
    ///
    /// Such a document has no `arguments` block at all: it spells its
    /// arguments as one string, and it names neither the classpath nor
    /// the directories of the natives.
    const LEGACY_1_12_2: &str = r#"{
        "id": "1.12.2",
        "type": "release",
        "mainClass": "net.minecraft.client.main.Main",
        "assets": "5",
        "minecraftArguments": "--username ${auth_player_name} --version ${version_name} --gameDir ${game_directory} --assetsDir ${assets_root} --assetIndex ${assets_index_name} --uuid ${auth_uuid} --accessToken ${auth_access_token} --userType ${user_type} --versionType ${version_type}"
    }"#;

    /// A request for a version of the times before 1.13.
    fn legacy_request() -> LaunchRequest {
        let mut request = request();
        request.version = VersionJson::parse(LEGACY_1_12_2.as_bytes()).unwrap();
        request.session.version_name = "1.12.2".into();
        request.session.user_type = "msa".into();
        request.client_jar = PathBuf::from("C:/versions/1.12.2/client.jar");
        request
    }

    #[test]
    fn a_legacy_version_gets_the_arguments_its_document_prescribes() {
        let command = build_command(
            &legacy_request(),
            platform(),
            &FeatureSet::none(),
            "release",
        )
        .unwrap();

        let main = command
            .arguments
            .iter()
            .position(|argument| argument == "net.minecraft.client.main.Main")
            .expect("the class of the game is not there");

        // The arguments of the game are the ones after the class.
        let game = &command.arguments[main + 1..];
        assert_eq!(game[0], "--username");
        assert_eq!(game[1], "Steve");
        assert!(game.contains(&"--versionType".to_string()), "{game:?}");
        assert!(game.contains(&"release".to_string()), "{game:?}");
        assert!(
            !game.iter().any(|argument| argument.contains("${")),
            "{game:?}"
        );
    }

    #[test]
    fn a_legacy_version_gets_the_classpath_and_the_natives() {
        // The document of such a version names neither the classpath
        // nor the directories of the natives: without them the JVM does
        // not find the game at all.
        let command = build_command(
            &legacy_request(),
            platform(),
            &FeatureSet::none(),
            "release",
        )
        .unwrap();

        let index = command
            .arguments
            .iter()
            .position(|argument| argument == "-cp")
            .expect("the classpath is not there");
        assert!(
            command.arguments[index + 1].starts_with("C:/versions/1.12.2/client.jar"),
            "{}",
            command.arguments[index + 1]
        );

        // The directories are the ones the natives are unpacked into.
        for property in [
            "java.library.path",
            "jna.tmpdir",
            "org.lwjgl.system.SharedLibraryExtractPath",
            "io.netty.native.workdir",
        ] {
            assert!(
                command
                    .arguments
                    .iter()
                    .any(|argument| argument.starts_with(&format!("-D{property}="))),
                "{property} is not there"
            );
        }

        // And the layout of the natives follows them.
        assert_eq!(
            command.natives_layout.directory_for(NativeFamily::System),
            PathBuf::from("C:/games/mc/natives/java")
        );
        assert_eq!(
            command.natives_layout.directory_for(NativeFamily::Lwjgl),
            PathBuf::from("C:/games/mc/natives/lwjgl")
        );
    }

    #[test]
    fn a_modern_version_is_left_alone() {
        // The document of a modern version names the classpath and all
        // four directories itself: the launcher adds nothing.
        let command =
            build_command(&request(), platform(), &FeatureSet::none(), "release").unwrap();

        let classpaths = command
            .arguments
            .iter()
            .filter(|argument| *argument == "-cp")
            .count();
        assert_eq!(classpaths, 1);

        assert!(
            command
                .arguments
                .contains(&"-Djava.library.path=C:/games/mc/natives/java".to_string())
        );
    }

    #[test]
    fn a_game_without_a_loader_starts_the_game_itself() {
        let command =
            build_command(&request(), platform(), &FeatureSet::none(), "release").unwrap();

        assert!(
            command
                .arguments
                .iter()
                .any(|argument| argument == "net.minecraft.client.main.Main")
        );
        assert!(
            !command
                .arguments
                .iter()
                .any(|argument| argument.starts_with("-Dfabric."))
        );
    }
}
