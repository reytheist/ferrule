import type { TextSize } from "../shell/useMailApp";

const IFRAME_ZOOM: Record<TextSize, number> = {
  small: 1,
  medium: 1.14,
  large: 1.28,
};

/**
 * Injects an inline style zoom into a sanitized email `srcdoc` so both plain-text
 * and inline-styled HTML messages scale smoothly with the independent reader size.
 * Safe under the existing `style-src 'unsafe-inline'` iframe CSP.
 */
export function applyReaderTextSize(srcdoc: string, size: TextSize): string {
  const zoom = IFRAME_ZOOM[size];
  if (zoom === 1) return srcdoc;
  const styleTag = `<style>html{zoom:${zoom};}</style>`;
  if (srcdoc.includes("</head>")) {
    return srcdoc.replace("</head>", `${styleTag}</head>`);
  }
  return `${styleTag}${srcdoc}`;
}
