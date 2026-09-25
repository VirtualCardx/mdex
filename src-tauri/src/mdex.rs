//! Core mdex document format: a ZIP archive bundling a markdown document
//! with the resources (images, etc.) it references.
//!
//! Layout of an `.mdex` file:
//!
//! ```text
//! meta.json      {"format":"mdex","version":1,"title":...,"created":...,"modified":...}
//! document.md    the markdown source
//! assets/        binary resources referenced from the markdown as assets/<name>
//! ```

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const FORMAT_NAME: &str = "mdex";
pub const FORMAT_VERSION: u32 = 1;
pub const META_ENTRY: &str = "meta.json";
pub const DOC_ENTRY: &str = "document.md";
pub const ASSET_PREFIX: &str = "assets/";

#[derive(Debug, thiserror::Error)]
pub enum MdexError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("metadata error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("missing required entry: {0}")]
    Missing(&'static str),
    #[error("invalid mdex file: {0}")]
    Invalid(String),
}

/// Document metadata, serialized to `meta.json` inside the archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub format: String,
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub created: String,
    pub modified: String,
}

/// A document currently loaded in memory.
#[derive(Debug)]
pub struct LoadedDocument {
    pub path: Option<PathBuf>,
    pub markdown: String,
    /// Map of archive entry name (`assets/<name>`) to bytes.
    pub assets: HashMap<String, Vec<u8>>,
    pub meta: Meta,
}

/// Payload sent to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPayload {
    pub path: Option<String>,
    pub file_name: String,
    pub markdown: String,
    pub asset_keys: Vec<String>,
    pub meta: Meta,
}

/// Global app state holding the currently open document.
#[derive(Default)]
pub struct AppState(pub Mutex<Option<LoadedDocument>>);

impl AppState {
    /// State initialized with a fresh untitled document so image embedding
    /// and the `mdexasset://` protocol work before the first explicit
    /// New/Open/Import action.
    pub fn with_new_document() -> Self {
        Self(Mutex::new(Some(new_document("", None))))
    }
}

/// A file path handed to the process on the command line (double-clicking a
/// `.mdex`/`.md` file via the OS association). Consumed by the frontend on
/// startup via `get_startup_file`.
#[derive(Default)]
pub struct PendingOpen(pub Mutex<Option<String>>);

pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| String::from("1970-01-01T00:00:00Z"))
}

/// Create a fresh untitled document.
pub fn new_document(markdown: impl Into<String>, title: Option<String>) -> LoadedDocument {
    let now = now_rfc3339();
    LoadedDocument {
        path: None,
        markdown: markdown.into(),
        assets: HashMap::new(),
        meta: Meta {
            format: FORMAT_NAME.to_string(),
            version: FORMAT_VERSION,
            title,
            created: now.clone(),
            modified: now,
        },
    }
}

/// Extract a title from the first level-1 heading of a markdown document.
pub fn title_from_markdown(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(|t| t.trim().to_string()))
        .filter(|t| !t.is_empty())
}

pub fn payload_of(doc: &LoadedDocument) -> DocumentPayload {
    let mut asset_keys: Vec<String> = doc.assets.keys().cloned().collect();
    asset_keys.sort();
    DocumentPayload {
        path: doc.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
        file_name: doc
            .path
            .as_ref()
            .map(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| String::from("Untitled.mdex"))
            })
            .unwrap_or_else(|| String::from("Untitled.mdex")),
        markdown: doc.markdown.clone(),
        asset_keys,
        meta: doc.meta.clone(),
    }
}

/// Open and validate an `.mdex` archive.
pub fn open(path: PathBuf) -> Result<LoadedDocument, MdexError> {
    let file = fs::File::open(&path)?;
    let mut archive = ZipArchive::new(file)?;

    let mut meta: Option<Meta> = None;
    let mut markdown: Option<String> = None;
    let mut assets: HashMap<String, Vec<u8>> = HashMap::new();

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        // Normalize separators so archives written on any platform load fine.
        let name = entry.name().replace('\\', "/");
        if entry.is_dir() {
            continue;
        }
        match name.as_str() {
            META_ENTRY => {
                let mut text = String::new();
                entry.read_to_string(&mut text)?;
                meta = Some(serde_json::from_str(&text)?);
            }
            DOC_ENTRY => {
                let mut text = String::new();
                entry.read_to_string(&mut text)?;
                markdown = Some(text);
            }
            _ if name.starts_with(ASSET_PREFIX) => {
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes)?;
                assets.insert(name, bytes);
            }
            _ => {
                // Unknown entries are ignored for forward compatibility.
            }
        }
    }

    let meta = meta.ok_or(MdexError::Missing(META_ENTRY))?;
    if meta.format != FORMAT_NAME {
        return Err(MdexError::Invalid(format!(
            "unexpected format {:?}",
            meta.format
        )));
    }
    if meta.version > FORMAT_VERSION {
        return Err(MdexError::Invalid(format!(
            "file uses mdex version {} but this build supports up to {}",
            meta.version, FORMAT_VERSION
        )));
    }
    let markdown = markdown.ok_or(MdexError::Missing(DOC_ENTRY))?;

    Ok(LoadedDocument {
        path: Some(path),
        markdown,
        assets,
        meta,
    })
}

/// Write the document to an `.mdex` archive atomically (tmp file + rename).
pub fn write_to(path: &Path, doc: &LoadedDocument) -> Result<(), MdexError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let tmp = path.with_extension("mdex.tmp");

    {
        let file = fs::File::create(&tmp)?;
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated);

        writer.start_file(META_ENTRY, options)?;
        writer.write_all(serde_json::to_string_pretty(&doc.meta)?.as_bytes())?;

        writer.start_file(DOC_ENTRY, options)?;
        writer.write_all(doc.markdown.as_bytes())?;

        let mut keys: Vec<&String> = doc.assets.keys().collect();
        keys.sort();
        for key in keys {
            writer.start_file(key.as_str(), options)?;
            writer.write_all(&doc.assets[key])?;
        }

        writer.finish()?;
    }

    // Windows `rename` fails when the destination exists; remove first.
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Sanitize a display file name into a safe asset base name.
fn sanitize_stem(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in input.chars() {
        let keep = ch.is_ascii_alphanumeric() || ch == '-' || ch == '_';
        if keep {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    let mut result: String = trimmed.chars().take(48).collect();
    if result.is_empty() {
        result = String::from("asset");
    }
    result
}

/// Build a unique `assets/<name>` key that does not collide with existing ones.
pub fn unique_asset_key(
    assets: &HashMap<String, Vec<u8>>,
    original_name: &str,
    fallback_ext: &str,
) -> String {
    let file_name = original_name.replace('\\', "/");
    let base = file_name.rsplit('/').next().unwrap_or("asset");
    let (stem_raw, ext_raw) = match base.rsplit_once('.') {
        Some((s, e)) if !e.is_empty() => (s, e),
        _ => (base, fallback_ext),
    };
    let ext: String = ext_raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect::<String>()
        .to_ascii_lowercase();
    let ext = if ext.is_empty() { String::from("bin") } else { ext };
    let stem = sanitize_stem(stem_raw);

    let candidate = format!("{ASSET_PREFIX}{stem}.{ext}");
    if !assets.contains_key(&candidate) {
        return candidate;
    }
    for counter in 1..u32::MAX {
        let candidate = format!("{ASSET_PREFIX}{stem}-{counter}.{ext}");
        if !assets.contains_key(&candidate) {
            return candidate;
        }
    }
    format!("{ASSET_PREFIX}{stem}-x.{ext}")
}

/// Guess a MIME type from an asset key extension.
pub fn mime_for(key: &str) -> &'static str {
    let ext = key
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "pdf" => "application/pdf",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "txt" => "text/plain",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mdex-test-{name}-{}.mdex", std::process::id()))
    }

    #[test]
    fn round_trip_document() {
        let path = temp_path("roundtrip");
        let mut doc = new_document("# Title\n\nhello ![i](assets/i.png)\n", Some("Title".into()));
        doc.assets.insert("assets/i.png".into(), vec![1, 2, 3, 4, 5]);

        write_to(&path, &doc).unwrap();
        let loaded = open(path.clone()).unwrap();

        assert_eq!(loaded.markdown, doc.markdown);
        assert_eq!(loaded.assets.get("assets/i.png").unwrap(), &(vec![1, 2, 3, 4, 5]));
        assert_eq!(loaded.meta.format, FORMAT_NAME);
        assert_eq!(loaded.meta.version, FORMAT_VERSION);
        assert_eq!(loaded.meta.title.as_deref(), Some("Title"));
        assert_eq!(loaded.path.as_deref(), Some(path.as_path()));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn rejects_non_mdex_archives() {
        let path = temp_path("notmdex");
        let file = fs::File::create(&path).unwrap();
        let mut writer = ZipWriter::new(file);
        writer
            .start_file("readme.txt", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"hello").unwrap();
        writer.finish().unwrap();

        let err = open(path.clone()).unwrap_err();
        assert!(matches!(err, MdexError::Missing(_)));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn unique_asset_keys_avoid_collisions() {
        let mut assets: HashMap<String, Vec<u8>> = HashMap::new();
        assets.insert("assets/photo.png".into(), vec![]);
        let first = unique_asset_key(&assets, "photo.png", "png");
        assert_ne!(first, "assets/photo.png");
        assets.insert(first.clone(), vec![]);
        let second = unique_asset_key(&assets, "photo.png", "png");
        assert_ne!(second, first);
    }

    #[test]
    fn title_extraction() {
        assert_eq!(
            title_from_markdown("prelude\n# My Doc\n\ntext"),
            Some("My Doc".to_string())
        );
        assert_eq!(title_from_markdown("no heading"), None);
    }
}
