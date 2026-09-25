/**
 * CodeMirror 6 markdown editor.
 *
 * The view is created once per document (App remounts this component with a
 * fresh `key` when a document is opened). Callbacks are kept in refs so the
 * extension array can be built a single time.
 */

import { useEffect, useRef } from "react";
import { EditorView, keymap, highlightActiveLine, highlightActiveLineGutter } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { languages } from "@codemirror/language-data";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import type { CursorInfo } from "../lib/types";

export interface EditorProps {
  initialDoc: string;
  onChange: (doc: string) => void;
  onCursor: (pos: CursorInfo) => void;
  onScrollRatio: (ratio: number) => void;
  onView: (view: EditorView | null) => void;
  /** Returns the asset key for the embedded image, or null to skip. */
  onPasteImage: (base64: string, ext: string) => Promise<string | null>;
}

const editorTheme = EditorView.theme({
  "&": {
    height: "100%",
    fontSize: "14.5px",
    backgroundColor: "transparent",
    color: "var(--fg)",
  },
  ".cm-scroller": {
    fontFamily: "var(--font-mono)",
    lineHeight: "1.65",
    padding: "18px 20px 40vh",
  },
  ".cm-gutters": {
    backgroundColor: "transparent",
    color: "var(--muted)",
    borderRight: "1px solid var(--border)",
    paddingLeft: "10px",
  },
  ".cm-activeLine": {
    backgroundColor: "color-mix(in srgb, var(--accent) 6%, transparent)",
  },
  ".cm-activeLineGutter": {
    backgroundColor: "transparent",
    color: "var(--fg)",
  },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": {
    backgroundColor: "color-mix(in srgb, var(--accent) 24%, transparent)",
  },
  ".cm-cursor": {
    borderLeftColor: "var(--accent)",
    borderLeftWidth: "2px",
  },
  ".cm-selectionMatch": {
    backgroundColor: "color-mix(in srgb, var(--accent) 18%, transparent)",
  },
});

const markdownHighlight = HighlightStyle.define([
  { tag: t.heading, fontWeight: "700", color: "var(--heading)" },
  { tag: t.strong, fontWeight: "700" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strikethrough, textDecoration: "line-through", color: "var(--muted)" },
  { tag: t.link, color: "var(--accent)", textDecoration: "underline" },
  { tag: t.url, color: "var(--accent)" },
  { tag: t.monospace, color: "var(--code)" },
  { tag: t.quote, color: "var(--fg-soft)", fontStyle: "italic" },
  { tag: [t.processingInstruction, t.bracket, t.punctuation], color: "var(--muted)" },
]);

const IMAGE_EXTENSIONS: Record<string, string> = {
  "image/png": "png",
  "image/jpeg": "jpg",
  "image/gif": "gif",
  "image/webp": "webp",
  "image/avif": "avif",
  "image/bmp": "bmp",
  "image/svg+xml": "svg",
};

export default function Editor(props: EditorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const callbacks = useRef(props);
  callbacks.current = props;

  useEffect(() => {
    if (!hostRef.current) return;

    const view = new EditorView({
      doc: props.initialDoc,
      parent: hostRef.current,
      extensions: [
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
        markdown({ base: markdownLanguage, codeLanguages: languages }),
        EditorView.lineWrapping,
        editorTheme,
        syntaxHighlighting(markdownHighlight),
        highlightActiveLine(),
        highlightActiveLineGutter(),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            callbacks.current.onChange(update.state.doc.toString());
          }
          if (update.docChanged || update.selectionSet) {
            const head = update.state.selection.main.head;
            const line = update.state.doc.lineAt(head);
            callbacks.current.onCursor({ ln: line.number, col: head - line.from + 1 });
          }
        }),
        EditorView.domEventHandlers({
          paste(event: ClipboardEvent): boolean {
            const items = event.clipboardData?.items;
            if (!items) return false;
            for (const item of items) {
              if (!item.type.startsWith("image/")) continue;
              const file = item.getAsFile();
              if (!file) continue;
              event.preventDefault();
              const ext = IMAGE_EXTENSIONS[file.type] ?? "png";
              const reader = new FileReader();
              reader.onload = () => {
                const dataUrl = String(reader.result ?? "");
                const base64 = dataUrl.slice(dataUrl.indexOf(",") + 1);
                void callbacks.current
                  .onPasteImage(base64, ext)
                  .then((key) => {
                    if (!key) return;
                    const snippet = `![](${key})`;
                    const range = view.state.selection.main;
                    view.dispatch({
                      changes: { from: range.from, to: range.to, insert: snippet },
                      selection: { anchor: range.from + snippet.length },
                    });
                    view.focus();
                  });
              };
              reader.readAsDataURL(file);
              return true;
            }
            return false;
          },
        }),
      ],
    });

    viewRef.current = view;
    callbacks.current.onView(view);

    const onScroll = () => {
      const el = view.scrollDOM;
      const max = el.scrollHeight - el.clientHeight;
      callbacks.current.onScrollRatio(max > 0 ? el.scrollTop / max : 0);
    };
    view.scrollDOM.addEventListener("scroll", onScroll);

    requestAnimationFrame(() => view.focus());

    return () => {
      view.scrollDOM.removeEventListener("scroll", onScroll);
      viewRef.current = null;
      callbacks.current.onView(null);
      view.destroy();
    };
    // Editor is remounted via key when the document changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return <div ref={hostRef} className="editor-host" />;
}
