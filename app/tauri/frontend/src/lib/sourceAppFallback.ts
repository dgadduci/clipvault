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

import type { EntryRecord } from "../types.ts";

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
export function sourceAppAccessibleLabel(entry: EntryRecord): string {
  const name = entry.source_app_name?.trim();
  if (name) return `Aplicación fuente: ${name}`;
  const id = entry.source_app?.trim();
  if (id) return `Aplicación fuente: ${id}`;
  return "Aplicación fuente desconocida";
}