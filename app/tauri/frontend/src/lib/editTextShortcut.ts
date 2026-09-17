// Pure helpers for the `Cmd/Ctrl+E` shortcut the Desktop install to
// open the in-window text editor on an eligible card.
//
// The matcher is intentionally minimal and shared between the desktop
// shell listener and the per-card matcher `HistoryCard.svelte` keeps
// for compatibility: any platform tweak lands in this one helper so
// the visible hint (`editTextShortcutLabel`), the accessible label
// (`editTextShortcutAccessibleLabel`), the WAI-ARIA `aria-keyshortcuts`
// attribute (`editTextShortcutKeyAttribute`) and the matcher
// (`matchesEditTextShortcut`) cannot drift apart.
//
// `editTextShortcutPlatform` resolves the platform the diagnostics
// payload returned. It delegates to `searchShortcutPlatform` so a
// future tweak to the diagnostics string only has to land in
// `platformFromDiagnostics`.
//
// The matcher accepts:
//   - macOS: `metaKey` + `E` (with no `Shift` / `Alt` and no `ctrlKey`).
//   - every other supported host: `ctrlKey` + `E` (with no `Shift` /
//     `Alt` and no `metaKey`).
//
// The check is case-insensitive on `event.key` so a non-QWERTY layout
// still triggers the shortcut through the webview.
//
// Privacy: the matcher only reads the platform string the backend
// diagnostics emitted and the keyboard event modifiers; it never
// inspects clipboard content, asset references, hashes, identifiers
// or paths.

import {
  searchShortcutPlatform,
  type SearchShortcutPlatform,
} from "./searchShortcut.ts";

/**
 * Platform the edit-text shortcut listener cares about. Mirrors
 * `SearchShortcutPlatform` so the helper never introduces a second
 * platform enum the rest of the UI has to import alongside the
 * search-shortcut one.
 */
export type EditTextShortcutPlatform = SearchShortcutPlatform;

/**
 * Resolve the platform the edit-text shortcut should match against.
 * Accepts the same string the diagnostics payload returns so the
 * shell can derive the value without importing the larger `platform`
 * module.
 */
export function editTextShortcutPlatform(
  diagnosticsPlatformOs: string | null | undefined,
): EditTextShortcutPlatform {
  return searchShortcutPlatform(diagnosticsPlatformOs);
}

/** Letter the platform shortcut triggers. Exposed so the visible hint
 *  and the matcher cannot drift apart. */
export const EDIT_TEXT_SHORTCUT_LETTER = "e";

/**
 * Visible label the menu item renders next to the `Editar captura`
 * affordance. The label mirrors the matcher: `⌘E` on macOS, `Ctrl+E`
 * everywhere else.
 */
export function editTextShortcutLabel(
  platform: EditTextShortcutPlatform,
): string {
  return platform === "macos" ? "⌘E" : "Ctrl+E";
}

/**
 * Long-form accessible label matching the visible hint. Used by the
 * menu item's accessible name and any tooltip the rail renders.
 */
export function editTextShortcutAccessibleLabel(
  platform: EditTextShortcutPlatform,
): string {
  return platform === "macos"
    ? "Editar captura (Comando E)"
    : "Editar captura (Control E)";
}

/**
 * Stable `aria-keyshortcuts` value the menu item exposes. The string
 * follows the WAI-ARIA syntax (`Meta+E` on macOS, `Control+E`
 * everywhere else).
 */
export function editTextShortcutKeyAttribute(
  platform: EditTextShortcutPlatform,
): string {
  return platform === "macos" ? "Meta+E" : "Control+E";
}

/**
 * Resolve the entry targeted by the desktop shortcut. The rail selection is
 * authoritative after a pointer selection because browser focus may still
 * remain on the card that previously opened the editor. A focused card is
 * only the fallback for keyboard navigation when there is no rail selection.
 */
export function resolveEditTextShortcutEntryId(
  selectedEntryId: number | null,
  focusedCardEntryId: number | null,
): number | null {
  return selectedEntryId ?? focusedCardEntryId;
}

/**
 * Whether a keyboard event matches the platform-specific edit-text
 * shortcut. The check uses the `key` field (case-insensitive) so a
 * non-QWERTY layout still triggers the shortcut through the
 * webview. `event.key === "E"` is explicitly covered because a user
 * with Caps Lock held will see the platform report the uppercase
 * letter.
 */
export function matchesEditTextShortcut(
  event: Pick<
    KeyboardEvent,
    "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey"
  >,
  platform: EditTextShortcutPlatform,
): boolean {
  if (event.altKey || event.shiftKey) return false;
  const key = (event.key ?? "").toLowerCase();
  if (key !== EDIT_TEXT_SHORTCUT_LETTER) return false;
  return platform === "macos"
    ? Boolean(event.metaKey) && !event.ctrlKey
    : Boolean(event.ctrlKey) && !event.metaKey;
}
