/**
 * Text transformations for the markdown toolbar. Each helper takes an
 * EditorView, applies a transaction, and refocuses the editor.
 */

import type { EditorView } from "@codemirror/view";

/** Wrap (or unwrap) the selection with markers like ** or ` . */
export function wrapSelection(view: EditorView, before: string, after = before): void {
  const { state } = view;
  const range = state.selection.main;
  const selected = state.sliceDoc(range.from, range.to);
  const minLen = before.length + after.length;

  if (
    selected.length >= minLen &&
    selected.startsWith(before) &&
    selected.endsWith(after)
  ) {
    const inner = selected.slice(before.length, selected.length - after.length);
    view.dispatch({
      changes: { from: range.from, to: range.to, insert: inner },
      selection: { anchor: range.from, head: range.from + inner.length },
    });
  } else {
    view.dispatch({
      changes: { from: range.from, to: range.to, insert: before + selected + after },
      selection: {
        anchor: range.from + before.length,
        head: range.from + before.length + selected.length,
      },
    });
  }
  view.focus();
}

/** Apply a transform to every selected line. */
function mapLines(
  view: EditorView,
  fn: (line: string, index: number) => string,
): void {
  const { state } = view;
  const range = state.selection.main;
  const first = state.doc.lineAt(range.from).number;
  const last = state.doc.lineAt(range.to).number;
  const changes = [];
  for (let n = first; n <= last; n++) {
    const line = state.doc.line(n);
    const next = fn(line.text, n - first);
    if (next !== line.text) {
      changes.push({ from: line.from, to: line.to, insert: next });
    }
  }
  if (changes.length > 0) {
    view.dispatch({ changes });
  }
  view.focus();
}

/** Toggle a line prefix such as "> " or "- "; removes when all lines have it. */
export function toggleLinePrefix(view: EditorView, prefix: string): void {
  const { state } = view;
  const range = state.selection.main;
  const first = state.doc.lineAt(range.from).number;
  const last = state.doc.lineAt(range.to).number;
  const lines: string[] = [];
  for (let n = first; n <= last; n++) lines.push(state.doc.line(n).text);
  const allHave = lines.every((l) => l.trim().length === 0 || l.trimStart().startsWith(prefix.trimStart()));
  mapLines(view, (line) => {
    if (allHave) {
      const trimmed = line.replace(/^(\s*)([-*+]\s\[ \]\s|[-*+]\s|>\s|\d+\.\s)/, "$1");
      return trimmed;
    }
    if (line.trim().length === 0) return line;
    return prefix + line;
  });
}

/** Toggle an ATX heading level; strips any existing heading first. */
export function toggleHeading(view: EditorView, level: number): void {
  const marker = "#".repeat(level) + " ";
  mapLines(view, (line) => {
    if (line.trim().length === 0) return line;
    const stripped = line.replace(/^(\s*)#{1,6}\s+/, "$1");
    const current = line.match(/^\s*(#{1,6})\s+/);
    // Clicking the same level removes it.
    if (current && current[1].length === level) return stripped;
    return marker + stripped;
  });
}

/** Turn selected lines into an ordered list (1. 2. 3. ...). */
export function toggleOrderedList(view: EditorView): void {
  const { state } = view;
  const range = state.selection.main;
  const first = state.doc.lineAt(range.from).number;
  const last = state.doc.lineAt(range.to).number;
  const anyNumbered = (() => {
    for (let n = first; n <= last; n++) {
      if (/^\s*\d+\.\s/.test(state.doc.line(n).text)) return true;
    }
    return false;
  })();
  if (anyNumbered) {
    mapLines(view, (line) => line.replace(/^(\s*)\d+\.\s+/, "$1"));
  } else {
    mapLines(view, (line, index) =>
      line.trim().length === 0 ? line : `${index + 1}. ${line}`,
    );
  }
}

/** Wrap selection with line-block markers (e.g. code fences). */
export function wrapBlock(view: EditorView, before: string, after: string): void {
  const { state } = view;
  const range = state.selection.main;
  const selected = state.sliceDoc(range.from, range.to);
  const insert = `${before}${selected}${after}`;
  view.dispatch({
    changes: { from: range.from, to: range.to, insert },
    selection: {
      anchor: range.from + before.length,
      head: range.from + before.length + selected.length,
    },
  });
  view.focus();
}

/** Insert arbitrary text at the cursor (replacing the selection). */
export function insertText(view: EditorView, text: string): void {
  const range = view.state.selection.main;
  view.dispatch({
    changes: { from: range.from, to: range.to, insert: text },
    selection: { anchor: range.from + text.length },
  });
  view.focus();
}

/** Insert a markdown link; uses selection as label or URL when it looks like one. */
export function insertLink(view: EditorView): void {
  const { state } = view;
  const range = state.selection.main;
  const selected = state.sliceDoc(range.from, range.to);
  const isUrl = /^(https?:\/\/|mailto:)/i.test(selected);
  const label = isUrl ? "link" : selected || "link";
  const url = isUrl ? selected : "https://";
  insertText(view, `[${label}](${url})`);
}

/** Insert an image reference for an archive asset. */
export function insertImageRef(view: EditorView, key: string, alt: string): void {
  const safeAlt = alt.replace(/[\[\]]/g, "");
  insertText(view, `![${safeAlt}](${key})`);
}

const TABLE_SNIPPET = `
| Column A | Column B |
| -------- | -------- |
| Cell     | Cell     |
`;

/** Toolbar action dispatcher. Returns false when the id is unknown. */
export function runFormatAction(view: EditorView, id: string): boolean {
  switch (id) {
    case "bold":
      wrapSelection(view, "**");
      return true;
    case "italic":
      wrapSelection(view, "*");
      return true;
    case "strike":
      wrapSelection(view, "~~");
      return true;
    case "codeInline":
      wrapSelection(view, "`");
      return true;
    case "codeBlock":
      wrapBlock(view, "```\n", "\n```");
      return true;
    case "h1":
      toggleHeading(view, 1);
      return true;
    case "h2":
      toggleHeading(view, 2);
      return true;
    case "h3":
      toggleHeading(view, 3);
      return true;
    case "quote":
      toggleLinePrefix(view, "> ");
      return true;
    case "ul":
      toggleLinePrefix(view, "- ");
      return true;
    case "ol":
      toggleOrderedList(view);
      return true;
    case "task":
      toggleLinePrefix(view, "- [ ] ");
      return true;
    case "link":
      insertLink(view);
      return true;
    case "table":
      insertText(view, TABLE_SNIPPET);
      return true;
    case "hr":
      insertText(view, "\n\n---\n\n");
      return true;
    default:
      return false;
  }
}
