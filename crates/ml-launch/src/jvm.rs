//! Java runtime discovery.
//!
//! A version needs a specific Java feature release: 26.3 needs Java 25,
//! 1.20.5 needs Java 21, 1.16.5 needs Java 8. The launcher looks for a
//! runtime in this order:
//!
//! 1. the runtime the user pointed at explicitly,
//! 2. the Mojang runtime the launcher downloaded itself,
//! 3. a Java installation found on the system.
//!
//! A candidate is only accepted when its version is new enough, so a
//! system Java 8 never gets picked for a version that needs Java 25.

use std::path::{Path, PathBuf};
use std::process::Command;

use ml_core::{AppPaths, Error, JavaRuntime, Result};

/// A Java installation the launcher can use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaInstallation {
    /// The `java` executable.
    pub executable: PathBuf,
    /// The `javaw` executable, when the platform has one.
    ///
    /// On Windows the game is started with `javaw.exe` so that no console
    /// window pops up next to the game.
    pub windowless_executable: Option<PathBuf>,
    /// The Java feature release, e.g. `25`.
    pub major_version: u32,
    /// The full version string, e.g. `25.0.1`.
    pub version: String,
    /// Where the installation came from.
    pub origin: JavaOrigin,
}

/// Where a Java installation was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaOrigin {
    /// The user named the installation explicitly.
    User,
    /// A runtime the launcher downloaded from Mojang.
    Mojang,
    /// A Java installation found on the system.
    System,
}

impl JavaOrigin {
    /// A short, human readable description.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::User => "выбран вручную",
            Self::Mojang => "загружен Mojang",
            Self::System => "системный",
        }
    }
}

/// The name of the `java` executable on this platform.
#[must_use]
pub fn java_executable_name() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}

/// The name of the console-free `java` executable on this platform.
#[must_use]
pub fn javaw_executable_name() -> &'static str {
    if cfg!(windows) { "javaw.exe" } else { "java" }
}

/// The `java` executable inside a Java installation directory.
#[must_use]
pub fn java_in_home(home: &Path) -> PathBuf {
    home.join("bin").join(java_executable_name())
}

/// The `javaw` executable inside a Java installation directory.
#[must_use]
pub fn javaw_in_home(home: &Path) -> PathBuf {
    home.join("bin").join(javaw_executable_name())
}

/// The Java version reported by a `java` executable.
///
/// The `java -version` output goes to standard error and looks like
///
/// ```text
/// openjdk version "25.0.1" 2025-12-10 LTS
/// ```
///
/// so the version is read from there.
///
/// # Errors
/// Fails if the executable cannot be run or prints nothing that looks
/// like a version.
pub fn probe_version(executable: &Path) -> Result<(u32, String)> {
    let output = Command::new(executable)
        .arg("-version")
        .output()
        .map_err(|e| Error::io(executable, e))?;

    // `java -version` writes to stderr, but some wrappers use stdout.
    let text = if output.stderr.is_empty() {
        String::from_utf8_lossy(&output.stdout).into_owned()
    } else {
        String::from_utf8_lossy(&output.stderr).into_owned()
    };

    parse_java_version(&text).ok_or_else(|| {
        Error::InvalidInput(format!(
            "could not read a java version from `{}`",
            executable.display()
        ))
    })
}

/// Extract the version from the text `java -version` prints.
///
/// Recognises the `OpenJDK` and `Oracle` layouts:
///
/// ```text
/// openjdk version "25.0.1" 2025-12-10 LTS
/// java version "1.8.0_51"
/// ```
#[must_use]
pub fn parse_java_version(text: &str) -> Option<(u32, String)> {
    for line in text.lines() {
        let Some((_, rest)) = line.split_once("version \"") else {
            continue;
        };
        let Some((raw, _)) = rest.split_once('"') else {
            continue;
        };
        let major = if let Some(legacy) = raw.strip_prefix("1.") {
            legacy.split('.').next()?.parse::<u32>().ok()?
        } else {
            raw.split('.').next()?.parse::<u32>().ok()?
        };
        return Some((major, raw.to_string()));
    }
    None
}

/// The directories a system Java installation is looked for in.
#[must_use]
pub fn well_known_java_homes() -> Vec<PathBuf> {
    let mut homes = Vec::new();

    if let Some(home) = std::env::var_os("JAVA_HOME") {
        let home = PathBuf::from(home);
        if !home.as_os_str().is_empty() {
            homes.push(home);
        }
    }

    #[cfg(target_os = "windows")]
    {
        // The registry is consulted separately; these are the paths a
        // default installation ends up in.
        for program_files in ["Program Files", "Program Files (x86)"] {
            if let Some(base) = std::env::var_os(program_files) {
                let base = PathBuf::from(base).join("Java");
                let Ok(entries) = std::fs::read_dir(&base) else {
                    continue;
                };
                for entry in entries.flatten() {
                    homes.push(entry.path());
                }
            }
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            homes.push(
                PathBuf::from(local)
                    .join("Programs")
                    .join("Eclipse Adoptium"),
            );
        }
    }

    #[cfg(target_os = "macos")]
    {
        homes.push(PathBuf::from("/Library/Java/JavaVirtualMachines"));
    }

    #[cfg(target_os = "linux")]
    {
        homes.push(PathBuf::from("/usr/lib/jvm"));
    }

    homes
}

/// The Java installations the registry knows about, on Windows.
///
/// The registry is read without administrator rights, and each entry is
/// reported as the home directory of the installation.
#[must_use]
pub fn registry_java_homes() -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        use winreg::RegKey;
        use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};

        let mut homes = Vec::new();
        let roots = [
            (HKEY_LOCAL_MACHINE, r"SOFTWARE\JavaSoft\JDK"),
            (
                HKEY_LOCAL_MACHINE,
                r"SOFTWARE\JavaSoft\Java Runtime Environment",
            ),
            (HKEY_LOCAL_MACHINE, r"SOFTWARE\Eclipse Adoptium\JDK"),
            (HKEY_LOCAL_MACHINE, r"SOFTWARE\Amazon Corretto"),
        ];

        for (root, path) in roots {
            let Ok(key) =
                RegKey::predef(root).open_subkey_with_flags(path, KEY_READ | KEY_WOW64_64KEY)
            else {
                continue;
            };
            for name in key.enum_keys().flatten() {
                let Ok(sub) = key.open_subkey_with_flags(&name, KEY_READ | KEY_WOW64_64KEY) else {
                    continue;
                };
                if let Ok(home) = sub.get_value::<String, _>("JavaHome") {
                    homes.push(PathBuf::from(home));
                }
            }
        }
        homes
    }
    #[cfg(not(target_os = "windows"))]
    {
        Vec::new()
    }
}

/// The Java installations found on the `PATH`.
///
/// Every directory of `PATH` is checked for a `bin/java` executable.
#[must_use]
pub fn path_java_homes() -> Vec<PathBuf> {
    let Some(path) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    path.to_string_lossy()
        .split(if cfg!(windows) { ';' } else { ':' })
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| {
            let candidate = PathBuf::from(entry)
                .join("bin")
                .join(java_executable_name());
            candidate.is_file().then(|| PathBuf::from(entry))
        })
        .collect()
}

/// The Mojang runtime the launcher downloaded, if any.
#[must_use]
pub fn mojang_runtime_home(paths: &AppPaths, runtime: &JavaRuntime) -> PathBuf {
    paths.jre_dir_for(&runtime.component)
}

/// Find a Java installation that satisfies the required major version.
///
/// The candidates are tried in the order the user expects: the runtime
/// the user pointed at, then the runtime the launcher downloaded itself,
/// then the installations of the system. The first candidate that is new
/// enough wins — the version document names the runtime it was tested
/// with, and a newer Java of the system is not a reason to ignore it.
///
/// Among the installations of the system the newest one wins, because
/// there is no order between them to prefer.
///
/// # Errors
/// Fails with [`Error::NoJavaRuntime`] when nothing suitable is found.
pub fn find_runtime(
    paths: &AppPaths,
    runtime: &JavaRuntime,
    explicit: Option<&Path>,
) -> Result<JavaInstallation> {
    // The first candidate that is new enough wins.
    for (home, origin) in preferred_candidates(paths, runtime, explicit) {
        if let Some(installation) = installation_at(&home, origin, runtime.major_version) {
            return Ok(installation);
        }
    }

    // The installations of the system have no order between them, so
    // the newest one wins.
    let mut best: Option<JavaInstallation> = None;
    for home in system_candidates() {
        let Some(installation) = installation_at(&home, JavaOrigin::System, runtime.major_version)
        else {
            continue;
        };
        let better = best
            .as_ref()
            .is_none_or(|current| installation.major_version > current.major_version);
        if better {
            best = Some(installation);
        }
    }

    best.ok_or(Error::NoJavaRuntime(runtime.major_version))
}

/// The candidates that have a name, in the order of the priority.
///
/// The first one is the Java the user pointed at, the second one the
/// runtime the launcher downloaded itself. The installations of the
/// system are not here: they have no order between them, and they are
/// only used when none of these is new enough.
fn preferred_candidates(
    paths: &AppPaths,
    runtime: &JavaRuntime,
    explicit: Option<&Path>,
) -> Vec<(PathBuf, JavaOrigin)> {
    let mut candidates: Vec<(PathBuf, JavaOrigin)> = Vec::new();

    if let Some(explicit) = explicit {
        candidates.push((explicit.to_path_buf(), JavaOrigin::User));
    }

    let mojang = mojang_runtime_home(paths, runtime);
    if mojang.join("bin").join(java_executable_name()).is_file() {
        candidates.push((mojang, JavaOrigin::Mojang));
    }

    candidates
}

/// The Java installations of the system.
fn system_candidates() -> Vec<PathBuf> {
    well_known_java_homes()
        .into_iter()
        .chain(registry_java_homes())
        .chain(path_java_homes())
        .collect()
}

/// Describe the Java installation in a directory, if it is new enough.
///
/// # Errors
/// Never fails: a directory without a Java, or with a Java that is too
/// old, is not an installation.
fn installation_at(
    home: &Path,
    origin: JavaOrigin,
    required_major: u32,
) -> Option<JavaInstallation> {
    let executable = java_in_home(home);
    if !executable.is_file() {
        return None;
    }
    let (major, version) = probe_version(&executable).ok()?;
    if major < required_major {
        return None;
    }

    let windowless = javaw_in_home(home);
    Some(JavaInstallation {
        executable,
        windowless_executable: windowless.is_file().then_some(windowless),
        major_version: major,
        version,
        origin,
    })
}

/// Describe a Java installation without running it.
///
/// The version is read from the `release` file that every modern Java
/// installation ships, which is much cheaper than starting a process —
/// and this runs on every launch, where the process would be noticeable.
#[must_use]
pub fn read_release_file(home: &Path) -> Option<(u32, String)> {
    let text = std::fs::read_to_string(home.join("release")).ok()?;
    for line in text.lines() {
        let Some(value) = line.strip_prefix("JAVA_VERSION=") else {
            continue;
        };
        let raw = value.trim().trim_matches('"');
        let major = if let Some(legacy) = raw.strip_prefix("1.") {
            legacy.split('.').next()?.parse::<u32>().ok()?
        } else {
            raw.split('.').next()?.parse::<u32>().ok()?
        };
        return Some((major, raw.to_string()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_openjdk_version_is_parsed() {
        let (major, version) =
            parse_java_version("openjdk version \"25.0.1\" 2025-12-10 LTS").unwrap();
        assert_eq!(major, 25);
        assert_eq!(version, "25.0.1");
    }

    #[test]
    fn the_legacy_version_is_parsed() {
        let (major, version) = parse_java_version("java version \"1.8.0_51\"").unwrap();
        assert_eq!(major, 8);
        assert_eq!(version, "1.8.0_51");
    }

    #[test]
    fn a_multi_line_output_is_parsed() {
        let text = "Picked up JAVA_TOOL_OPTIONS: -Xmx512m\nopenjdk version \"21.0.5\" 2024-10-15 LTS\nOpenJDK Runtime Environment";
        let (major, version) = parse_java_version(text).unwrap();
        assert_eq!(major, 21);
        assert_eq!(version, "21.0.5");
    }

    #[test]
    fn unparsable_output_is_rejected() {
        assert!(parse_java_version("no version here").is_none());
        assert!(parse_java_version("").is_none());
    }

    #[test]
    fn the_executable_name_follows_the_platform() {
        if cfg!(windows) {
            assert_eq!(java_executable_name(), "java.exe");
            assert_eq!(javaw_executable_name(), "javaw.exe");
        } else {
            assert_eq!(java_executable_name(), "java");
        }
    }

    #[test]
    fn the_executable_lives_in_bin() {
        let home = Path::new("/opt/jdk");
        assert!(
            java_in_home(home).ends_with("bin/java")
                || java_in_home(home).ends_with("bin/java.exe")
        );
    }

    #[test]
    fn the_release_file_is_read() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("release"),
            "JAVA_VERSION=\"21.0.5\"\nOS_NAME=\"Windows\"\n",
        )
        .unwrap();

        let (major, version) = read_release_file(dir.path()).unwrap();
        assert_eq!(major, 21);
        assert_eq!(version, "21.0.5");
    }

    #[test]
    fn a_missing_release_file_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_release_file(dir.path()).is_none());
    }

    #[test]
    fn the_mojang_runtime_lives_in_the_jre_directory() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let runtime = JavaRuntime::new("java-runtime-epsilon", 25);
        let home = mojang_runtime_home(&paths, &runtime);
        assert!(home.starts_with(&paths.jre_dir));
        assert!(home.ends_with("java-runtime-epsilon"));
    }

    #[test]
    fn the_candidates_are_tried_in_the_order_of_the_priority() {
        // The Java the user pointed at comes first, then the runtime
        // the launcher downloaded itself, and only then the installations
        // of the system.
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let runtime = JavaRuntime::new("java-runtime-epsilon", 21);

        // The runtime of the launcher, with a java in it.
        let home = paths.jre_dir_for("java-runtime-epsilon");
        std::fs::create_dir_all(home.join("bin")).unwrap();
        std::fs::write(home.join("bin").join(java_executable_name()), b"x").unwrap();

        let user = PathBuf::from("/opt/user-java");
        let candidates = preferred_candidates(&paths, &runtime, Some(&user));

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0], (user, JavaOrigin::User));
        assert_eq!(candidates[1].1, JavaOrigin::Mojang);
        assert_eq!(candidates[1].0, home);
    }

    #[test]
    fn a_runtime_that_is_not_there_is_not_a_candidate() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let runtime = JavaRuntime::new("java-runtime-epsilon", 21);

        assert!(
            preferred_candidates(&paths, &runtime, None).is_empty(),
            "there is no runtime to offer"
        );
    }

    #[test]
    fn an_impossible_requirement_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("cache"));
        let runtime = JavaRuntime::new("java-runtime-epsilon", 99);
        let error = find_runtime(&paths, &runtime, None).unwrap_err();
        assert!(matches!(error, Error::NoJavaRuntime(99)));
    }
}
