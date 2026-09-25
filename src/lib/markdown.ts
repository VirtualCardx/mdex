/**
 * Markdown rendering pipeline.
 *
 * Relative image references (e.g. `assets/foo.png`) are rewritten to the
 * `mdexasset://` custom protocol, which is served from the loaded archive by
 * the Rust backend.
 */

import MarkdownIt from "markdown-it";
import hljs from "highlight.js";
// The package ships its own types; this shim is a fallback for older versions.
import mdKatex from "@vscode/markdown-it-katex";

// Windows/Android: wry serves custom protocols through an http bridge —
// `<scheme>://localhost/abc` is rewritten to `http://<scheme>.localhost/abc`
// (WebView2 does not fetch non-standard schemes; see wry's
// custom_protocol_workaround). Subresources like <img> must use the bridged
// form directly or the request never reaches the protocol handler.
// macOS/Linux keep the plain scheme URL.
const IS_WINDOWS_OR_ANDROID =
  /windows|android/i.test(navigator.userAgent);
export const ASSET_PROTOCOL = IS_WINDOWS_OR_ANDROID
  ? "http://mdexasset.localhost/"
  : "mdexasset://localhost/";

/**
 * Build the URL used to load an archive asset inside the webview.
 * Each path segment is encoded but slashes are kept, so the URL stays a
 * plain path — encoding the whole key would turn `/` into `%2F`, which
 * WebView2 URL canonicalization may rewrite.
 */
export function assetUrl(key: string): string {
  return ASSET_PROTOCOL + key.split("/").map(encodeURIComponent).join("/");
}

const md: MarkdownIt = new MarkdownIt({
  html: true,
  linkify: true,
  breaks: false,
  highlight(code: string, lang: string): string {
    if (lang && hljs.getLanguage(lang)) {
      try {
        return hljs.highlight(code, { language: lang, ignoreIllegals: true }).value;
      } catch {
        // fall through to default escaping
      }
    }
    return "";
  },
});

md.use(mdKatex);

/**
 * Minimal GFM task-list support: markdown-it renders `[x]` as literal text
 * by default. This core rule turns a leading `[ ]`/`[x]` in a list item
 * into a disabled checkbox and marks the item for styling.
 */
function taskListsPlugin(md: MarkdownIt): void {
  md.core.ruler.after("inline", "mdex-task-lists", (state) => {
    const tokens = state.tokens;
    for (let i = 2; i < tokens.length; i++) {
      if (tokens[i].type !== "inline") continue;
      if (tokens[i - 1].type !== "paragraph_open") continue;
      const item = tokens[i - 2];
      if (item.type !== "list_item_open") continue;
      const children = tokens[i].children;
      const first = children?.[0];
      if (!first || first.type !== "text") continue;
      const match = /^\[([ xX])\]\s+/.exec(first.content);
      if (!match) continue;
      const checked = match[1].toLowerCase() === "x";
      first.content = first.content.slice(match[0].length);
      const nested = first.children?.[0];
      if (nested && nested.type === "text") {
        nested.content = nested.content.slice(match[0].length);
      }
      const checkbox = new state.Token("html_inline", "", 0);
      checkbox.content = `<input class="task-list-item-checkbox" type="checkbox"${checked ? " checked" : ""} disabled> `;
      children!.unshift(checkbox);
      item.attrJoin("class", "task-list-item");
    }
    return true;
  });
}

md.use(taskListsPlugin);

// Rewrite relative image sources to the mdexasset:// protocol.
const defaultImageRule = md.renderer.rules.image;
md.renderer.rules.image = (tokens, idx, options, env, self) => {
  const token = tokens[idx];
  const src = token.attrGet("src") ?? "";
  const hasScheme = /^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(src);
  const isAnchor = src.startsWith("#");
  if (src && !hasScheme && !isAnchor && !src.startsWith("/")) {
    // The markdown destination may already be percent-encoded (a path with
    // spaces must be, e.g. `assets/my%20image.png`). Decode each segment
    // back to the raw archive key, then let assetUrl() encode it cleanly.
    const raw = src.replace(/\\/g, "/").replace(/^\.?\//, "");
    const key = raw
      .split("/")
      .map((seg) => {
        try {
          return decodeURIComponent(seg);
        } catch {
          return seg;
        }
      })
      .join("/");
    token.attrSet("src", assetUrl(key));
  }
  return defaultImageRule
    ? defaultImageRule(tokens, idx, options, env, self)
    : self.renderToken(tokens, idx, options);
};

export function renderMarkdown(source: string): string {
  return md.render(source);
}
