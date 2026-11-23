import type { Anchor } from "../shell/useMailApp";

/** Viewport anchor just below an element, for popovers opened from a click. */
export function anchorBelow(element: HTMLElement): Anchor {
  const rect = element.getBoundingClientRect();
  return { x: rect.left, y: rect.bottom + 6 };
}

/** Anchor near the middle of the viewport (keyboard-triggered popovers). */
export function anchorCenter(): Anchor {
  return {
    x: Math.max(12, window.innerWidth / 2 - 118),
    y: Math.max(12, window.innerHeight / 2 - 110),
  };
}
