//! Starting the launcher with Windows.
//!
//! Windows starts the programs that are written into the key
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. The key is in
//! the branch of the **user**, so writing into it needs no rights of an
//! administrator — which matters, because the launcher is not installed
//! with rights of an administrator either.
//!
//! The settings file is the truth: the record in the registry is
//! written from it every time the launcher starts, so a record that was
//! removed by hand — or by a cleaner of the system — comes back.
//!
//! On the other systems the launcher does not start itself. There is no
//! one place to write it down, and each of those systems has its own
//! way of doing it. The functions say so plainly instead of pretending
//! that the toggle does something.

use crate::error::{Error, Result};

/// The name of the value the launcher writes into the registry.
pub const VALUE_NAME: &str = "MinLauncher";

/// The key of the registry where Windows looks for the programs to start.
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Whether this system can start the launcher with itself.
///
/// The interface hides the toggle when the answer is no: a toggle that
/// does nothing is worse than no toggle at all.
#[must_use]
pub fn is_supported() -> bool {
    cfg!(windows)
}

/// Set the launcher to start with Windows, or to stop doing that.
///
/// # Errors
/// Fails when the registry cannot be written, or when the path of the
/// executable of the launcher cannot be found.
pub fn set_enabled(enabled: bool) -> Result<()> {
    #[cfg(windows)]
    {
        let value = if enabled { Some(command_line()?) } else { None };
        write_value(RUN_KEY, VALUE_NAME, value.as_deref())
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Ok(())
    }
}

/// The command Windows runs to start the launcher.
///
/// The path is quoted: a path of a program that is installed under
/// `C:\Program Files` has a space in it, and Windows splits the command
/// on spaces when the path is bare.
///
/// # Errors
/// Fails when the path of the executable cannot be found.
pub fn command_line() -> Result<String> {
    let path = std::env::current_exe().map_err(|error| Error::Other(Box::new(error)))?;
    Ok(format!("\"{}\"", path.display()))
}

/// Write a value into a key of the registry of the user, or remove it.
///
/// The key and the name are parameters, so that a test can write into a
/// key of its own and leave the machine as it found it.
#[cfg(windows)]
fn write_value(key: &str, name: &str, value: Option<&str>) -> Result<()> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(key)
        .map_err(|error| Error::Other(Box::new(error)))?;

    match value {
        Some(value) => key
            .set_value(name, &value)
            .map_err(|error| Error::Other(Box::new(error))),
        None => match key.delete_value(name) {
            Ok(()) => Ok(()),
            // The value was not there at all: the launcher is not set
            // to start with Windows, which is what was asked for.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(Error::Other(Box::new(error))),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The key the test writes into. It is a key of its own, under the
    /// namespace of the launcher, and the test removes the value it
    /// wrote. The empty key itself is left behind: the tests run in
    /// parallel, and one of them deleting the key under another one
    /// would be a race.
    const TEST_KEY: &str = r"Software\MinLauncher\tests\autostart";

    #[test]
    fn the_command_line_names_the_executable_of_the_launcher() {
        let command = command_line().unwrap();

        assert!(command.starts_with('"'), "{command}");
        assert!(command.ends_with('"'), "{command}");
        assert!(command.len() > 2, "{command}");
    }

    #[test]
    fn the_key_of_the_run_is_the_one_of_windows() {
        assert!(
            RUN_KEY.starts_with(r"Software\Microsoft\Windows"),
            "{RUN_KEY}"
        );
        assert!(RUN_KEY.ends_with("Run"), "{RUN_KEY}");
    }

    /// Read a value of the test key back.
    #[cfg(windows)]
    fn read_back(name: &str) -> Option<String> {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

        let opened = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(TEST_KEY, KEY_READ)
            .ok()?;
        opened.get_value::<String, _>(name).ok()
    }

    #[cfg(windows)]
    #[test]
    fn a_value_is_written_into_the_registry_and_read_back() {
        let name = "probe";
        let command = r#""C:\games\minlaucher.exe""#;

        write_value(TEST_KEY, name, Some(command)).unwrap();
        assert_eq!(read_back(name).as_deref(), Some(command));

        write_value(TEST_KEY, name, None).unwrap();
        assert!(read_back(name).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn a_value_that_is_not_there_is_not_a_failure() {
        assert!(read_back("never-written").is_none());

        // Removing a value that is not there is what the player asked
        // for, and it is not an error.
        write_value(TEST_KEY, "never-written", None).unwrap();
    }
}
