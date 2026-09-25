/**
 * Throwaway runtime check for the preview pipeline fixes:
 * 1. task-list plugin renders checkboxes instead of literal [x]
 * 2. image rule rewrites relative src to mdexasset:// (per-segment encoded)
 */
import MarkdownIt from "markdown-it";

const ASSET_PROTOCOL = "http://mdexasset.localhost/"; // Windows bridge form
const assetUrl = (key) => ASSET_PROTOCOL + key.split("/").map(encodeURIComponent).join("/");

const md = new MarkdownIt({ html: true, linkify: true });
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
    children.unshift(checkbox);
    item.attrJoin("class", "task-list-item");
  }
  return true;
});
const defaultImageRule = md.renderer.rules.image;
md.renderer.rules.image = (tokens, idx, options, env, self) => {
  const token = tokens[idx];
  const src = token.attrGet("src") ?? "";
  const hasScheme = /^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(src);
  if (src && !hasScheme && !src.startsWith("#") && !src.startsWith("/")) {
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
  return defaultImageRule(tokens, idx, options, env, self);
};

const sample = [
  "# Title",
  "",
  "- [x] done thing",
  "- [ ] open thing",
  "- plain item",
  "",
  "![alt text](assets/my%20image.png)",
  "![plain](assets/icon.png)",
  "",
  "[link](https://example.com)",
].join("\n");

const html = md.render(sample);
console.log(html);

const checks = [
  ['checkbox checked', html.includes('type="checkbox" checked disabled')],
  ['checkbox unchecked', html.includes('type="checkbox" disabled')],
  ['task class on li', html.includes('class="task-list-item"')],
  ['no literal [x]', !html.includes("[x]")],
  ['asset url per-segment', html.includes(`src="${assetUrl("assets/my image.png")}"`)],
  ['asset url value', html.includes("http://mdexasset.localhost/assets/my%20image.png")],
  ['plain asset url', html.includes('src="http://mdexasset.localhost/assets/icon.png"')],
  ['no %2F', !html.includes("%2F")],
  ['no double encoding', !html.includes("%2520")],
];
let failed = 0;
for (const [name, ok] of checks) {
  console.log(`${ok ? "PASS" : "FAIL"} - ${name}`);
  if (!ok) failed++;
}
process.exit(failed === 0 ? 0 : 1);
