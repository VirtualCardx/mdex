//! Windows file-association registration: lets mdex appear in (and become)
//! the default "Open with" handler for `.md` and `.mdex` files, and adds
//! Explorer "New" context-menu entries for both extensions (`ShellNew`):
//! `.mdex` is created from a minimal embedded template so the new file is a
//! valid archive, `.md` starts as an empty file.
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

use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_BINARY};
use winreg::{RegKey, RegValue};

/// Per-extension registration data. Each extension gets its own ProgID so
/// the Explorer "New" context-menu entries have distinct, stable labels no
/// matter which other applications are installed.
struct FileType {
    ext: &'static str,
    prog_id: &'static str,
    description: &'static str,
}

const CLASSES: &str = r"Software\Classes";

const FILE_TYPES: [FileType; 2] = [
    FileType {
        ext: ".mdex",
        prog_id: "Mdex.Editor",
        description: "Mdex Archive Document",
    },
    FileType {
        ext: ".md",
        prog_id: "Mdex.Markdown",
        description: "Markdown Document",
    },
];

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

/// Register the ProgIDs and hook both extensions. Idempotent.
pub fn register() -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("cannot resolve exe path: {e}"))?
        .to_string_lossy()
        .into_owned();

    for ft in FILE_TYPES.iter() {
        let prog = classes_subkey(ft.prog_id)?;
        prog.set_value("", &ft.description)
            .map_err(err("cannot set ProgID description"))?;

        let icon = classes_subkey(&format!(r"{}\DefaultIcon", ft.prog_id))?;
        icon.set_value("", &format!("\"{exe}\",0"))
            .map_err(err("cannot set ProgID icon"))?;

        let command = classes_subkey(&format!(r"{}\shell\open\command", ft.prog_id))?;
        command
            .set_value("", &format!("\"{exe}\" \"%1\""))
            .map_err(err("cannot set open command"))?;

        // Only claim the extension default when no other application owns
        // it — or when it is ours already, including a ProgID left by an
        // earlier mdex version, which then migrates to this extension's
        // own ProgID. We always join the OpenWith list.
        let key = classes_subkey(ft.ext)?;
        let current: String = key.get_value("").unwrap_or_default();
        let owned_by_us = FILE_TYPES.iter().any(|f| f.prog_id == current);
        if current.is_empty() || owned_by_us {
            key.set_value("", &ft.prog_id)
                .map_err(err("cannot set extension default"))?;
        }
        let open_with = classes_subkey(&format!(r"{}\OpenWithProgids", ft.ext))?;
        open_with
            .set_value(ft.prog_id, &"")
            .map_err(err("cannot join OpenWith list"))?;

        // Explorer "New" context-menu entry. An `.mdex` must not be created
        // as an empty file (it would not be a valid archive), so we embed
        // the bytes of a minimal template; `.md` starts empty via NullFile.
        let shell_new = classes_subkey(&format!(r"{}\ShellNew", ft.ext))?;
        if ft.ext == ".mdex" {
            let template = RegValue {
                bytes: crate::mdex::template_bytes(),
                vtype: REG_BINARY,
            };
            shell_new
                .set_raw_value("Data", &template)
                .map_err(err("cannot set ShellNew template"))?;
        } else {
            shell_new
                .set_value("NullFile", &"")
                .map_err(err("cannot set ShellNew NullFile"))?;
        }
    }

    notify_shell();
    Ok(())
}

/// Remove the registration written by [`register`]. Also cleans up values
/// left by earlier mdex versions that used a single shared ProgID.
pub fn unregister() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for ft in FILE_TYPES.iter() {
        let _ = hkcu.delete_subkey_all(format!(r"{CLASSES}\{}\ShellNew", ft.ext));
        if let Ok(key) = hkcu.open_subkey_with_flags(
            format!(r"{CLASSES}\{}", ft.ext),
            KEY_READ | KEY_SET_VALUE,
        ) {
            // Clear the default only when it points at one of our ProgIDs.
            let current: String = key.get_value("").unwrap_or_default();
            if FILE_TYPES.iter().any(|f| f.prog_id == current) {
                let _ = key.delete_value("");
            }
        }
        if let Ok(open_with) = hkcu.open_subkey_with_flags(
            format!(r"{CLASSES}\{}\OpenWithProgids", ft.ext),
            KEY_READ | KEY_SET_VALUE,
        ) {
            for f in FILE_TYPES.iter() {
                let _ = open_with.delete_value(f.prog_id);
            }
        }
    }
    for ft in FILE_TYPES.iter() {
        let _ = hkcu.delete_subkey_all(format!(r"{CLASSES}\{}", ft.prog_id));
    }
    notify_shell();
    Ok(())
}

/// True when every ProgID is in place and at least one extension points at
/// its own ProgID.
pub fn is_registered() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let progids_ready = FILE_TYPES.iter().all(|ft| {
        hkcu.open_subkey_with_flags(
            format!(r"{CLASSES}\{}\shell\open\command", ft.prog_id),
            KEY_READ,
        )
        .is_ok()
    });
    if !progids_ready {
        return false;
    }
    FILE_TYPES.iter().any(|ft| {
        hkcu.open_subkey_with_flags(
            format!(r"{CLASSES}\{}\OpenWithProgids", ft.ext),
            KEY_READ,
        )
        .map(|open_with| open_with.get_value::<String, _>(ft.prog_id).is_ok())
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

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);

        // Each extension has its own ProgID with a distinct description, so
        // the Explorer "New" menu shows two different entries.
        for ft in FILE_TYPES.iter() {
            let prog = hkcu
                .open_subkey_with_flags(format!(r"{CLASSES}\{}", ft.prog_id), KEY_READ)
                .expect("ProgID should exist");
            let description: String = prog.get_value("").expect("ProgID description");
            assert_eq!(description, ft.description, "menu label for {}", ft.ext);

            let open_with = hkcu
                .open_subkey_with_flags(
                    format!(r"{CLASSES}\{}\OpenWithProgids", ft.ext),
                    KEY_READ,
                )
                .expect("OpenWithProgids should exist");
            assert!(
                open_with.get_value::<String, _>(ft.prog_id).is_ok(),
                "{} should list {}",
                ft.ext,
                ft.prog_id
            );
        }

        // ShellNew entries power the Explorer "New" context menu.
        let mdex_new = hkcu
            .open_subkey_with_flags(format!(r"{CLASSES}\.mdex\ShellNew"), KEY_READ)
            .expect(".mdex ShellNew should exist");
        let template = mdex_new
            .get_raw_value("Data")
            .expect("embedded template should exist");
        assert_eq!(template.vtype, REG_BINARY, "template is REG_BINARY");
        assert!(
            template.bytes.starts_with(b"PK"),
            "template must be a ZIP archive"
        );

        let md_new = hkcu
            .open_subkey_with_flags(format!(r"{CLASSES}\.md\ShellNew"), KEY_READ)
            .expect(".md ShellNew should exist");
        assert!(
            md_new.get_value::<String, _>("NullFile").is_ok(),
            ".md uses NullFile"
        );

        unregister().expect("unregister should succeed");
        assert!(!is_registered(), "should report unregistered");
        assert!(
            hkcu.open_subkey_with_flags(format!(r"{CLASSES}\.mdex\ShellNew"), KEY_READ)
                .is_err(),
            "ShellNew keys should be removed"
        );
        for ft in FILE_TYPES.iter() {
            assert!(
                hkcu.open_subkey_with_flags(format!(r"{CLASSES}\{}", ft.prog_id), KEY_READ)
                    .is_err(),
                "{} tree should be removed",
                ft.prog_id
            );
        }
    }
}
