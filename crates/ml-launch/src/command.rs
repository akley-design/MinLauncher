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
//! <java> <user jvm args> <version jvm args> <main class> <game args>
//! ```
//!
//! The user arguments come first so that the version's own flags win:
//! a user who passes `-Xmx8G` gets `8G`, not the `4G` of the version.

use std::collections::BTreeMap;
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
    #[must_use]
    pub fn classpath_entries(&self) -> Vec<PathBuf> {
        let mut entries = Vec::new();

        if let Some(loader) = &self.loader {
            entries.extend(loader.libraries.iter().filter_map(|library| {
                library.artifact.as_ref().map(|_| {
                    self.library_directory
                        .join(library.coordinate.artifact_path())
                })
            }));
        }

        entries.extend(self.libraries.iter().filter_map(|library| {
            library.artifact.as_ref().map(|_| {
                self.library_directory
                    .join(library.coordinate.artifact_path())
            })
        }));

        entries
    }

    /// The native jars that have to be unpacked before the game starts.
    #[must_use]
    pub fn native_jars(&self, platform: Platform) -> Vec<NativeJar> {
        self.libraries
            .iter()
            .filter_map(|library| {
                // A library only contributes a native jar when the
                // platform's classifier is the one it ships.
                library.native_jar(platform)?;
                let family = NativeFamily::of_group(&library.coordinate.group);
                Some(NativeJar {
                    path: self
                        .library_directory
                        .join(library.coordinate.artifact_path()),
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
    /// The whole command line, for logging.
    #[must_use]
    pub fn display(&self) -> String {
        let mut parts = vec![self.executable.to_string_lossy().into_owned()];
        parts.extend(self.arguments.iter().cloned());
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
    let main_class = match &request.loader {
        Some(loader) => loader.main_class.clone(),
        None => {
            let Some(main_class) = request.version.main_class.as_ref() else {
                return Err(Error::InvalidInput(format!(
                    "version `{}` has no main class",
                    request.version.id
                )));
            };
            main_class.clone()
        }
    };

    let mut session = request.session.clone();
    session.version_type = version_type.to_string();

    // The classpath has to be known before the arguments are resolved,
    // because `${classpath}` is one of them.
    let classpath = build_classpath(&request.client_jar, &request.classpath_entries());
    let mut values: BTreeMap<&str, String> = session.placeholders();
    values.insert("classpath", classpath);

    // The JVM arguments, in the order the version prescribes.
    let mut jvm_arguments: Vec<String> = Vec::new();
    jvm_arguments.extend(request.extra_jvm_args.iter().cloned());

    let version_id = Some(request.version.id.as_str());
    jvm_arguments.extend(substitute_all(
        &request
            .version
            .arguments
            .resolve_default_user_jvm(platform, version_id, features),
        &values,
    ));
    jvm_arguments.extend(substitute_all(
        &request
            .version
            .arguments
            .resolve_jvm(platform, version_id, features),
        &values,
    ));

    // The arguments of the loader come after the arguments of the game,
    // so that the arguments of the game win where both of them say
    // something about the same thing.
    if let Some(loader) = &request.loader {
        jvm_arguments.extend(loader.jvm_arguments.iter().cloned());

        // The loader looks for the jar of the game. It finds the one on
        // the classpath on its own, but it is told where the jar is
        // anyway: the jar of the game is on the classpath under a name
        // of the launcher, and the loader should not have to guess.
        jvm_arguments.push(format!(
            "-Dfabric.gameJarPath.client={}",
            request.client_jar.display()
        ));

        // The mods are not put on the classpath: the loader loads them
        // itself, from the directory of the mods, so that it can
        // transform them before the game sees them.
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
        // The property that says where the report goes comes first, so
        // that it is set before the agent reads it. The agent reads it
        // in `premain`, which runs before the game starts, so the order
        // of the two arguments does not matter to the agent — but the
        // JVM sets the properties as it reads them, and this way the
        // property is already in place when the agent is loaded.
        if let Some(report) = &request.agent_report {
            jvm_arguments.push(crate::args::agent_output_property(report));
        }
        jvm_arguments.push(argument);
    }

    // The game arguments.
    let mut game_arguments = substitute_all(
        &request
            .version
            .arguments
            .resolve_game(platform, version_id, features),
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

/// The classpath the command uses, for diagnostics.
///
/// The classpath is either the argument after a standalone `-cp` or the
/// value of a `-cp=…` argument.
#[must_use]
pub fn classpath_of(command: &LaunchCommand) -> Vec<String> {
    let mut arguments = command.arguments.iter();
    while let Some(argument) = arguments.next() {
        if argument == "-cp"
            && let Some(value) = arguments.next()
        {
            return crate::args::split_classpath(value)
                .iter()
                .map(|entry| (*entry).to_string())
                .collect();
        }
        if let Some(value) = argument.strip_prefix("-cp=") {
            return crate::args::split_classpath(value)
                .iter()
                .map(|entry| (*entry).to_string())
                .collect();
        }
    }
    Vec::new()
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
    fn the_user_arguments_come_first() {
        let mut request = request();
        request.extra_jvm_args = vec!["-Xmx8G".into()];
        let command = build_command(&request, platform(), &FeatureSet::none(), "release").unwrap();

        assert_eq!(command.arguments[0], "-Xmx8G");
        // The version's own heap setting comes later and wins.
        let heap = command
            .arguments
            .iter()
            .position(|argument| argument == "-Xmx4G")
            .unwrap();
        assert!(heap > 0);
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
            downloads: ml_minecraft::version::LibraryDownloads {
                artifact: Some(ml_minecraft::version::FileDownload {
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
            downloads: ml_minecraft::version::LibraryDownloads {
                artifact: Some(ml_minecraft::version::FileDownload {
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
