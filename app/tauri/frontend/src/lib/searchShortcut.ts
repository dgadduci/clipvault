// Pure helpers for the `Cmd/Ctrl+F` shortcut the desktop registers at
// the shell level.
//
// `isMacPlatform` returns the platform the search-shortcut listener
// should match against. It accepts the same string the diagnostics
// payload returns so the shell can derive the value without
// importing the larger `platform` module.
//
// `matchesSearchShortcut` decides whether a keyboard event qualifies
// as the search-shortcut press on the detected platform:
//   - macOS: Cmd + F (matches `metaKey`).
//   - everything else: Ctrl + F.
// The check explicitly excludes `Alt` / `Shift` modifiers so an
// accidental combination does not steal focus from a search input.

import { platformFromDiagnostics } from "./platform.ts";

/**
 * Resolve the platform the search-shortcut listener cares about.
 * Returns `"macos"` for the macOS family (macOS / Darwin / OSX) and
 * `"other"` for everything else.
 */
export type SearchShortcutPlatform = "macos" | "other";

export function searchShortcutPlatform(
  diagnosticsPlatformOs: string | null | undefined,
): SearchShortcutPlatform {
  return platformFromDiagnostics(diagnosticsPlatformOs) === "macos"
    ? "macos"
    : "other";
}

/** Visible label for the platform shortcut hint. */
export function searchShortcutLabel(
  platform: SearchShortcutPlatform,
): string {
  return platform === "macos" ? "⌘F" : "Ctrl F";
}

/** Long-form accessible label matching the visible hint. */
export function searchShortcutAccessibleLabel(
  platform: SearchShortcutPlatform,
): string {
  return platform === "macos"
    ? "Buscar (Comando F)"
    : "Buscar (Control F)";
}

/**
 * Whether a keyboard event matches the platform-specific search
 * shortcut. The check uses the `key` field (defaults to "F" or "f")
 * instead of `code` so a non-QWERTY layout still triggers the
 * shortcut through the webview.
 */
export function matchesSearchShortcut(
  event: KeyboardEvent | { key: string; metaKey?: boolean; ctrlKey?: boolean; altKey?: boolean; shiftKey?: boolean },
  platform: SearchShortcutPlatform,
): boolean {
  if (event.altKey || event.shiftKey) return false;
  const key = (event.key ?? "").toLowerCase();
  if (key !== "f") return false;
  return platform === "macos"
    ? Boolean(event.metaKey) && !event.ctrlKey
    : Boolean(event.ctrlKey) && !event.metaKey;
}
