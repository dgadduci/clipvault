/**
 * Pure helpers for positioning and laying out the card-menu popover
 * the `preview-interaction-regressions` change introduces.
 *
 * The card menu used to render inline inside the card's
 * `position: absolute` flow; the manual regression confirmed that
 * the menu could be clipped by the card's `overflow: hidden`,
 * truncated by the rail, or partially hidden when the trigger sits
 * near the viewport's bottom edge. The helpers in this module:
 *
 *   - compute a `position: fixed` rectangle that stays inside the
 *     viewport, flipping above the trigger when there is not enough
 *     room below;
 *   - bound the popover width and the visible item count so a
 *     short viewport still scrolls inside the menu (never the card);
 *   - publish the canonical shortcut list so the menu items can
 *     render the platform-aware hint next to actions that actually
 *     have one (the spec forbids invented shortcuts);
 *   - never carry clipboard content, snippets, hashes, asset
 *     references or paths — the metadata-only contract is the
 *     reason the helpers are pure and DOM-free.
 *
 * The helpers are intentionally framework-agnostic: the
 * `HistoryCard.svelte` component reads them through the menu's
 * `style` binding and renders the items in document order. Unit
 * tests in `tests/cardMenuPositioning.test.ts` pin the math
 * byte-for-byte so a regression that breaks the bottom-edge flip
 * or the right-edge clamp surfaces in CI.
 */

import {
  previewShortcutAccessibleLabel,
  previewShortcutLabel,
  type PreviewShortcutPlatform,
} from "./clipboardPreview.ts";

/**
 * Dimensions the card-menu popover reserves. The width mirrors the
 * CSS rule on `.menu`; the max-height / item-height pair is the
 * floor the `max-height` clamp uses so the helper never over-runs
 * the visible viewport when the trigger sits near the top edge.
 *
 * The popover is visually attached to the trigger button: the gap
 * between `trigger.bottom` and `popover.top` (or `trigger.top` and
 * `popover.bottom` when flipped) is exactly `0` so the menu reads
 * as a continuation of the `...` button. The gutter is reserved
 * for the popover's distance from the viewport edges only — never
 * from the trigger.
 */
export const CARD_MENU_WIDTH = 176;
export const CARD_MENU_MIN_HEIGHT = 220;
export const CARD_MENU_GUTTER = 8;
/**
 * Vertical distance between the trigger button and the popover
 * edge. `0` pins the popover to the trigger's edge so the menu
 * reads as a visual continuation of the `...` button. The
 * regression suite asserts on this contract so the popover never
 * drifts back to a 4 px gap.
 */
export const CARD_MENU_TRIGGER_GAP = 0;

/**
 * Platform-aware shortcut displayed next to the `Previsualizar`
 * menu item. The label mirrors the visible hint the rail surfaces
 * and the matcher `matchesPreviewShortcut` consumes so a future
 * tweak to the modifier table lands in all three places at once.
 */
export function cardMenuPreviewShortcutLabel(
  platform: PreviewShortcutPlatform,
): string {
  return previewShortcutLabel(platform);
}

/** Accessible label the menu item exposes through `aria-label`. */
export function cardMenuPreviewShortcutAccessibleLabel(
  platform: PreviewShortcutPlatform,
): string {
  return previewShortcutAccessibleLabel(platform);
}

/**
 * Stable `aria-keyshortcuts` value the menu item exposes. The
 * string follows the WAI-ARIA `aria-keyshortcuts` syntax
 * (modifier + "+" + key, separated by spaces). The helper keeps
 * the rendering of this attribute out of the component template
 * so a future platform tweak only has to land here.
 */
export function cardMenuPreviewShortcutKeyAttribute(
  platform: PreviewShortcutPlatform,
): string {
  return platform === "macos" ? "Meta+Enter" : "Control+Enter";
}

export interface CardMenuTriggerRect {
  top: number;
  left: number;
  right: number;
  bottom: number;
  width: number;
  height: number;
}

export interface CardMenuViewport {
  width: number;
  height: number;
}

export interface CardMenuPosition {
  top: number;
  left: number;
  width: number;
  maxHeight: number;
  /** Whether the popover flipped above the trigger. */
  flippedAbove: boolean;
  /** Whether the popover is allowed to scroll internally. */
  scrollable: boolean;
}

/**
 * Resolve the popover rectangle the card menu renders into.
 *
 * The helper is total: any combination of viewport + trigger
 * produces a deterministic, clamped rectangle the menu can paint
 * without ever escaping the visible viewport. The four cases the
 * helper handles:
 *
 *   - normal layout: popover sits below the trigger, anchored to
 *     its right edge;
 *   - trigger near the bottom: popover flips above the trigger;
 *   - trigger near the right edge: left edge is clamped to the
 *     viewport's right inset;
 *   - very short viewport: `max-height` is bounded and the popover
 *     can scroll internally.
 *
 * The helper reads no DOM; the caller passes the trigger's
 * bounding rect (a `DOMRect`-shaped object) and the viewport size
 * it observed. This keeps the test suite free of jsdom.
 */
export function computeCardMenuPosition(
  trigger: CardMenuTriggerRect,
  viewport: CardMenuViewport,
): CardMenuPosition {
  const width = CARD_MENU_WIDTH;
  // Right-aligned to the trigger: popover's right edge matches the
  // trigger's right edge so the menu reads as a continuation of
  // the card. Clamp to the viewport inset so the popover never
  // escapes the right edge.
  const desiredLeft = trigger.right - width;
  const left = Math.max(
    CARD_MENU_GUTTER,
    Math.min(desiredLeft, viewport.width - width - CARD_MENU_GUTTER),
  );
  const spaceBelow = viewport.height - trigger.bottom - CARD_MENU_GUTTER;
  const spaceAbove = trigger.top - CARD_MENU_GUTTER;
  const flippedAbove =
    spaceBelow < CARD_MENU_MIN_HEIGHT && spaceAbove > spaceBelow;
  const availableHeight = flippedAbove ? spaceAbove : spaceBelow;
  const maxHeight = Math.max(
    CARD_MENU_MIN_HEIGHT / 2,
    Math.min(availableHeight, viewport.height - 2 * CARD_MENU_GUTTER),
  );
  // Pin the popover to the trigger edge. The 4 px gap the previous
  // baseline reserved between trigger and popover made the menu
  // read as detached from the `...` button; the regression suite
  // asserts the popover's top (or bottom, when flipped) lands on
  // the trigger's edge.
  const top = flippedAbove
    ? Math.max(
        CARD_MENU_GUTTER,
        trigger.top - maxHeight - CARD_MENU_TRIGGER_GAP,
      )
    : Math.min(
        trigger.bottom + CARD_MENU_TRIGGER_GAP,
        viewport.height - maxHeight - CARD_MENU_GUTTER,
      );
  return {
    top,
    left,
    width,
    maxHeight,
    flippedAbove,
    scrollable: availableHeight < CARD_MENU_MIN_HEIGHT,
  };
}

/**
 * Build the inline `style` string the menu binds to its
 * `position: fixed` element. The helper exists so the component
 * template stays a thin renderer and the regression suite can
 * assert on the produced string.
 *
 * The string ALWAYS carries `position: fixed` because the popover
 * must escape the card's clipping context: the card uses
 * `overflow: hidden` and renders the popover inline, so an
 * absolutely-positioned popover would be clipped by the card's
 * own border-box. `position: fixed` makes the popover position
 * itself against the viewport (the card does not create a
 * containing block for fixed-position descendants) so the menu
 * is always visible regardless of the card's nesting.
 *
 * The `z-index` value mirrors the documented popover layer the
 * rest of the desktop surfaces (`999`) so the menu always sits
 * above the card, the rail, the toolbar and the parent window
 * even when the trigger is the very last card on the rail.
 */
export function cardMenuStyle(position: CardMenuPosition): string {
  const overflow = position.scrollable ? "overflow-y: auto;" : "";
  return [
    `position: fixed;`,
    `top: ${Math.round(position.top)}px;`,
    `left: ${Math.round(position.left)}px;`,
    `width: ${position.width}px;`,
    `max-height: ${Math.round(position.maxHeight)}px;`,
    `z-index: 999;`,
    overflow,
  ]
    .join(" ")
    .trim();
}

/**
 * Re-anchor the popover rectangle after the popover has been
 * rendered, so `popover.bottom === trigger.top` (when the menu
 * flipped above) holds even when the popover's actual content is
 * shorter than the clamped `maxHeight` the first pass returned.
 *
 * The first pass uses `maxHeight` (the upper bound the helper
 * could safely reserve without escaping the viewport) so the
 * popover does not overflow before its content has been laid out.
 * Once the popover has painted the caller can measure its real
 * `height` with `getBoundingClientRect()` and re-run the math
 * against that value. The resulting `top` anchors the visible
 * bottom edge to `trigger.top` regardless of how short the
 * rendered content is, while the viewport-aware clamps are
 * preserved.
 *
 * When the popover drops below the trigger no re-anchoring is
 * necessary — the first pass already pinned `top` to
 * `trigger.bottom`. The helper still updates `maxHeight` so the
 * rendered `<div>` does not retain a stale ceiling.
 *
 * The helper reads no DOM; the caller passes the original
 * position, the trigger's bounding rect and the actual rendered
 * height of the popover. This keeps the test suite free of jsdom
 * and lets the regression suite exercise the second pass
 * byte-for-byte.
 */
export function recomputeCardMenuPositionForActualHeight(
  position: CardMenuPosition,
  trigger: CardMenuTriggerRect,
  viewport: CardMenuViewport,
  popoverHeight: number,
): CardMenuPosition {
  if (!Number.isFinite(popoverHeight) || popoverHeight <= 0) {
    return position;
  }
  if (position.flippedAbove) {
    const desiredTop = trigger.top - popoverHeight - CARD_MENU_TRIGGER_GAP;
    const top = Math.max(
      CARD_MENU_GUTTER,
      Math.min(
        desiredTop,
        viewport.height - popoverHeight - CARD_MENU_GUTTER,
      ),
    );
    return {
      ...position,
      top,
      maxHeight: Math.min(
        popoverHeight,
        viewport.height - 2 * CARD_MENU_GUTTER,
      ),
    };
  }
  // Drop below: re-clamp `maxHeight` to the popover's actual
  // height so the inline style never claims more vertical room
  // than the rendered content actually consumes. The `top`
  // already equals `trigger.bottom` from the first pass so the
  // helper leaves it is the safe default.
  return {
    ...position,
    maxHeight: Math.min(
      popoverHeight,
      viewport.height - 2 * CARD_MENU_GUTTER,
    ),
  };
}
