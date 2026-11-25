import type { TextSize } from "../shell/useMailApp";

const IFRAME_ZOOM: Record<TextSize, number> = {
  small: 1,
  medium: 1.14,
  large: 1.28,
};

/**
 * Injects inline reader styles (frame breathing room and optional text zoom)
 * into a sanitized email `srcdoc` so both plain-text and inline-styled HTML
 * messages have comfortable padding from the iframe edge and scale smoothly
 * with the independent reader size. Safe under the existing
 * `style-src 'unsafe-inline'` iframe CSP.
 */
export function applyReaderTextSize(srcdoc: string, size: TextSize): string {
  const zoom = IFRAME_ZOOM[size];
  const zoomRule = zoom === 1 ? "" : `zoom:${zoom};`;
  const styleTag = `<style>html{box-sizing:border-box;padding:14px 16px!important;${zoomRule}}</style>`;
  const bodyClose = srcdoc.lastIndexOf("</body>");
  if (bodyClose !== -1) {
    return `${srcdoc.slice(0, bodyClose)}${styleTag}${srcdoc.slice(bodyClose)}`;
  }
  if (srcdoc.includes("</head>")) {
    return srcdoc.replace("</head>", `${styleTag}</head>`);
  }
  return `${styleTag}${srcdoc}`;
}
