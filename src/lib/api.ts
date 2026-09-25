/** Typed wrappers around the Tauri backend commands and dialogs. */

import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { DocumentPayload } from "./types";

export function newDocument(): Promise<DocumentPayload> {
  return invoke<DocumentPayload>("new_document");
}

export function openMdex(path: string): Promise<DocumentPayload> {
  return invoke<DocumentPayload>("open_mdex", { path });
}

export function saveMdex(
  path: string | null,
  markdown: string,
): Promise<DocumentPayload> {
  return invoke<DocumentPayload>("save_mdex", { path, markdown });
}

export function importMarkdown(path: string): Promise<DocumentPayload> {
  return invoke<DocumentPayload>("import_markdown", { path });
}

export function addAsset(filePath: string): Promise<string> {
  return invoke<string>("add_asset", { filePath });
}

export function addAssetBytes(
  name: string,
  base64Data: string,
): Promise<string> {
  return invoke<string>("add_asset_bytes", { name, base64Data });
}

export function exportMarkdown(path: string, markdown: string): Promise<void> {
  return invoke<void>("export_markdown", { path, markdown });
}

export function exportHtml(path: string, html: string): Promise<void> {
  return invoke<void>("export_html", { path, html });
}

/** Consume a file path passed to the process at launch (OS association). */
export function getStartupFile(): Promise<string | null> {
  return invoke<string | null>("get_startup_file");
}

/** True when mdex is registered as an open handler for .md/.mdex. */
export function fileAssociationsRegistered(): Promise<boolean> {
  return invoke<boolean>("file_associations_registered");
}

/** Register mdex as the default "Open with" app for .md/.mdex (Windows). */
export function registerFileAssociations(): Promise<void> {
  return invoke<void>("register_file_associations");
}

/** Remove the file-association registration written above. */
export function unregisterFileAssociations(): Promise<void> {
  return invoke<void>("unregister_file_associations");
}

/** Ask the user to pick an existing .mdex or .md file. */
export async function pickDocumentToOpen(): Promise<string | null> {
  const selection = await open({
    multiple: false,
    directory: false,
    title: "Open document",
    filters: [
      { name: "mdex documents", extensions: ["mdex"] },
      { name: "Markdown files", extensions: ["md"] },
    ],
  });
  return typeof selection === "string" ? selection : null;
}

/** Ask the user to pick an image file to embed as an asset. */
export async function pickImageToAdd(): Promise<string | null> {
  const selection = await open({
    multiple: false,
    directory: false,
    title: "Add image",
    filters: [
      {
        name: "Images",
        extensions: ["png", "jpg", "jpeg", "gif", "webp", "avif", "bmp", "svg"],
      },
    ],
  });
  return typeof selection === "string" ? selection : null;
}

/** Ask the user where to save a file. */
export async function pickSaveTarget(
  extensions: string[],
  filterName: string,
  defaultName: string,
): Promise<string | null> {
  const target = await save({
    title: "Save as",
    filters: [{ name: filterName, extensions }],
    defaultPath: defaultName,
  });
  return typeof target === "string" ? target : null;
}
