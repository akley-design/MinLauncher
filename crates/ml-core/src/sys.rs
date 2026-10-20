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
/// `CurrentVersion` holds `major.minor` and `CurrentBuildNumber` holds the
/// build. Both are needed: Mojang's bounds are written as
/// `10.0.17134`, which is `major.minor.build`.
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

    let major_minor: String = key.get_value("CurrentVersion").ok()?;
    let build: String = key.get_value("CurrentBuildNumber").ok()?;

    // `CurrentVersion` is already `major.minor`; the build is appended so
    // that the result can be compared segment by segment.
    let version = format!("{major_minor}.{build}");

    // The string has to outlive this function, and the answer is asked for
    // once per process, so leaking it here is the cheapest correct option.
    Some(Box::leak(version.into_boxed_str()))
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
}
