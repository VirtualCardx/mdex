mod commands;
mod mdex;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running mdex");
}
