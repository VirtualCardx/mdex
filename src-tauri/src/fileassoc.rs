//! Windows file-association registration: lets mdex appear in (and become)
//! the default "Open with" handler for `.md` and `.mdex` files.
//!
//! Everything is written under `HKCU\Software\Classes`, so no administrator
//! rights are needed and the registration is fully reversible via
//! [`unregister`]. The NSIS installer registers the same associations at
//! install time through `bundle.fileAssociations` in tauri.conf.json.
//!
//! Windows 10/11 protects an existing "UserChoice" default with a hash, so
//! for extensions already claimed by another app we still land in the
//! "Open with" list and the user can confirm mdex as default from there.

#![cfg(windows)]

use std::io;

use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
use winreg::RegKey;

const PROG_ID: &str = "Mdex.Editor";
const EXTENSIONS: [&str; 2] = [".mdex", ".md"];
const CLASSES: &str = r"Software\Classes";

fn err(context: &str) -> impl Fn(io::Error) -> String + '_ {
    move |e| format!("{context}: {e}")
}

/// Create (or open) `HKCU\Software\Classes\<path>` with write access.
fn classes_subkey(path: &str) -> Result<RegKey, String> {
    RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(format!(r"{CLASSES}\{path}"))
        .map(|(key, _)| key)
        .map_err(err("cannot create registry key"))
}

/// Register the ProgID and hook both extensions. Idempotent.
pub fn register() -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("cannot resolve exe path: {e}"))?
        .to_string_lossy()
        .into_owned();

    let prog = classes_subkey(PROG_ID)?;
    prog.set_value("", &"Mdex Markdown Document")
        .map_err(err("cannot set ProgID description"))?;

    let icon = classes_subkey(&format!(r"{PROG_ID}\DefaultIcon"))?;
    icon.set_value("", &format!("\"{exe}\",0"))
        .map_err(err("cannot set ProgID icon"))?;

    let command = classes_subkey(&format!(r"{PROG_ID}\shell\open\command"))?;
    command
        .set_value("", &format!("\"{exe}\" \"%1\""))
        .map_err(err("cannot set open command"))?;

    for ext in EXTENSIONS {
        // Only claim the extension default when no other ProgID owns it
        // (or it is ours already); we always join the OpenWith list.
        let key = classes_subkey(ext)?;
        let current: String = key.get_value("").unwrap_or_default();
        if current.is_empty() || current == PROG_ID {
            key.set_value("", &PROG_ID)
                .map_err(err("cannot set extension default"))?;
        }
        let open_with = classes_subkey(&format!(r"{ext}\OpenWithProgids"))?;
        open_with
            .set_value(PROG_ID, &"")
            .map_err(err("cannot join OpenWith list"))?;
    }

    notify_shell();
    Ok(())
}

/// Remove the registration written by [`register`].
pub fn unregister() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for ext in EXTENSIONS {
        if let Ok(key) = hkcu.open_subkey_with_flags(
            format!(r"{CLASSES}\{ext}"),
            KEY_READ | KEY_SET_VALUE,
        ) {
            let current: String = key.get_value("").unwrap_or_default();
            if current == PROG_ID {
                let _ = key.delete_value("");
            }
        }
        if let Ok(open_with) = hkcu.open_subkey_with_flags(
            format!(r"{CLASSES}\{ext}\OpenWithProgids"),
            KEY_READ | KEY_SET_VALUE,
        ) {
            let _ = open_with.delete_value(PROG_ID);
        }
    }
    let _ = hkcu.delete_subkey_all(format!(r"{CLASSES}\{PROG_ID}"));
    notify_shell();
    Ok(())
}

/// True when the ProgID exists and at least one extension points at us.
pub fn is_registered() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if hkcu
        .open_subkey_with_flags(
            format!(r"{CLASSES}\{PROG_ID}\shell\open\command"),
            KEY_READ,
        )
        .is_err()
    {
        return false;
    }
    EXTENSIONS.iter().any(|ext| {
        hkcu.open_subkey_with_flags(
            format!(r"{CLASSES}\{ext}\OpenWithProgids"),
            KEY_READ,
        )
        .map(|open_with| open_with.get_value::<String, _>(PROG_ID).is_ok())
        .unwrap_or(false)
    })
}

/// Tell Explorer that association data changed so icons/menus refresh now.
fn notify_shell() {
    use windows_sys::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED as i32, SHCNF_IDLIST, std::ptr::null(), std::ptr::null());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trips against the real (current-user) registry and cleans up.
    /// The exe path recorded is the test binary, which is harmless.
    #[ignore = "needs real HKCU access; run with `cargo test -- --ignored`"]
    #[test]
    fn file_association_round_trip() {
        register().expect("register should succeed");
        assert!(is_registered(), "should report registered");

        unregister().expect("unregister should succeed");
        assert!(!is_registered(), "should report unregistered");
    }
}
