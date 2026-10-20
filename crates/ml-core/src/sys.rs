//! Host system queries.
//!
//! The launcher needs to know two things about the machine it runs on that
//! cannot be derived from the executable itself:
//!
//! * the **operating system version** — Mojang's rules compare against it
//!   with a regular expression (`^10\.`) and with inclusive bounds
//!   (`10.0.17134`), and
//! * the **Java runtime** that is already installed, so a user who has a
//!   suitable JDK does not have to download one.
//!
//! The version is read once and cached, because the registry round-trip is
//! not free and the answer cannot change while the launcher runs.

use std::sync::OnceLock;

use crate::rules::OsName;

/// The cached operating system version.
static OS_VERSION: OnceLock<Option<&'static str>> = OnceLock::new();

/// The cached name of the processor.
static CPU_NAME: OnceLock<Option<&'static str>> = OnceLock::new();

/// The operating system version of the host, e.g. `10.0.22621`.
///
/// The string is `major.minor.build` on Windows, which is the shape
/// Mojang's `versionRange` bounds use. On the other systems the value is
/// whatever the platform reports; no shipped rule constrains a macOS or
/// Linux version by range, so the exact format does not matter there.
#[must_use]
pub fn os_version() -> Option<&'static str> {
    *OS_VERSION.get_or_init(detect_os_version)
}

/// The version of the operating system, detected once.
fn detect_os_version() -> Option<&'static str> {
    #[cfg(target_os = "windows")]
    {
        windows_version()
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// Read the Windows version from the registry.
///
/// `CurrentMajorVersionNumber`, `CurrentMinorVersionNumber` and
/// `CurrentBuildNumber` hold the version, and Mojang's bounds are
/// written as `10.0.17134`, which is `major.minor.build`.
///
/// The key is readable without administrative rights.
#[cfg(target_os = "windows")]
fn windows_version() -> Option<&'static str> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};

    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            KEY_READ | KEY_WOW64_64KEY,
        )
        .ok()?;

    let major = key.get_value::<u32, _>("CurrentMajorVersionNumber").ok();
    let minor = key.get_value::<u32, _>("CurrentMinorVersionNumber").ok();
    let current = key.get_value::<String, _>("CurrentVersion").ok();
    let build = registry_number(&key, "CurrentBuildNumber");

    let version = windows_version_from(major, minor, current.as_deref(), build)?;

    // The string has to outlive this function, and the answer is asked for
    // once per process, so leaking it here is the cheapest correct option.
    Some(Box::leak(version.into_boxed_str()))
}

/// The number in a registry value, whatever the type it is stored under.
///
/// Windows is not consistent here: `CurrentBuildNumber` is a number on some
/// builds and a string on others. A value that cannot be read as a number is
/// no number at all — and a launcher that has no version answers wrongly
/// rather than not at all.
#[cfg(target_os = "windows")]
fn registry_number(key: &winreg::RegKey, name: &str) -> Option<u32> {
    if let Ok(value) = key.get_value::<u32, _>(name) {
        return Some(value);
    }
    key.get_value::<String, _>(name)
        .ok()
        .and_then(|value| value.trim().parse().ok())
}

/// The version of Windows, from the parts the registry gives.
///
/// `CurrentVersion` lies on Windows 11: it holds `6.3` there, for the sake
/// of the programs that were written for Windows 8. The real version is in
/// `CurrentMajorVersionNumber` and `CurrentMinorVersionNumber` — and those
/// keys are missing on the Windows before 10, where the build number says
/// the version anyway: every Windows since Windows 10 is `10.0`.
///
/// A wrong version is not a cosmetic mistake: the version of the game
/// chooses its garbage collector by the version of Windows, and the wrong
/// branch asks the JVM for a flag it no longer knows.
#[cfg(target_os = "windows")]
fn windows_version_from(
    major: Option<u32>,
    minor: Option<u32>,
    current: Option<&str>,
    build: Option<u32>,
) -> Option<String> {
    let build = build?;
    let (major, minor) = match (major, minor) {
        (Some(major), Some(minor)) => (major, minor),
        // Windows 10 and later are all `10.0`, whatever the registry says
        // about the version.
        _ if build >= 10_240 => (10, 0),
        // Before Windows 10 the registry is the only source there is.
        _ => {
            let (major, minor) = current?.split_once('.')?;
            (major.parse().ok()?, minor.parse().ok()?)
        }
    };
    Some(format!("{major}.{minor}.{build}"))
}

/// The name of the operating system the launcher runs on.
///
/// This mirrors [`crate::rules::OsName`]; it exists so that callers that
/// only need the name do not have to import the rules module.
#[must_use]
pub fn os_name() -> OsName {
    crate::rules::Platform::current().os
}

/// The number of usable CPU cores.
///
/// The launcher uses this to size its download pool. It returns at least 1,
/// because a machine that reports 0 cores would otherwise produce a
/// division by zero further down.
#[must_use]
pub fn cpu_count() -> usize {
    std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
}

/// The name of the processor of the host, e.g. `AMD Ryzen 7 5800X`.
///
/// The numbers of a measurement mean nothing without the machine they
/// were taken on, so a record carries the name of the processor with it.
/// On Windows the name is in the registry; on the other systems there is
/// no such name to read and the answer is `None`.
#[must_use]
pub fn cpu_name() -> Option<&'static str> {
    *CPU_NAME.get_or_init(detect_cpu_name)
}

/// The name of the processor, detected once.
fn detect_cpu_name() -> Option<&'static str> {
    #[cfg(target_os = "windows")]
    {
        windows_cpu_name()
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// Read the name of the processor from the registry.
///
/// The name is under the description of the system, and the key is
/// readable without administrative rights.
#[cfg(target_os = "windows")]
fn windows_cpu_name() -> Option<&'static str> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};

    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0", KEY_READ)
        .ok()?;

    let name: String = key.get_value("ProcessorNameString").ok()?;
    let name = name.trim();

    if name.is_empty() {
        return None;
    }

    // The string has to outlive this function, and the answer is asked
    // for once per process, so leaking it here is the cheapest correct
    // option.
    Some(Box::leak(name.to_string().into_boxed_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_count_is_at_least_one() {
        assert!(cpu_count() >= 1);
    }

    #[test]
    fn os_version_is_stable() {
        // The version is cached, so two calls have to agree.
        assert_eq!(os_version(), os_version());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_version_has_three_segments() {
        let version = os_version().expect("windows reports a version");
        let segments: Vec<&str> = version.split('.').collect();
        assert_eq!(segments.len(), 3, "unexpected version {version}");
        assert!(
            segments
                .iter()
                .all(|segment| segment.parse::<u64>().is_ok())
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn the_processor_has_a_name() {
        let name = cpu_name().expect("windows reports a processor");
        assert_ne!(name, "");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn the_windows_version_is_not_the_lie_of_the_registry() {
        // Windows 11 writes `6.3` into `CurrentVersion`, and a launcher
        // that believes it thinks it runs on Windows 8.
        assert_eq!(
            windows_version_from(Some(10), Some(0), Some("6.3"), Some(26300)).as_deref(),
            Some("10.0.26300")
        );
        // Without the modern keys the build number says the version.
        assert_eq!(
            windows_version_from(None, None, Some("6.3"), Some(26300)).as_deref(),
            Some("10.0.26300")
        );
        // Before Windows 10 the registry is the only source there is.
        assert_eq!(
            windows_version_from(None, None, Some("6.3"), Some(9600)).as_deref(),
            Some("6.3.9600")
        );
    }
}
