/**
 * Bottom status bar: document state, cursor position, counts, messages.
 */

import type { CursorInfo } from "../lib/types";

interface StatusBarProps {
  fileName: string;
  filePath: string | null;
  dirty: boolean;
  cursor: CursorInfo;
  words: number;
  chars: number;
  assets: number;
  message: string;
}

export default function StatusBar({
  fileName,
  filePath,
  dirty,
  cursor,
  words,
  chars,
  assets,
  message,
}: StatusBarProps) {
  return (
    <footer className="statusbar">
      <span className="sb-item sb-file" title={filePath ?? "Not saved yet"}>
        <span className={`sb-dot ${dirty ? "dirty" : ""}`} aria-hidden="true" />
        {fileName}
      </span>
      <span className="sb-item sb-message">{message}</span>
      <span className="sb-spacer" />
      <span className="sb-item">Ln {cursor.ln}, Col {cursor.col}</span>
      <span className="sb-item">{words} words</span>
      <span className="sb-item">{chars} chars</span>
      <span className="sb-item">{assets} assets</span>
    </footer>
  );
}
