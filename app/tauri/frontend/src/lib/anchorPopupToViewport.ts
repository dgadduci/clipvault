export type ViewportPopupOptions = {
  anchor: () => HTMLElement | null;
  align?: "start" | "end";
  matchAnchorWidth?: boolean;
};

/**
 * Move a popup out of its toolbar scroll container and keep it aligned
 * with its trigger in viewport coordinates. Overflow containers clip
 * descendants even when those descendants have a high z-index.
 */
export function anchorPopupToViewport(
  node: HTMLElement,
  options: ViewportPopupOptions,
): { destroy: () => void } {
  if (typeof document === "undefined" || typeof window === "undefined") {
    return { destroy: () => {} };
  }

  document.body.appendChild(node);
  node.style.position = "fixed";
  node.style.boxSizing = "border-box";
  node.style.overflowY = "auto";
  const declaredMaxHeight = Number.parseFloat(getComputedStyle(node).maxHeight);

  function positionPopup(): void {
    const anchor = options.anchor();
    if (!anchor?.isConnected || !node.isConnected) return;

    const anchorRect = anchor.getBoundingClientRect();
    const inset = 8;
    const gap = 6;
    const availableWidth = Math.max(0, window.innerWidth - inset * 2);
    const availableHeight = Math.max(0, window.innerHeight - inset * 2);

    if (options.matchAnchorWidth) {
      node.style.width = `${Math.min(anchorRect.width, availableWidth)}px`;
      node.style.minWidth = "0";
    }
    node.style.maxWidth = `${availableWidth}px`;

    const preferredMaxHeight = Number.isFinite(declaredMaxHeight)
      ? declaredMaxHeight
      : availableHeight;
    node.style.maxHeight = `${Math.min(preferredMaxHeight, availableHeight)}px`;

    const popupRect = node.getBoundingClientRect();
    const popupWidth = Math.min(popupRect.width, availableWidth);
    const popupHeight = Math.min(popupRect.height, availableHeight);
    const preferredLeft = options.align === "end"
      ? anchorRect.right - popupWidth
      : anchorRect.left;
    const left = Math.max(inset, Math.min(preferredLeft, window.innerWidth - inset - popupWidth));
    const below = anchorRect.bottom + gap;
    const top = below + popupHeight <= window.innerHeight - inset
      ? below
      : Math.max(inset, anchorRect.top - gap - popupHeight);

    node.style.left = `${left}px`;
    node.style.top = `${top}px`;
  }

  positionPopup();
  window.addEventListener("resize", positionPopup);
  window.addEventListener("scroll", positionPopup, true);

  return {
    destroy() {
      window.removeEventListener("resize", positionPopup);
      window.removeEventListener("scroll", positionPopup, true);
    },
  };
}
