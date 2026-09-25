/**
 * Top toolbar: file operations, formatting actions, view mode and theme.
 */

import type { Theme, ViewMode } from "../lib/types";

export type ToolbarAction =
  | "new"
  | "open"
  | "save"
  | "saveAs"
  | "import"
  | "exportMd"
  | "exportHtml"
  | "image"
  | "fileAssoc"
  | "bold"
  | "italic"
  | "strike"
  | "codeInline"
  | "codeBlock"
  | "h1"
  | "h2"
  | "h3"
  | "quote"
  | "ul"
  | "ol"
  | "task"
  | "link"
  | "table"
  | "hr";

interface ToolbarProps {
  onAction: (id: ToolbarAction) => void;
  mode: ViewMode;
  onModeChange: (mode: ViewMode) => void;
  theme: Theme;
  onThemeToggle: () => void;
}

interface ButtonSpec {
  id: ToolbarAction;
  label: string;
  title: string;
  className?: string;
}

const FILE_BUTTONS: ButtonSpec[] = [
  { id: "new", label: "New", title: "New document (Ctrl+N)" },
  { id: "open", label: "Open", title: "Open .mdex or .md (Ctrl+O)" },
  { id: "save", label: "Save", title: "Save (Ctrl+S)" },
  { id: "saveAs", label: "Save As", title: "Save as .mdex (Ctrl+Shift+S)" },
];

const IO_BUTTONS: ButtonSpec[] = [
  { id: "import", label: "Import .md", title: "Import a plain markdown file" },
  { id: "exportMd", label: "Export .md", title: "Export markdown + assets folder" },
  { id: "exportHtml", label: "Export .html", title: "Export self-contained HTML" },
  { id: "image", label: "Image", title: "Embed an image into the document" },
  { id: "fileAssoc", label: "\u2699 Link files", title: "Register mdex as the default app for .md / .mdex files" },
];

const FORMAT_BUTTONS: ButtonSpec[] = [
  { id: "bold", label: "B", title: "Bold (**text**)", className: "tb-bold" },
  { id: "italic", label: "I", title: "Italic (*text*)", className: "tb-italic" },
  { id: "strike", label: "S", title: "Strikethrough (~~text~~)", className: "tb-strike" },
  { id: "codeInline", label: "</>", title: "Inline code (`code`)" },
  { id: "codeBlock", label: "{ }", title: "Code block (```)" },
  { id: "h1", label: "H1", title: "Heading 1" },
  { id: "h2", label: "H2", title: "Heading 2" },
  { id: "h3", label: "H3", title: "Heading 3" },
  { id: "quote", label: "\u201D", title: "Blockquote" },
  { id: "ul", label: "\u2022 \u2014", title: "Bullet list" },
  { id: "ol", label: "1.", title: "Numbered list" },
  { id: "task", label: "\u2610", title: "Task list" },
  { id: "link", label: "Link", title: "Insert link" },
  { id: "table", label: "\u25A6", title: "Insert table" },
  { id: "hr", label: "\u2015", title: "Horizontal rule" },
];

const MODES: ViewMode[] = ["edit", "split", "preview"];

function renderGroup(buttons: ButtonSpec[], onAction: (id: ToolbarAction) => void) {
  return buttons.map((button) => (
    <button
      key={button.id}
      type="button"
      className={`tb-btn ${button.className ?? ""}`}
      title={button.title}
      onClick={() => onAction(button.id)}
    >
      {button.label}
    </button>
  ));
}

export default function Toolbar({
  onAction,
  mode,
  onModeChange,
  theme,
  onThemeToggle,
}: ToolbarProps) {
  return (
    <div className="toolbar" role="toolbar" aria-label="mdex toolbar">
      <div className="tb-group">{renderGroup(FILE_BUTTONS, onAction)}</div>
      <div className="tb-sep" />
      <div className="tb-group">{renderGroup(IO_BUTTONS, onAction)}</div>
      <div className="tb-sep" />
      <div className="tb-group">{renderGroup(FORMAT_BUTTONS, onAction)}</div>
      <div className="tb-spacer" />
      <div className="tb-group tb-modes" role="tablist" aria-label="view mode">
        {MODES.map((m) => (
          <button
            key={m}
            type="button"
            className={`tb-btn tb-mode ${mode === m ? "active" : ""}`}
            title={`${m} view`}
            onClick={() => onModeChange(m)}
          >
            {m === "edit" ? "Edit" : m === "split" ? "Split" : "Preview"}
          </button>
        ))}
      </div>
      <div className="tb-sep" />
      <button
        type="button"
        className="tb-btn"
        title={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
        onClick={onThemeToggle}
      >
        {theme === "dark" ? "\u263D" : "\u2600"}
      </button>
    </div>
  );
}
