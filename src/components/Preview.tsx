/**
 * Live markdown preview. External links open in the system browser;
 * in-page anchors and asset images are handled natively.
 */

import { forwardRef } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

interface PreviewProps {
  html: string;
}

const Preview = forwardRef<HTMLDivElement, PreviewProps>(function Preview(
  { html },
  ref,
) {
  const onClick = (event: React.MouseEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement;
    const anchor = target.closest("a");
    if (!anchor) return;
    const href = anchor.getAttribute("href") ?? "";
    if (/^https?:\/\//i.test(href)) {
      event.preventDefault();
      void openUrl(href).catch(() => undefined);
    } else if (href.startsWith("#") || href === "") {
      event.preventDefault();
    }
  };

  return (
    <div ref={ref} className="preview-pane" onClick={onClick}>
      {/* Content is rendered by markdown-it from the local document;
          html:true is an intentional feature for a local editor. */}
      <article className="md-body" dangerouslySetInnerHTML={{ __html: html }} />
    </div>
  );
});

export default Preview;
