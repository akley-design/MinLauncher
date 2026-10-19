//! The launch pipeline against the real 26.3 version document.
//!
//! The fixture is the version JSON Mojang serves for 26.3, captured
//! verbatim. Running the whole pipeline against it checks the parts
//! that unit tests with hand-written fixtures cannot: the rule
//! evaluation on Mojang's own data, the placeholder set, the classpath
//! and the layout of the natives.
//!
//! If Mojang changes the document, this test is the one that notices.

use std::collections::BTreeSet;
use std::path::PathBuf;

use ml_core::{FeatureSet, OsArch, OsName, Platform};
use ml_launch::args::Session;
use ml_launch::command::{LaunchRequest, build_command};
use ml_launch::jvm::{JavaInstallation, JavaOrigin};
use ml_launch::natives::NativeFamily;
use ml_minecraft::library::native_classifier;
use ml_minecraft::version::VersionJson;

fn platform() -> Platform {
    Platform::new(OsName::Windows, OsArch::X86_64).with_os_version(Some("10.0.26300"))
}

fn real_version() -> VersionJson {
    let bytes = include_bytes!("fixtures/version_26.3.json");
    VersionJson::parse(bytes).expect("the captured 26.3 document must parse")
}

fn request(version: &VersionJson) -> LaunchRequest {
    let platform = platform();
    let libraries = version.resolve_libraries(platform, Some(&version.id));

    LaunchRequest {
        version: version.clone(),
        libraries,
        java: JavaInstallation {
            executable: PathBuf::from("C:/runtimes/java-runtime-epsilon/bin/java.exe"),
            windowless_executable: Some(PathBuf::from(
                "C:/runtimes/java-runtime-epsilon/bin/javaw.exe",
            )),
            major_version: 25,
            version: "25.0.1".into(),
            origin: JavaOrigin::Mojang,
        },
        session: Session {
            player_name: "Steve".into(),
            uuid: "069a79f4-44e9-4726-a5be-fca90e38aaf5".into(),
            uuid_prefix: "069a79f444e94726a5befca90e38aaf5".into(),
            access_token: "token".into(),
            xuid: "1234567890".into(),
            client_id: "00000000402b5328".into(),
            version_name: version.id.clone(),
            game_directory: PathBuf::from("C:/games/mc"),
            assets_root: PathBuf::from("C:/cache/assets"),
            assets_index_name: "34".into(),
            library_directory: PathBuf::from("C:/cache/libraries"),
            natives_directory: PathBuf::from("C:/games/mc/natives"),
            launcher_name: "MinLauncher".into(),
            launcher_version: "0.1.0".into(),
            ..Session::default()
        },
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
fn the_real_document_resolves_to_the_expected_libraries() {
    let version = real_version();
    let libraries = version.resolve_libraries(platform(), Some("26.3"));

    // The 26.3 document lists 114 libraries; 74 of them apply to
    // Windows on x86-64. The other 40 are the macOS and Linux variants
    // and the 32-bit Windows ones.
    assert_eq!(
        libraries.len(),
        74,
        "the number of libraries that apply to Windows x86-64 changed"
    );

    let names: BTreeSet<&str> = libraries.iter().map(|lib| lib.name.as_str()).collect();
    assert!(names.contains("com.mojang:jtracy:1.14.38"));
    assert!(names.contains("org.lwjgl:lwjgl:3.4.3"));

    // The macOS-only libraries must be gone.
    assert!(!names.contains("ca.weblite:java-objc-bridge:1.1"));
    assert!(!names.iter().any(|name| name.contains("natives-macos")));
    assert!(!names.iter().any(|name| name.contains("natives-linux")));

    // The Windows natives are there.
    assert!(names.contains("com.mojang:jtracy:1.14.38:natives-windows"));
    assert!(names.contains("org.lwjgl:lwjgl:3.4.3:natives-windows"));
}

#[test]
fn the_native_classifier_matches_the_platform() {
    assert_eq!(native_classifier(platform()), "natives-windows");
}

#[test]
fn the_command_line_is_complete_and_resolved() {
    let version = real_version();
    let request = request(&version);
    let command = build_command(&request, platform(), &FeatureSet::none(), "release").unwrap();

    // No placeholder survives.
    for argument in &command.arguments {
        assert!(
            !argument.contains("${"),
            "unresolved placeholder in `{argument}`"
        );
    }

    // The entry point separates the JVM arguments from the game ones.
    let main = command
        .arguments
        .iter()
        .position(|argument| argument == "net.minecraft.client.main.Main")
        .expect("the main class must be in the command line");

    let jvm = &command.arguments[..main];
    let game = &command.arguments[main + 1..];

    // The memory settings of the version.
    assert!(jvm.contains(&"-Xms2G".to_string()));
    assert!(jvm.contains(&"-Xmx4G".to_string()));

    // The garbage collector of a modern Windows.
    assert!(
        jvm.contains(&"-XX:+UseZGC".to_string()),
        "a Windows 11 build must get ZGC"
    );
    assert!(
        !jvm.contains(&"-XX:+UseG1GC".to_string()),
        "the legacy collector must not be applied on top of ZGC"
    );

    // The modern flags.
    assert!(jvm.contains(&"-XX:StackShadowPages=32".to_string()));
    assert!(jvm.contains(&"--enable-native-access=ALL-UNNAMED".to_string()));

    // The game arguments.
    assert_eq!(game[0], "--username");
    assert_eq!(game[1], "Steve");
    assert!(game.contains(&"--version".to_string()));
    assert!(game.contains(&"26.3".to_string()));
    assert!(game.contains(&"--assetIndex".to_string()));
    assert!(game.contains(&"34".to_string()));
    assert!(game.contains(&"--versionType".to_string()));
    assert!(game.contains(&"release".to_string()));
    assert!(game.contains(&"--accessToken".to_string()));

    // The demo and the quick play flags stay out of a normal launch.
    assert!(!game.contains(&"--demo".to_string()));
    assert!(!game.iter().any(|a| a.starts_with("--quickPlay")));
    assert!(!game.contains(&"--width".to_string()));
}

#[test]
fn the_natives_use_the_four_directories_of_the_modern_layout() {
    let version = real_version();
    let request = request(&version);
    let command = build_command(&request, platform(), &FeatureSet::none(), "release").unwrap();

    let layout = &command.natives_layout;

    assert_eq!(
        layout.directory_for(NativeFamily::System),
        PathBuf::from("C:/games/mc/natives/java")
    );
    assert_eq!(
        layout.directory_for(NativeFamily::Jna),
        PathBuf::from("C:/games/mc/natives/jna")
    );
    assert_eq!(
        layout.directory_for(NativeFamily::Lwjgl),
        PathBuf::from("C:/games/mc/natives/lwjgl")
    );
    assert_eq!(
        layout.directory_for(NativeFamily::Netty),
        PathBuf::from("C:/games/mc/natives/netty")
    );
}

#[test]
fn the_classpath_holds_the_client_jar_and_every_library() {
    let version = real_version();
    let request = request(&version);
    let entries = request.classpath_entries();

    // One entry per library that resolved.
    assert_eq!(entries.len(), request.libraries.len());
    assert!(
        entries
            .iter()
            .any(|entry| entry.ends_with("com/mojang/jtracy/1.14.38/jtracy-1.14.38.jar"))
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry.ends_with("org/lwjgl/lwjgl/3.4.3/lwjgl-3.4.3.jar"))
    );

    // The natives jars are on the classpath: LWJGL finds its native
    // inside the jar and unpacks it itself.
    assert!(
        entries
            .iter()
            .any(|entry| entry.ends_with("lwjgl-3.4.3-natives-windows.jar"))
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry.ends_with("jtracy-1.14.38-natives-windows.jar"))
    );
}

#[test]
fn the_natives_jars_are_collected_for_unpacking() {
    let version = real_version();
    let request = request(&version);
    let jars = request.native_jars(platform());

    // Twenty-one native jars resolve on Windows: twenty LWJGL ones and
    // the jtracy one.
    assert_eq!(jars.len(), 21);

    let families: BTreeSet<NativeFamily> = jars.iter().map(|jar| jar.family).collect();
    assert!(families.contains(&NativeFamily::Lwjgl));
    assert!(families.contains(&NativeFamily::System));
}

#[test]
fn the_natives_of_both_architectures_are_unpacked() {
    // The rules of the version only look at the system, so the natives of
    // the other architecture resolve as well. The two jars hold their
    // binaries in directories of their own (`windows/x64` against
    // `windows/arm64`), so unpacking both is safe — and it is what makes
    // the same download set work on both architectures.
    let version = real_version();
    let request = request(&version);
    let jars = request.native_jars(platform());

    let names: Vec<String> = jars
        .iter()
        .map(|jar| jar.path.to_string_lossy().into_owned())
        .collect();

    assert!(
        names
            .iter()
            .any(|name| name.ends_with("lwjgl-3.4.3-natives-windows.jar")),
        "{names:?}"
    );
    assert!(
        names
            .iter()
            .any(|name| name.ends_with("lwjgl-3.4.3-natives-windows-arm64.jar")),
        "{names:?}"
    );

    // The natives of another system are never unpacked.
    assert!(!names.iter().any(|name| name.contains("natives-linux")));
    assert!(!names.iter().any(|name| name.contains("natives-macos")));
}

#[test]
fn the_version_requires_java_25() {
    let version = real_version();
    let runtime = version.java_runtime();
    assert_eq!(runtime.major_version, 25);
    assert_eq!(runtime.component, "java-runtime-epsilon");
}

#[test]
fn the_entry_point_and_the_asset_index_are_read() {
    let version = real_version();
    assert_eq!(
        version.main_class.as_deref(),
        Some("net.minecraft.client.main.Main")
    );
    assert_eq!(version.assets.as_deref(), Some("34"));
    assert_eq!(
        version.asset_index.as_ref().map(|index| index.id.as_str()),
        Some("34")
    );
}
