/**
 * Shared helpers for the in-window preview overlay the Desktop and
 * Quick Paste surfaces render.
 *
 * The Quick Paste window has exposed a strictly read-only preview
 * overlay since the `quick-paste-preview-ui` change; the
 * `desktop-card-preview` change extends that exact same contract to
 * the desktop rail. To prevent the two surfaces from drifting apart
 * this module owns the helpers that used to live inline inside
 * `QuickPaste.svelte` so both consumers import one implementation:
 *
 *   - the platform-aware `Cmd/Ctrl+Enter` matcher
 *     (`matchesPreviewShortcut`) that the keyboard layer
 *     consults;
 *   - the visible/accessible shortcut label the two surfaces
 *     render next to the affordance (`previewShortcutLabel`,
 *     `previewShortcutAccessibleLabel`);
 *   - re-exports of the typed metadata helpers the existing tests
 *     already pin (`entryFullPreviewText`, `escapeForPreview`,
 *     `hasRenderableImage`, `isImageEntry`) so Desktop and Quick
 *     Paste both consume them from one path.
 *
 * The module intentionally contains **no Svelte component** and **no
 * DOM access** — it is a pure helper layer the shared
 * `ClipboardPreview.svelte` component imports. Keeping it pure means
 * the regression suite can exercise the keyboard matcher and the
 * text/escape pipeline without rendering any DOM.
 *
 * Privacy: the matcher reads only the platform string the backend
 * diagnostics emitted (already collected once at window open) and
 * the keyboard event modifiers; it never inspects clipboard
 * content, asset references, hashes, identifiers or paths.
 */

import { searchShortcutPlatform, type SearchShortcutPlatform } from "./searchShortcut.ts";

/**
 * Re-export of the `SearchShortcutPlatform` shape so Desktop and
 * Quick Paste consumers can read the platform from one place without
 * pulling the `searchShortcut` module themselves.
 */
export type PreviewShortcutPlatform = SearchShortcutPlatform;

/**
 * Re-export the typed helpers that already live in `clipboardAsset.ts`
 * so the Desktop preview, the Quick Paste preview and any future
 * surface import the exact same metadata predicates.
 */
export {
  entryFullPreviewText,
  escapeForPreview,
  isImageEntry,
  hasRenderableImage,
} from "./clipboardAsset.ts";

/**
 * Whether the keyboard event matches the platform-aware preview
 * shortcut the Desktop and Quick Paste windows install.
 *
 *   - macOS: `metaKey` + `Enter`, no `Shift` / `Alt`.
 *   - everything else (Linux / Windows future): `ctrlKey` + `Enter`,
 *     no `Shift` / `Alt`.
 *
 * The check intentionally ignores a plain `Enter` so the legacy
 * copy-only keyboard flow (Enter / Shift+Enter) is unaffected: the
 * matcher only fires when the user *also* holds the documented
 * platform modifier. The plain `Cmd` or `Ctrl` key alone is not a
 * trigger either.
 */
export function matchesPreviewShortcut(
  event: Pick<
    KeyboardEvent,
    "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey"
  >,
  platform: PreviewShortcutPlatform,
): boolean {
  if (event.altKey || event.shiftKey) return false;
  if (event.key !== "Enter") return false;
  return platform === "macos"
    ? Boolean(event.metaKey) && !event.ctrlKey
    : Boolean(event.ctrlKey) && !event.metaKey;
}

/**
 * Resolve the platform the preview matcher should use. Mirrors
 * `searchShortcutPlatform` so a future tweak to the diagnostics
 * payload only has to land in one helper.
 */
export function previewShortcutPlatform(
  diagnosticsPlatformOs: string | null | undefined,
): SearchShortcutPlatform {
  return searchShortcutPlatform(diagnosticsPlatformOs);
}

/** Visible label the affordance renders next to the trigger. */
export function previewShortcutLabel(
  platform: PreviewShortcutPlatform,
): string {
  return platform === "macos" ? "⌘Enter" : "Ctrl Enter";
}

/** Long-form accessible label matching the visible hint. */
export function previewShortcutAccessibleLabel(
  platform: PreviewShortcutPlatform,
): string {
  return platform === "macos"
    ? "Previsualizar (Comando Enter)"
    : "Previsualizar (Control Enter)";
}