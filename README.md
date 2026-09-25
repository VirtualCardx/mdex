# mdex

A desktop markdown editor for the custom **`.mdex` document format** — a single
ZIP archive that bundles a markdown document together with the resources it
references. Built with Tauri 2, React, CodeMirror 6, and Rust.

![mdex](src-tauri/icons/128x128.png)

## The mdex format

An `.mdex` file is a standard ZIP archive (deflate) with this layout:

```text
my-document.mdex
├── meta.json      # format metadata (see below)
├── document.md    # the markdown source
└── assets/        # binary resources referenced from the markdown
    └── logo.png
```

`meta.json`:

```json
{
  "format": "mdex",
  "version": 1,
  "title": "Optional document title",
  "created": "2026-09-25T00:00:00Z",
  "modified": "2026-09-25T00:00:00Z"
}
```

- `format` must be `"mdex"`; `version` `1` is the current revision.
- `document.md` is required; images are referenced as `![alt](assets/name.png)`.
- Unknown entries are ignored, so future revisions can extend the format.
- Any ZIP tool can open an `.mdex` file — it is a plain archive.

A `sample.mdex` lives in the repository root and showcases all features.

## Features

- **Split view**: CodeMirror 6 source editor + live rendered preview with
  proportional scroll sync; Edit / Split / Preview modes.
- **Embedded assets**: images live inside the archive and are served to the
  webview through the custom `mdexasset://` protocol (never unpacked to disk).
- **Markdown niceties**: GFM tables and task lists, fenced code blocks with
  syntax highlighting (highlight.js), math via KaTeX, linkified URLs.
- **Image workflows**: paste an image from the clipboard, drag & drop an image
  file, or use the toolbar — all stored as archive assets.
- **Import / export**: open or import plain `.md`; export back to `.md`
  (+ `assets/` folder) or to a self-contained single-file HTML with inlined
  images.
- **Polish**: light/dark themes, unsaved-change confirmation on close,
  Ctrl+S / Ctrl+Shift+S / Ctrl+O / Ctrl+N shortcuts, status bar with word
  count and cursor position.

## Getting started

Requires Node.js 18+, pnpm, and the Rust toolchain with MSVC.

```bash
pnpm install          # install frontend dependencies
pnpm tauri dev        # run the app in dev mode
pnpm tauri build      # build a release bundle (NSIS installer + exe)
```

Useful extras:

```bash
node scripts/make-icon.mjs   # regenerate src-tauri/app-icon.png
pnpm tauri icon src-tauri/app-icon.png
cargo test --manifest-path src-tauri/Cargo.toml   # format round-trip tests
```

## Architecture

```text
src/                    React frontend
  App.tsx               document state, shortcuts, dialogs, export flows
  components/           Editor (CodeMirror), Preview, Toolbar, StatusBar
  lib/markdown.ts       markdown-it pipeline; rewrites assets/* to mdexasset://
  lib/actions.ts        toolbar text transformations on the editor view
  lib/api.ts            typed wrappers around Tauri commands and dialogs
src-tauri/
  src/mdex.rs           mdex format core: read/write ZIP archives, metadata
  src/commands.rs       Tauri commands + mdexasset:// protocol handler
  src/lib.rs            app builder, plugins, state, command registry
```

The Rust backend owns all file I/O. The currently open document (markdown,
assets, metadata) lives in managed state; save writes the archive atomically
(temp file + rename). The frontend never touches the filesystem directly.

## Roadmap ideas

- File association for double-click `.mdex` open, recent files menu
- Multiple documents in tabs
- Asset manager panel (rename/replace/delete unused assets)
- Document outline / table of contents sidebar
- Mermaid diagram support
