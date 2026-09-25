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
- **Format-aware saving**: a direct save (Ctrl+S) keeps the format of the
  file the document came from — `.mdex` archives re-save as archives,
  opened `.md` files write back as plain markdown (+ `assets/` folder for
  embedded images). Save As offers both formats with the current one
  preselected, so it doubles as a converter between `.mdex` and `.md`.
- **Export**: plain `.md` (+ `assets/` folder) or a self-contained
  single-file HTML with inlined images.
- **File associations (Windows)**: register mdex as the default app for
  `.mdex` / `.md` from the toolbar — see the section below.
- **Polish**: light/dark themes (the choice is remembered across launches
  and follows the OS preference until changed), unsaved-change confirmation
  on close, Ctrl+S / Ctrl+Shift+S / Ctrl+O / Ctrl+N shortcuts, status bar
  with word count and cursor position.

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
cargo test --manifest-path src-tauri/Cargo.toml --lib -- --ignored
                              # file-association registry round-trip (needs
                              # real HKCU access; skipped by default)
```

## File associations (Windows)

mdex can register itself as the default "Open with" application for `.mdex`
and `.md` files. Click the **⚙ Link files** toolbar button to register (or,
when already registered, to remove the registration). After registering,
double-clicking a document opens it in mdex; if mdex is already running, the
file is routed to the existing window instead of launching a second instance
(`tauri-plugin-single-instance`), with unsaved-change confirmation applied.

### What is written to the registry

Everything lives under `HKEY_CURRENT_USER\Software\Classes` — **no
administrator rights are required** and the registration is fully reversible
(the same button removes it):

```text
HKCU\Software\Classes
├── Mdex.Editor                          # ProgID
│   (Default)          = "Mdex Markdown Document"
│   DefaultIcon
│     (Default)        = "<path-to-mdex.exe>",0
│   shell\open\command
│     (Default)        = "<path-to-mdex.exe>" "%1"
├── .mdex
│   (Default)          = Mdex.Editor            # claimed as default
│   OpenWithProgids
│     Mdex.Editor      = ""
└── .md
    OpenWithProgids
      Mdex.Editor      = ""
```

- The extension default is only written when no other application owns it
  (or it is already ours). Windows 10/11 protects existing per-user defaults
  with a `UserChoice` hash, so for an extension already claimed by another
  app mdex still appears in the "Open with" list and you can confirm it as
  the default from there (or via Settings → Default apps).
- Explorer is notified via `SHChangeNotify(SHCNE_ASSOCCHANGED)` so icons and
  context menus refresh immediately.
- The NSIS installer produced by `pnpm tauri build` registers the same
  associations declaratively through `bundle.fileAssociations` in
  `tauri.conf.json` (and removes them on uninstall).
- Registration records the path of the running executable — when developing
  with `pnpm tauri dev` this is the debug binary, so re-register after
  installing a release build.

Leftover entries can be removed by hand if needed:

```text
reg delete "HKCU\Software\Classes\Mdex.Editor" /f
reg delete "HKCU\Software\Classes\.mdex\OpenWithProgids\Mdex.Editor" /f
reg delete "HKCU\Software\Classes\.md\OpenWithProgids\Mdex.Editor" /f
```

## Architecture

```text
src/                    React frontend
  App.tsx               document state, shortcuts, dialogs, export flows
  components/           Editor (CodeMirror), Preview, Toolbar, StatusBar
  lib/markdown.ts       markdown-it pipeline; rewrites assets/* to mdexasset://
  lib/actions.ts        toolbar text transformations on the editor view
  lib/api.ts            typed wrappers around Tauri commands and dialogs
  lib/settings.ts       persisted UI preferences (theme) via localStorage
src-tauri/
  src/mdex.rs           mdex format core: read/write ZIP archives, metadata
  src/commands.rs       Tauri commands + mdexasset:// protocol handler
  src/fileassoc.rs      Windows file-association registry (HKCU, reversible)
  src/lib.rs            app builder, plugins, state, command registry
```

The Rust backend owns all file I/O. The currently open document (markdown,
assets, metadata) lives in managed state; save writes the archive atomically
(temp file + rename). The frontend never touches the filesystem directly.

## Roadmap ideas

- Recent files menu
- Multiple documents in tabs
- Asset manager panel (rename/replace/delete unused assets)
- Document outline / table of contents sidebar
- Mermaid diagram support
