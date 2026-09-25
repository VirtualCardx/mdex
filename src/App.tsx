/**
 * mdex — desktop markdown editor for the .mdex archive format.
 *
 * Owns document state and wires the Rust backend (open/save/add-asset) to the
 * editor, preview, toolbar and status bar. Also handles global shortcuts,
 * unsaved-change confirmation, drag & drop and window close interception.
 */

import { useCallback, useDeferredValue, useEffect, useMemo, useRef, useState } from "react";
import type { EditorView } from "@codemirror/view";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ask } from "@tauri-apps/plugin-dialog";
import Editor from "./components/Editor";
import Preview from "./components/Preview";
import Toolbar, { type ToolbarAction } from "./components/Toolbar";
import StatusBar from "./components/StatusBar";
import {
  addAsset,
  addAssetBytes,
  exportHtml,
  exportMarkdown,
  fileAssociationsRegistered,
  getStartupFile,
  importMarkdown,
  newDocument,
  openMdex,
  pickDocumentToOpen,
  pickImageToAdd,
  pickSaveTarget,
  pickSaveTargetAs,
  registerFileAssociations,
  saveDocument,
  unregisterFileAssociations,
} from "./lib/api";
import { assetUrl, renderMarkdown } from "./lib/markdown";
import { insertImageRef, runFormatAction } from "./lib/actions";
import { loadTheme, saveTheme } from "./lib/settings";
import type { CursorInfo, DocumentPayload, Theme, ViewMode } from "./lib/types";

type SaveChoice = "saved" | "discard" | "cancel";

const EXPORT_HTML_CSS = `
:root { color-scheme: light dark; }
body { margin: 0; font-family: -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
  background: #ffffff; color: #1f2328; line-height: 1.7; }
article { max-width: 820px; margin: 0 auto; padding: 48px 24px 120px; }
h1, h2, h3, h4 { line-height: 1.3; margin: 1.4em 0 0.6em; }
pre { background: #0d1117; color: #e6edf3; padding: 14px 16px; border-radius: 8px; overflow: auto; }
code { font-family: Consolas, "Cascadia Code", monospace; font-size: 0.92em; }
:not(pre) > code { background: rgba(127,127,127,.15); padding: 2px 5px; border-radius: 4px; }
blockquote { margin: 1em 0; padding: 2px 16px; border-left: 4px solid #b6b8c2; color: #59636e; }
table { border-collapse: collapse; margin: 1em 0; }
th, td { border: 1px solid #d8dee4; padding: 6px 12px; }
img { max-width: 100%; border-radius: 6px; }
.katex-display { overflow-x: auto; overflow-y: hidden; }
`;

export default function App() {
  const [doc, setDoc] = useState<DocumentPayload | null>(null);
  const [markdown, setMarkdown] = useState("");
  const [dirty, setDirty] = useState(false);
  const [docId, setDocId] = useState("initial");
  const [theme, setTheme] = useState<Theme>(loadTheme);
  const [mode, setMode] = useState<ViewMode>("split");
  const [cursor, setCursor] = useState<CursorInfo>({ ln: 1, col: 1 });
  const [message, setMessage] = useState("");
  const [editorWidth, setEditorWidth] = useState<number | null>(null);
  const [dragOver, setDragOver] = useState(false);

  const viewRef = useRef<EditorView | null>(null);
  const previewRef = useRef<HTMLDivElement | null>(null);
  const mainRef = useRef<HTMLDivElement | null>(null);
  const messageTimer = useRef<number | null>(null);

  const fileName = doc?.fileName ?? "Untitled.mdex";
  const assetCount = doc?.assetKeys.length ?? 0;

  // Mirror the latest state for listeners registered once.
  const latest = useRef({ markdown, dirty, fileName });
  latest.current = { markdown, dirty, fileName };

  const flash = useCallback((text: string) => {
    setMessage(text);
    if (messageTimer.current !== null) window.clearTimeout(messageTimer.current);
    messageTimer.current = window.setTimeout(() => setMessage(""), 4000);
  }, []);

  /** Keep the status-bar asset count in sync after an embed. */
  const trackAsset = useCallback((key: string) => {
    setDoc((prev) =>
      prev && !prev.assetKeys.includes(key)
        ? { ...prev, assetKeys: [...prev.assetKeys, key] }
        : prev,
    );
  }, []);

  const applyPayload = useCallback((payload: DocumentPayload) => {
    setDoc(payload);
    setMarkdown(payload.markdown);
    setDirty(false);
    setDocId(`doc-${payload.path ?? "untitled"}-${Date.now()}`);
  }, []);

  /** Save the document; `as` forces the save-as dialog. Returns true on success.
   *
   * A direct save keeps the format of the file the document came from
   * (`.mdex` archive or plain `.md`); Save As offers both formats, with the
   * current one preselected. */
  const doSave = useCallback(
    async (saveAs = false): Promise<boolean> => {
      try {
        const currentPath = doc?.path ?? null;
        let target: string | null = currentPath;
        if (saveAs || !target) {
          const isPlainMd = /\.md$/i.test(currentPath ?? "");
          const defaultName = currentPath
            ? fileName
            : fileName.endsWith(".mdex")
              ? fileName
              : `${fileName.replace(/\.mdex?$/, "")}.mdex`;
          const filters = isPlainMd
            ? [
                { name: "Markdown", extensions: ["md"] },
                { name: "mdex documents", extensions: ["mdex"] },
              ]
            : [
                { name: "mdex documents", extensions: ["mdex"] },
                { name: "Markdown", extensions: ["md"] },
              ];
          target = await pickSaveTargetAs(filters, defaultName);
          if (!target) return false;
        }
        const payload = await saveDocument(target, latest.current.markdown);
        applyPayload(payload);
        flash(`Saved ${payload.fileName}`);
        return true;
      } catch (error) {
        flash(`Save failed: ${String(error)}`);
        return false;
      }
    },
    [applyPayload, doc, fileName, flash],
  );

  /** Confirm unsaved changes. Resolves what should happen next. */
  const ensureSaved = useCallback(async (): Promise<SaveChoice> => {
    if (!latest.current.dirty) return "discard";
    const save = await ask(
      `"${latest.current.fileName}" has unsaved changes. Save before continuing?`,
      {
        title: "Unsaved changes",
        kind: "warning",
        okLabel: "Save",
        cancelLabel: "Discard",
      },
    );
    if (!save) return "discard";
    return (await doSave(false)) ? "saved" : "cancel";
  }, [doSave]);

  const handleNew = useCallback(async () => {
    if ((await ensureSaved()) === "cancel") return;
    applyPayload(await newDocument());
  }, [applyPayload, ensureSaved]);

  const openPath = useCallback(
    async (path: string) => {
      try {
        const payload = path.toLowerCase().endsWith(".md")
          ? await importMarkdown(path)
          : await openMdex(path);
        applyPayload(payload);
        flash(`Opened ${payload.fileName}`);
      } catch (error) {
        flash(`Open failed: ${String(error)}`);
      }
    },
    [applyPayload, flash],
  );

  const handleOpenDialog = useCallback(async () => {
    if ((await ensureSaved()) === "cancel") return;
    const path = await pickDocumentToOpen();
    if (path) await openPath(path);
  }, [ensureSaved, openPath]);

  const handleImport = useCallback(async () => {
    if ((await ensureSaved()) === "cancel") return;
    const path = await pickDocumentToOpen();
    if (path) await openPath(path);
  }, [ensureSaved, openPath]);

  const insertAssetFromDisk = useCallback(
    async (filePath: string) => {
      try {
        const key = await addAsset(filePath);
        const alt = filePath.split(/[\\/]/).pop() ?? "image";
        setDirty(true);
        trackAsset(key);
        if (viewRef.current) insertImageRef(viewRef.current, key, alt);
        flash(`Embedded ${key}`);
      } catch (error) {
        flash(`Embed failed: ${String(error)}`);
      }
    },
    [flash, trackAsset],
  );

  const handleInsertImage = useCallback(async () => {
    const path = await pickImageToAdd();
    if (path) await insertAssetFromDisk(path);
  }, [insertAssetFromDisk]);

  const handlePasteImage = useCallback(
    async (base64: string, ext: string): Promise<string | null> => {
      try {
        const key = await addAssetBytes(`pasted-image.${ext}`, base64);
        setDirty(true);
        trackAsset(key);
        return key;
      } catch (error) {
        flash(`Paste failed: ${String(error)}`);
        return null;
      }
    },
    [flash, trackAsset],
  );

  const handleExportMd = useCallback(async () => {
    const base = fileName.replace(/\.mdex$/, "");
    const target = await pickSaveTarget(["md"], "Markdown", `${base || "document"}.md`);
    if (!target) return;
    try {
      await exportMarkdown(target, latest.current.markdown);
      flash(`Exported ${target}`);
    } catch (error) {
      flash(`Export failed: ${String(error)}`);
    }
  }, [fileName, flash]);

  const handleExportHtml = useCallback(async () => {
    const base = fileName.replace(/\.mdex$/, "");
    const target = await pickSaveTarget(["html"], "HTML", `${base || "document"}.html`);
    if (!target) return;
    try {
      let html = renderMarkdown(latest.current.markdown);
      const keys = doc?.assetKeys ?? [];
      for (const key of keys) {
        try {
          const response = await fetch(assetUrl(key));
          if (!response.ok) continue;
          const blob = await response.blob();
          const dataUrl = await new Promise<string>((resolve, reject) => {
            const reader = new FileReader();
            reader.onload = () => resolve(String(reader.result));
            reader.onerror = () => reject(reader.error);
            reader.readAsDataURL(blob);
          });
          html = html.split(assetUrl(key)).join(dataUrl);
        } catch {
          // Keep the protocol URL if the asset cannot be inlined.
        }
      }
      const title = doc?.meta.title ?? fileName;
      const page = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${title.replace(/[<>&]/g, "")}</title>
<style>${EXPORT_HTML_CSS}</style>
</head>
<body><article>${html}</article></body>
</html>`;
      await exportHtml(target, page);
      flash(`Exported ${target}`);
    } catch (error) {
      flash(`Export failed: ${String(error)}`);
    }
  }, [doc, fileName, flash]);

  /** Register/unregister mdex as the default .md/.mdex handler. */
  const handleFileAssoc = useCallback(async () => {
    try {
      const registered = await fileAssociationsRegistered();
      const confirm = await ask(
        registered
          ? "mdex is currently registered for .mdex / .md files.\n\nRemove the registration?"
          : "Register mdex as the default app for .mdex and .md files?\n\nWindows may ask you to confirm in Settings when another app already owns .md.",
        {
          title: "File association",
          kind: registered ? "warning" : "info",
          okLabel: registered ? "Remove" : "Register",
          cancelLabel: "Cancel",
        },
      );
      if (!confirm) return;
      await (registered ? unregisterFileAssociations() : registerFileAssociations());
      flash(registered ? "File association removed" : "Registered for .mdex / .md");
    } catch (error) {
      flash(`File association failed: ${String(error)}`);
    }
  }, [flash]);

  // Stable handler registry for once-registered listeners.
  const handlers = useRef({
    newDoc: handleNew,
    open: handleOpenDialog,
    save: () => doSave(false),
    saveAs: () => doSave(true),
  });
  handlers.current = {
    newDoc: handleNew,
    open: handleOpenDialog,
    save: () => doSave(false),
    saveAs: () => doSave(true),
  };

  // Global shortcuts.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
      const key = event.key.toLowerCase();
      if (key === "s" && !event.shiftKey) {
        event.preventDefault();
        void handlers.current.save();
      } else if (key === "s" && event.shiftKey) {
        event.preventDefault();
        void handlers.current.saveAs();
      } else if (key === "o") {
        event.preventDefault();
        void handlers.current.open();
      } else if (key === "n") {
        event.preventDefault();
        void handlers.current.newDoc();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Confirm unsaved changes before closing the window.
  useEffect(() => {
    const win = getCurrentWindow();
    const promise = win.onCloseRequested(async (event) => {
      if (!latest.current.dirty) {
        await win.destroy();
        return;
      }
      event.preventDefault();
      const choice = await ensureSaved();
      if (choice !== "cancel") await win.destroy();
    });
    return () => {
      void promise.then((unlisten) => unlisten());
    };
  }, [ensureSaved]);

  // Drag & drop: open .mdex/.md files, embed dropped images.
  useEffect(() => {
    const webview = getCurrentWebview();
    const promise = webview.onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === "enter") {
        setDragOver(payload.paths.length > 0);
      } else if (payload.type === "over") {
        setDragOver(true);
      } else {
        setDragOver(false);
      }
      if (payload.type !== "drop") return;
      const paths = payload.paths;
      const docPath = paths.find((p) => /\.(mdex|md)$/i.test(p));
      if (docPath) {
        void (async () => {
          if ((await ensureSaved()) === "cancel") return;
          await openPath(docPath);
        })();
        return;
      }
      for (const path of paths) {
        if (/\.(png|jpe?g|gif|webp|avif|bmp|svg)$/i.test(path)) {
          void insertAssetFromDisk(path);
        }
      }
    });
    return () => {
      void promise.then((unlisten) => unlisten());
    };
  }, [ensureSaved, insertAssetFromDisk, openPath]);

  // Reflect document state in the native window title.
  useEffect(() => {
    void getCurrentWindow()
      .setTitle(`${dirty ? "\u25CF " : ""}${fileName} \u2014 mdex`)
      .catch(() => undefined);
  }, [dirty, fileName]);

  // Persist the theme choice so the next launch starts in the same mode.
  useEffect(() => {
    saveTheme(theme);
  }, [theme]);

  // Open the file passed to the process at launch (OS association).
  useEffect(() => {
    void (async () => {
      const path = await getStartupFile();
      if (path) await openPath(path);
    })();
  }, [openPath]);

  // A second instance was launched with a file (single-instance plugin).
  useEffect(() => {
    const promise = listen<string>("mdex:open-file", (event) => {
      const path = event.payload;
      void (async () => {
        if ((await ensureSaved()) === "cancel") return;
        await openPath(path);
      })();
    });
    return () => {
      void promise.then((unlisten) => unlisten());
    };
  }, [ensureSaved, openPath]);

  // Render pipeline: defer heavy markdown rendering while typing.
  const deferred = useDeferredValue(markdown);
  const html = useMemo(() => renderMarkdown(deferred), [deferred]);

  const words = useMemo(
    () => markdown.split(/\s+/).filter((w) => w.length > 0).length,
    [markdown],
  );

  const handleAction = useCallback(
    (id: ToolbarAction) => {
      switch (id) {
        case "new":
          void handleNew();
          return;
        case "open":
          void handleOpenDialog();
          return;
        case "save":
          void doSave(false);
          return;
        case "saveAs":
          void doSave(true);
          return;
        case "import":
          void handleImport();
          return;
        case "exportMd":
          void handleExportMd();
          return;
        case "exportHtml":
          void handleExportHtml();
          return;
        case "image":
          void handleInsertImage();
          return;
        case "fileAssoc":
          void handleFileAssoc();
          return;
        default:
          if (viewRef.current) runFormatAction(viewRef.current, id);
          return;
      }
    },
    [doSave, handleExportHtml, handleExportMd, handleFileAssoc, handleImport,
      handleInsertImage, handleNew, handleOpenDialog],
  );

  // Divider drag: adjust editor pane width.
  const startResize = (event: React.PointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    const container = mainRef.current;
    if (!container) return;
    const startX = event.clientX;
    const startWidth = viewRef.current?.scrollDOM.clientWidth ?? container.clientWidth / 2;
    const onMove = (moveEvent: PointerEvent) => {
      const delta = moveEvent.clientX - startX;
      const max = container.clientWidth - 300;
      setEditorWidth(Math.min(Math.max(300, startWidth + delta), Math.max(300, max)));
    };
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  const syncPreviewScroll = (ratio: number) => {
    const el = previewRef.current;
    if (!el) return;
    const max = el.scrollHeight - el.clientHeight;
    if (max > 0) el.scrollTop = ratio * max;
  };

  return (
    <div className="app" data-theme={theme}>
      <Toolbar
        onAction={handleAction}
        mode={mode}
        onModeChange={setMode}
        theme={theme}
        onThemeToggle={() => setTheme(theme === "dark" ? "light" : "dark")}
      />
      <div
        ref={mainRef}
        className={`main ${mode} ${dragOver ? "drag-over" : ""}`}
      >
        {mode !== "preview" && (
          <div
            className="pane editor-pane"
            style={editorWidth !== null ? { width: `${editorWidth}px` } : undefined}
          >
            <Editor
              key={docId}
              initialDoc={markdown}
              onChange={(next) => {
                setMarkdown(next);
                setDirty(true);
              }}
              onCursor={setCursor}
              onScrollRatio={syncPreviewScroll}
              onView={(view) => {
                viewRef.current = view;
              }}
              onPasteImage={handlePasteImage}
            />
          </div>
        )}
        {mode === "split" && <div className="divider" onPointerDown={startResize} />}
        {mode !== "edit" && (
          <div className="pane">
            <Preview ref={previewRef} html={html} />
          </div>
        )}
        {dragOver && (
          <div className="drop-hint">
            Drop a <code>.mdex</code> file to open, or an image to embed
          </div>
        )}
      </div>
      <StatusBar
        fileName={fileName}
        filePath={doc?.path ?? null}
        dirty={dirty}
        cursor={cursor}
        words={words}
        chars={markdown.length}
        assets={assetCount}
        message={message}
      />
    </div>
  );
}
