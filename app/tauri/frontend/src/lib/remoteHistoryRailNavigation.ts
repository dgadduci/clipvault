/**
 * Pure guard helpers the remote-history rail's document-level
 * keydown listener uses to decide whether the user-pressed
 * `ArrowLeft` / `ArrowRight` should move the rail-owned
 * selection or fall through to the native horizontal scroll.
 *
 * The helpers are intentionally DOM-only — the file has no
 * Svelte or Tauri dependencies — so the keyboard contract can
 * be exercised in unit tests without standing up a Svelte
 * component. Every entry point takes the minimal typed shape
 * it needs (a target element) and never reaches for a global
 * or document selector so the contract stays auditable.
 *
 * The guards are split so each scenario the
 * `peer-remote-preview-card-ux` change lists pins a separate
 * regression test:
 *
 *   - `mapHorizontalArrowKey` rejects every key that is not
 *     `ArrowLeft` / `ArrowRight`;
 *   - `isInteractiveControl` rejects HTML controls, anchors,
 *     buttons, selects, contentEditable surfaces and custom
 *     button/menu roles. The rail's own `listbox` and `option`
 *     roles are allowed because arrow keys navigate its cards;
 *   - `isInsideRailSurface` walks the parent chain and
 *     accepts only the focused element whose nearest
 *     `data-testid="remote-history-rail"` ancestor exists.
 */

export type HorizontalRailKeyDirection = "left" | "right";

/**
 * Translate the raw `KeyboardEvent.key` value into the typed
 * direction the rail's pure navigation helper consumes.
 * Returns `null` for every other key so the caller can
 * short-circuit without a second guard.
 */
export function mapHorizontalArrowKey(
  key: string,
): HorizontalRailKeyDirection | null {
  if (key === "ArrowRight") return "right";
  if (key === "ArrowLeft") return "left";
  return null;
}

/**
 * Decide whether the focused element should consume the
 * keypress or fall through to the next handler. The helper
 * never throws; an unknown element collapses to a
 * "non-interactive" → consumed-key → continue decision so a
 * regression in the DOM API cannot leak the key.
 *
 * The interactive set is the union of the native HTML
 * controls the browser already routes (text inputs, text
 * areas, selects, buttons, anchors) plus the ARIA roles the
 * rail uses for its custom menu surface. The helper
 * recognises the HTML controls by `tagName` (uppercased) so
 * the test polyfill — where every HTML element shares the
 * same `DomElement` constructor — can still exercise the
 * guards; the runtime behaviour is identical to a true
 * `instanceof HTMLInputElement` check on every browser.
 */
export function isInteractiveControl(target: Element | null): boolean {
  if (target === null) return false;
  const tagName = target.tagName ? target.tagName.toUpperCase() : "";
  if (
    tagName === "INPUT" ||
    tagName === "TEXTAREA" ||
    tagName === "SELECT" ||
    tagName === "BUTTON" ||
    tagName === "A"
  ) {
    return true;
  }
  if (
    target instanceof HTMLElement &&
    (target.isContentEditable ||
      (target.getAttribute("contenteditable") !== null &&
        target.getAttribute("contenteditable") !== "false"))
  ) {
    return true;
  }
  if (
    target instanceof HTMLElement &&
    target.hasAttribute("role") &&
    (target.getAttribute("role") === "button" ||
      target.getAttribute("role") === "menuitem" ||
      target.getAttribute("role") === "menu")
  ) {
    return true;
  }
  return false;
}

/**
 * Walk the parent chain from `target` up to the document
 * root and return `true` only when the focused element
 * descends from the rail root the template renders with
 * `data-testid="remote-history-rail"`. The helper returns
 * `false` when the element lives outside the rail surface
 * (sidebar button, search field, link, …) so the document-
 * level listener never steals the key from a control that
 * sits in a different panel.
 *
 * The walk uses the standard `Element.parentElement` accessor
 * (with a `parentNode` fallback for the test polyfill where
 * only `parentNode` is wired). A regression that drops the
 * polyfill from the production tree is caught at the first
 * assertion because the live runtime resolves
 * `parentElement` exactly the same way.
 */
export function isInsideRailSurface(target: Element | null): boolean {
  if (target === null) return false;
  let current: Element | null = target;
  while (current !== null) {
    if (
      current instanceof HTMLElement &&
      current.dataset &&
      current.dataset.testid === "remote-history-rail"
    ) {
      return true;
    }
    const next: Element | null =
      typeof (current as ParentNode).parentElement !== "undefined"
        ? (current as ParentNode).parentElement
        : ((current as unknown as { parentNode: Element | null })
            .parentNode ?? null);
    current = next;
  }
  return false;
}

/**
 * Single entry point the rail's keydown listener calls with
 * the focused element. Returns `true` only when the caller
 * should consume the keypress; the helper composes the
 * three primitives above so every entry point stays
 * consistent with the rules.
 */
export function shouldConsumeHorizontalRailKey(
  key: string,
  target: Element | null,
): boolean {
  return (
    mapHorizontalArrowKey(key) !== null &&
    !isInteractiveControl(target) &&
    isInsideRailSurface(target)
  );
}
