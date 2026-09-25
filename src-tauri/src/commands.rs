//! Tauri commands and the `mdexasset://` protocol that serves archive
//! resources to the webview.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

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

    save_to(doc, &target)?;
    Ok(mdex::payload_of(doc))
}

/// Shared save path behind `save_document`: the target extension decides the
/// format (`.md` plain, anything else archive) and becomes the document path.
fn save_to(doc: &mut mdex::LoadedDocument, target: &std::path::Path) -> Result<(), String> {
    let is_plain_md = target
        .extension()
        .map(|e| e.eq_ignore_ascii_case("md"))
        .unwrap_or(false);
    if is_plain_md {
        write_plain(target, &doc.markdown, &doc.assets)?;
    } else {
        // Converting to `.mdex`: an opened `.md` keeps its images outside the
        // document, so pull every referenced file into the archive and rewrite
        // the references to their new `assets/` keys.
        import_external_assets(doc);
        mdex::write_to(target, doc).map_err(|e| e.to_string())?;
    }
    doc.path = Some(target.to_path_buf());
    Ok(())
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

/// Pull the external images referenced by an opened `.md` file into the
/// document's asset map (so an archive save becomes self-contained) and
/// rewrite each reference to its new `assets/` key. References that cannot
/// be resolved are left untouched.
fn import_external_assets(doc: &mut mdex::LoadedDocument) {
    let Some(src) = doc.path.clone() else {
        return;
    };
    let mut rewrites: Vec<(usize, usize, String)> = Vec::new();
    for image in find_image_refs(&doc.markdown) {
        let dest = &doc.markdown[image.dest_start..image.dest_end];
        let Some(key) = normalize_asset_ref(dest) else {
            continue;
        };
        if doc.assets.contains_key(&key) {
            continue;
        }
        let Some(bytes) = read_disk_asset(&src, &key) else {
            continue;
        };
        let name = key.rsplit('/').next().unwrap_or("image");
        let new_key = mdex::unique_asset_key(&doc.assets, name, "bin");
        doc.assets.insert(new_key.clone(), bytes);
        rewrites.push((image.dest_start, image.dest_end, markdown_dest(&new_key)));
    }
    // Byte offsets stay valid as long as we rewrite back to front.
    for (start, end, dest) in rewrites.into_iter().rev() {
        doc.markdown.replace_range(start..end, &dest);
    }
}

/// The destination part of an inline image `![alt](dest)`, as byte offsets
/// into the markdown.
struct ImageRef {
    dest_start: usize,
    dest_end: usize,
}

/// Scan markdown for inline image syntax. Deliberately simple: reference-
/// style images and `<img>` tags are left alone (they keep pointing at
/// external files, same as before).
fn find_image_refs(markdown: &str) -> Vec<ImageRef> {
    let bytes = markdown.as_bytes();
    let mut refs = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] != b'!' || bytes[i + 1] != b'[' {
            i += 1;
            continue;
        }
        let mut j = i + 2;
        while j < bytes.len() && bytes[j] != b']' {
            j += 1;
        }
        if j + 1 >= bytes.len() || bytes[j + 1] != b'(' {
            i = j.max(i + 2);
            continue;
        }
        let mut k = j + 2;
        while k < bytes.len() && bytes[k] != b')' {
            k += 1;
        }
        if k >= bytes.len() {
            i = j + 1;
            continue;
        }
        refs.push(ImageRef {
            dest_start: j + 2,
            dest_end: k,
        });
        i = k + 1;
    }
    refs
}

/// Normalize an image destination into an archive-asset key: unify
/// separators, drop `./` segments, percent-decode. Rejects URLs, absolute
/// paths, and anything that could traverse upwards.
fn normalize_asset_ref(dest: &str) -> Option<String> {
    let trimmed = dest.trim();
    let inner = trimmed
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(trimmed);
    let unified = inner.replace('\\', "/");
    if unified.is_empty() || unified.starts_with('/') || unified.starts_with('#') {
        return None;
    }
    // Any colon makes this a scheme (`https:`, `data:`) or a Windows drive
    // (`c:/x`) — neither may resolve inside the archive, so reject outright.
    if unified.contains(':') {
        return None;
    }
    let joined: Vec<&str> = unified
        .split('/')
        .filter(|seg| !seg.is_empty() && *seg != ".")
        .collect();
    if joined.is_empty() || joined.iter().any(|seg| *seg == "..") {
        return None;
    }
    let decoded = percent_encoding::percent_decode_str(&joined.join("/"))
        .decode_utf8_lossy()
        .into_owned();
    // Decoding may reintroduce separators (`%2F`), so validate again.
    if decoded.is_empty()
        || decoded.starts_with('/')
        || decoded.split('/').any(|seg| seg == "..")
    {
        return None;
    }
    Some(decoded)
}

/// Markdown destination for an asset key, safe inside `![](...)`: wrapped in
/// `<...>` when the key contains whitespace or link-syntax characters.
fn markdown_dest(key: &str) -> String {
    let plain = key.chars().all(|c| {
        !c.is_whitespace() && !matches!(c, '(' | ')' | '<' | '>' | '"')
    });
    if plain {
        String::from(key)
    } else {
        format!(
            "<{}>",
            key.replace('%', "%25")
                .replace('<', "%3C")
                .replace('>', "%3E")
        )
    }
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
    let bytes = match doc.assets.get(&key) {
        Some(bytes) => bytes.clone(),
        None => {
            // Plain-markdown documents keep their images on disk next to
            // the file instead of inside the archive.
            let disk = doc
                .path
                .as_deref()
                .and_then(|p| read_disk_asset(p, &key));
            match disk {
                Some(bytes) => bytes,
                None => return not_found("asset not found"),
            }
        }
    };

    http::Response::builder()
        .header("Content-Type", mdex::mime_for(&key))
        .header("Cache-Control", "no-store")
        // Allow fetch() from the app page (e.g. HTML export inlining).
        .header("Access-Control-Allow-Origin", "*")
        .body(bytes)
        .unwrap()
}

/// Read an asset from disk for a plain-markdown document, resolving `key`
/// relative to the document's directory (e.g. `images/foo.png` next to the
/// opened `.md` file). Archive documents keep every asset in memory, so only
/// `.md`/`.markdown` paths qualify. The key is validated so it cannot
/// escape the document directory.
fn read_disk_asset(doc_path: &Path, key: &str) -> Option<Vec<u8>> {
    let ext = doc_path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !matches!(ext.as_str(), "md" | "markdown") {
        return None;
    }
    if key.is_empty() || key.split('/').any(|seg| seg == "..") {
        return None;
    }
    let dir = doc_path.parent()?;
    if dir.as_os_str().is_empty() {
        return None;
    }
    let candidate = dir.join(key);
    // Defense in depth: the joined path must stay under the document dir.
    if !candidate.starts_with(dir) {
        return None;
    }
    fs::read(candidate).ok()
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

    #[test]
    fn read_disk_asset_resolves_relative_keys_safely() {
        // Use src-tauri/target (gitignored) so the test works under sandboxes
        // that restrict writes to the workspace.
        let base = std::env::current_dir()
            .expect("cwd")
            .join("target")
            .join("disk-asset-test");
        let _ = fs::remove_dir_all(&base);
        let doc = base.join("notes.md");
        fs::create_dir_all(base.join("images")).expect("mkdir");
        fs::write(&doc, "# notes").expect("write doc");
        fs::write(base.join("images").join("pic.png"), [9u8, 8, 7]).expect("write image");

        // Relative reference next to the .md file resolves.
        assert_eq!(
            read_disk_asset(&doc, "images/pic.png"),
            Some(vec![9u8, 8, 7])
        );
        // Traversal and non-markdown documents are rejected.
        assert_eq!(read_disk_asset(&doc, "../escape.png"), None);
        assert_eq!(read_disk_asset(&doc, "a/../b.png"), None);
        assert_eq!(read_disk_asset(&base.join("book.mdex"), "images/pic.png"), None);
        assert_eq!(read_disk_asset(&doc, "missing.png"), None);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn normalize_asset_ref_filters_unsafe_destinations() {
        assert_eq!(
            normalize_asset_ref("images/pic.png").as_deref(),
            Some("images/pic.png")
        );
        assert_eq!(normalize_asset_ref("./a b/pic.png").as_deref(), Some("a b/pic.png"));
        assert_eq!(normalize_asset_ref("a%20b.png").as_deref(), Some("a b.png"));
        assert_eq!(normalize_asset_ref("<x.png>").as_deref(), Some("x.png"));
        assert_eq!(normalize_asset_ref(""), None);
        assert_eq!(normalize_asset_ref("https://x/y.png"), None);
        assert_eq!(normalize_asset_ref("data:image/png;base64,AA"), None);
        assert_eq!(normalize_asset_ref("/abs.png"), None);
        assert_eq!(normalize_asset_ref("c:/win.png"), None);
        assert_eq!(normalize_asset_ref("../up.png"), None);
        assert_eq!(normalize_asset_ref("a/../../b.png"), None);
    }

    #[test]
    fn save_as_mdex_imports_external_images() {
        // Use src-tauri/target (gitignored) so the test works under sandboxes
        // that restrict writes to the workspace.
        let base = std::env::current_dir()
            .expect("cwd")
            .join("target")
            .join("mdex-import-test");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("images")).expect("mkdir");
        fs::write(base.join("notes.md"), "# notes").expect("write doc");
        fs::write(base.join("images").join("pic.png"), [7u8, 8, 9]).expect("write image");

        let markdown = String::from(
            "![a](images/pic.png)\n\n![b](https://example.com/x.png)\n\n![c](missing.png)\n",
        );
        let mut doc = mdex::new_document(markdown, None);
        doc.path = Some(base.join("notes.md"));

        let target = base.join("out").join("notes.mdex");
        save_to(&mut doc, &target).expect("save_to should succeed");

        // The external image was pulled in and its reference rewritten to
        // the new assets key; URL and missing references stay untouched.
        assert!(
            doc.markdown.contains("](assets/pic.png)"),
            "reference rewritten: {}",
            doc.markdown
        );
        assert!(
            !doc.markdown.contains("images/pic.png"),
            "old reference gone: {}",
            doc.markdown
        );
        assert!(doc.markdown.contains("https://example.com/x.png"));
        assert!(doc.markdown.contains("missing.png"));
        let png_key = doc
            .assets
            .keys()
            .find(|k| k.starts_with("assets/") && k.ends_with(".png"))
            .expect("image imported into assets");
        assert_eq!(doc.assets.get(png_key).map(|v| v.as_slice()), Some(&[7u8, 8, 9][..]));

        // Reopening the archive proves it is self-contained.
        let reopened = mdex::open(target.clone()).expect("reopen archive");
        assert!(reopened.markdown.contains("](assets/pic.png)"));
        assert_eq!(
            reopened.assets.get(png_key).map(|v| v.as_slice()),
            Some(&[7u8, 8, 9][..])
        );

        let _ = fs::remove_dir_all(&base);
    }
}