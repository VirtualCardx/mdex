//! Tauri commands and the `mdexasset://` protocol that serves archive
//! resources to the webview.

use std::fs;
use std::io::Read;
use std::path::PathBuf;

use base64::Engine as _;
use tauri::http;
use tauri::{Manager, State};

use crate::mdex::{self, AppState, DocumentPayload};

#[tauri::command]
pub fn new_document(state: State<AppState>) -> Result<DocumentPayload, String> {
    let doc = mdex::new_document("", None);
    let payload = mdex::payload_of(&doc);
    *state.0.lock().unwrap() = Some(doc);
    Ok(payload)
}

#[tauri::command]
pub fn open_mdex(path: String, state: State<AppState>) -> Result<DocumentPayload, String> {
    let doc = mdex::open(PathBuf::from(&path)).map_err(|e| e.to_string())?;
    let payload = mdex::payload_of(&doc);
    *state.0.lock().unwrap() = Some(doc);
    Ok(payload)
}

/// Save the document to `path`. The format follows the target extension:
/// `.mdex` writes an archive, `.md` writes plain markdown (plus an `assets/`
/// folder when the document has embedded assets). With `path: None`, the
/// document re-saves to its current location in its current format.
#[tauri::command]
pub fn save_document(
    path: Option<String>,
    markdown: String,
    state: State<AppState>,
) -> Result<DocumentPayload, String> {
    let mut guard = state.0.lock().unwrap();
    let doc = guard.as_mut().ok_or("no document is open")?;
    doc.markdown = markdown;
    doc.meta.modified = mdex::now_rfc3339();

    let target = match path {
        Some(p) => PathBuf::from(p),
        None => doc.path.clone().ok_or("no path: use Save As")?,
    };

    let is_plain_md = target
        .extension()
        .map(|e| e.eq_ignore_ascii_case("md"))
        .unwrap_or(false);
    if is_plain_md {
        write_plain(&target, &doc.markdown, &doc.assets)?;
    } else {
        mdex::write_to(&target, doc).map_err(|e| e.to_string())?;
    }
    doc.path = Some(target);
    Ok(mdex::payload_of(doc))
}

/// Write a plain `.md` file; embedded assets go to an `assets/` folder next
/// to it so relative references (`![x](assets/y.png)`) keep working.
fn write_plain(
    target: &std::path::Path,
    markdown: &str,
    assets: &std::collections::HashMap<String, Vec<u8>>,
) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    fs::write(target, markdown).map_err(|e| e.to_string())?;
    if !assets.is_empty() {
        if let Some(parent) = target.parent() {
            let assets_dir = parent.join("assets");
            fs::create_dir_all(&assets_dir).map_err(|e| e.to_string())?;
            for (key, bytes) in assets {
                let name = key.strip_prefix("assets/").unwrap_or(key);
                fs::write(assets_dir.join(name), bytes).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

/// Import a plain `.md` file. The source path is kept on the document so a
/// direct save writes the file back in place (still as `.md`).
#[tauri::command]
pub fn import_markdown(path: String, state: State<AppState>) -> Result<DocumentPayload, String> {
    let markdown = fs::read_to_string(&path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let title = mdex::title_from_markdown(&markdown);
    let mut doc = mdex::new_document(markdown, title);
    doc.path = Some(PathBuf::from(&path));
    let payload = mdex::payload_of(&doc);
    *state.0.lock().unwrap() = Some(doc);
    Ok(payload)
}

/// Copy an external file into the open document's assets. Returns the asset key.
#[tauri::command]
pub fn add_asset(file_path: String, state: State<AppState>) -> Result<String, String> {
    let mut guard = state.0.lock().unwrap();
    let doc = guard.as_mut().ok_or("no document is open")?;
    let mut file = fs::File::open(&file_path).map_err(|e| format!("cannot read {file_path}: {e}"))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    let name = PathBuf::from(&file_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from("asset"));
    let key = mdex::unique_asset_key(&doc.assets, &name, "bin");
    doc.assets.insert(key.clone(), bytes);
    doc.meta.modified = mdex::now_rfc3339();
    Ok(key)
}

/// Add an asset from raw base64 bytes (e.g. an image pasted from the
/// clipboard). Returns the asset key.
#[tauri::command]
pub fn add_asset_bytes(
    name: String,
    base64_data: String,
    state: State<AppState>,
) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data.trim())
        .map_err(|e| format!("invalid base64: {e}"))?;
    let mut guard = state.0.lock().unwrap();
    let doc = guard.as_mut().ok_or("no document is open")?;
    let key = mdex::unique_asset_key(&doc.assets, &name, "png");
    doc.assets.insert(key.clone(), bytes);
    doc.meta.modified = mdex::now_rfc3339();
    Ok(key)
}

/// Export the current document as a plain `.md` file; assets are written to
/// an `assets/` folder next to it so relative references keep working.
#[tauri::command]
pub fn export_markdown(path: String, markdown: String, state: State<AppState>) -> Result<(), String> {
    let guard = state.0.lock().unwrap();
    let doc = guard.as_ref().ok_or("no document is open")?;
    write_plain(&PathBuf::from(&path), &markdown, &doc.assets)
}

/// Save a fully-rendered, self-contained HTML page produced by the frontend.
#[tauri::command]
pub fn export_html(path: String, html: String) -> Result<(), String> {
    let target = PathBuf::from(&path);
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    fs::write(&target, html).map_err(|e| e.to_string())
}

/// Consume the file path passed to the process at launch (OS file
/// association double-click). Returns `None` after the first call.
#[tauri::command]
pub fn get_startup_file(state: State<mdex::PendingOpen>) -> Option<String> {
    state.0.lock().unwrap().take()
}

#[cfg(windows)]
#[tauri::command]
pub fn file_associations_registered() -> bool {
    crate::fileassoc::is_registered()
}

#[cfg(not(windows))]
#[tauri::command]
pub fn file_associations_registered() -> bool {
    false
}

#[cfg(windows)]
#[tauri::command]
pub fn register_file_associations() -> Result<(), String> {
    crate::fileassoc::register()
}

#[cfg(not(windows))]
#[tauri::command]
pub fn register_file_associations() -> Result<(), String> {
    Err(String::from("file associations are only supported on Windows"))
}

#[cfg(windows)]
#[tauri::command]
pub fn unregister_file_associations() -> Result<(), String> {
    crate::fileassoc::unregister()
}

#[cfg(not(windows))]
#[tauri::command]
pub fn unregister_file_associations() -> Result<(), String> {
    Err(String::from("file associations are only supported on Windows"))
}

/// `mdexasset://<key>` protocol: serves asset bytes from the loaded archive.
/// Keys are only looked up in the in-memory map, so path traversal is
/// impossible by construction.
pub fn handle_asset_scheme(
    ctx: tauri::UriSchemeContext<'_, tauri::Wry>,
    request: http::Request<Vec<u8>>,
) -> http::Response<Vec<u8>> {
    let not_found = |msg: &str| {
        http::Response::builder()
            .status(http::StatusCode::NOT_FOUND)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(msg.as_bytes().to_vec())
            .unwrap()
    };

    let raw_path = request.uri().path().trim_start_matches('/');
    let decoded = percent_encoding::percent_decode_str(raw_path)
        .decode_utf8_lossy()
        .into_owned();
    let key = decoded.trim_start_matches('/').replace('\\', "/");

    if key.is_empty() || key.contains("..") || key.starts_with('/') {
        return not_found("invalid asset key");
    }

    let state = ctx.app_handle().state::<AppState>();
    let guard = state.0.lock().unwrap();
    let Some(doc) = guard.as_ref() else {
        return not_found("no document open");
    };
    let Some(bytes) = doc.assets.get(&key) else {
        return not_found("asset not found");
    };

    http::Response::builder()
        .header("Content-Type", mdex::mime_for(&key))
        .header("Cache-Control", "no-store")
        // Allow fetch() from the app page (e.g. HTML export inlining).
        .header("Access-Control-Allow-Origin", "*")
        .body(bytes.clone())
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_plain_outputs_document_and_assets() {
        // Use src-tauri/target (gitignored) so the test works under sandboxes
        // that restrict writes to the workspace.
        let base = std::env::current_dir()
            .expect("cwd")
            .join("target")
            .join("write-plain-test");
        let _ = fs::remove_dir_all(&base);
        let target = base.join("out").join("doc.md");

        let mut assets = std::collections::HashMap::new();
        assets.insert(String::from("assets/logo.png"), vec![1u8, 2, 3]);

        write_plain(&target, "# hi", &assets).expect("write_plain should succeed");

        assert_eq!(fs::read_to_string(&target).unwrap(), "# hi");
        assert_eq!(
            fs::read(base.join("out").join("assets").join("logo.png")).unwrap(),
            vec![1u8, 2, 3]
        );
        let _ = fs::remove_dir_all(&base);
    }
}
