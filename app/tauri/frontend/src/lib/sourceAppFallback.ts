// Helpers for the source-app metadata the history card renders.
//
// The `history-card-layout` spec pins the surface the card shows for
// the source application: the icon only — never the user-visible
// name, identifier, bundle identifier or any snippet. The helper in
// this module exists so the card rail can keep a stable accessible
// label for screen readers and tooltips without ever leaking the
// identifier or the application name as visible text.
//
// The card renders, in order:
//   1. An icon (through the safe icon bridge when the backend
//      persisted a `source_app_icon_ref`; a deterministic generic
//      fallback SVG otherwise).
//   2. No visible text: the name, identifier, bundle id and the
//      "unknown" sentinel all stay off the visual surface.
//
// The pure helper in this module keeps the rendering rules
// testable without spinning up a Svelte runtime — the Svelte
// component imports it and binds the value to the `aria-label`
// and `title` attributes only.

import type {
  EntryRecord,
  PeerImportedSourceAppPresentation,
} from "../types.ts";

/** Shared local-only fallback for an imported source app with no valid PNG. */
export const IMPORTED_SOURCE_APP_FALLBACK_ICON_SVG =
  '<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><path d="M12 3v11m0 0 4-4m-4 4-4-4M5 15v4a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-4" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>';

/**
 * Resolve the accessible label the card rail must announce for the
 * source-app badge. The label is exposed via `aria-label` and
 * `title` only — the visual surface never carries the application
 * name, the bundle identifier or any snippet.
 *
 * The contract:
 * - When the metadata provider resolved a user-visible display
 *   name, return `"Aplicación fuente: <name>"`.
 * - When only the privacy/matching identifier is populated, return
 *   `"Aplicación fuente: <id>"`. The identifier never reaches the
 *   visual surface, only the screen reader / tooltip.
 * - When neither column is populated, return
 *   `"Aplicación fuente desconocida"`.
 *
 * The label intentionally prefixes the value with the Spanish
 * stable string `Aplicación fuente:` so screen readers can announce
 * the badge as a coherent sentence without having to fall back on
 * the bare identifier. The card rail must NEVER surface the
 * legacy `—` placeholder; the screen-reader experience must match
 * the visual experience.
 */
type TranslateText = (key: string, params?: Record<string, string | number | Date>) => string;

export function sourceAppAccessibleLabel(
  entry: EntryRecord,
  translateText?: TranslateText,
): string {
  const name = entry.source_app_name?.trim();
  if (name) return translateText?.("source_app.label", { name }) ?? `Aplicación fuente: ${name}`;
  const id = entry.source_app?.trim();
  if (id) return translateText?.("source_app.label", { name: id }) ?? `Aplicación fuente: ${id}`;
  return translateText?.("source_app.unknown") ?? "Aplicación fuente desconocida";
}

/** Use imported provenance as a separate presentation, never as local entry metadata. */
export function sourceAppPresentationIconRef(
  entry: EntryRecord,
  imported: PeerImportedSourceAppPresentation | null,
): string | null {
  return imported !== null
    ? imported.source_app_icon_ref
    : entry.source_app_icon_ref ?? null;
}

/** Imported source names are tooltip/accessibility text only; peer IDs never enter the label. */
export function sourceAppPresentationAccessibleLabel(
  entry: EntryRecord,
  imported: PeerImportedSourceAppPresentation | null,
  translateText?: TranslateText,
): string {
  if (imported === null) return sourceAppAccessibleLabel(entry, translateText);
  const name = imported.source_app_name?.trim();
  if (!name) return translateText?.("source_app.unknown") ?? "Aplicación fuente: desconocida";
  return translateText?.("source_app.label", { name }) ?? `Aplicación fuente: ${name}`;
}
