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
}

impl LaunchRequest {
    /// The library jars that go on the classpath, in order.
    ///
    /// Every library that resolved contributes its artifact, the natives
    /// jars included: LWJGL finds its native inside the jar on the
    /// classpath and unpacks it itself.
    #[must_use]
    pub fn classpath_entries(&self) -> Vec<PathBuf> {
        self.libraries
            .iter()
            .filter_map(|library| {
                library.artifact.as_ref().map(|_| {
                    self.library_directory
                        .join(library.coordinate.artifact_path())
                })
            })
            .collect()
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
    let Some(main_class) = request.version.main_class.as_ref() else {
        return Err(Error::InvalidInput(format!(
            "version `{}` has no main class",
            request.version.id
        )));
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

    if let Some(agent) = &request.agent_jar
        && let Some(argument) = crate::args::java_agent(agent)
    {
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
}
