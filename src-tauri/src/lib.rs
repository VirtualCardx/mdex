mod commands;
#[cfg(windows)]
mod fileassoc;
mod mdex;

use tauri::{Emitter, Manager};

/// Extract the first non-flag argument (a file path when launched via an OS
/// file association).
fn file_arg<I: Iterator<Item = String>>(args: I) -> Option<String> {
    args.skip(1).find(|a| !a.starts_with('-'))
}

pub fn run() {
    tauri::Builder::default()
        // Must stay first: routes launches from a second instance (e.g. a
        // double-clicked file while mdex is running) into this instance.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(path) = file_arg(argv.into_iter()) {
                if let Some(pending) = app.try_state::<mdex::PendingOpen>() {
                    *pending.0.lock().unwrap() = Some(path.clone());
                }
                let _ = app.emit("mdex:open-file", path);
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(mdex::PendingOpen::default())
        .manage(mdex::AppState::with_new_document())
        .register_uri_scheme_protocol("mdexasset", commands::handle_asset_scheme)
        .invoke_handler(tauri::generate_handler![
            commands::new_document,
            commands::open_mdex,
            commands::save_mdex,
            commands::import_markdown,
            commands::add_asset,
            commands::add_asset_bytes,
            commands::export_markdown,
            commands::export_html,
            commands::get_startup_file,
            commands::file_associations_registered,
            commands::register_file_associations,
            commands::unregister_file_associations,
        ])
        .setup(|app| {
            // A file path passed at launch (association double-click) is
            // stashed for the frontend to consume once the webview is ready.
            if let Some(path) = file_arg(std::env::args()) {
                let pending = app.state::<mdex::PendingOpen>();
                *pending.0.lock().unwrap() = Some(path);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running mdex");
}
