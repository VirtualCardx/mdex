/** Shared types mirroring the Rust payloads (serde camelCase). */

export interface MdexMeta {
  format: string;
  version: number;
  title?: string;
  created: string;
  modified: string;
}

export interface DocumentPayload {
  path: string | null;
  fileName: string;
  markdown: string;
  assetKeys: string[];
  meta: MdexMeta;
}

export type Theme = "dark" | "light";

export type ViewMode = "edit" | "split" | "preview";

export interface CursorInfo {
  ln: number;
  col: number;
}
