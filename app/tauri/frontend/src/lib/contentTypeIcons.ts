// Local SVG icon map for the documented textual content types.
//
// The icons are intentionally minimal: each shape is a single
// monochrome glyph the card rail can render at 16-24 CSS pixels
// without anti-aliasing artefacts. The frontend never reaches out
// for an external resource, so the rail stays usable offline and the
// privacy guarantees documented in the proposal remain intact.
//
// The fallback (`text`) is the canonical glyph for the unknown-type
// branch — when the backend introduces a new `content_type`, the
// rail renders this generic glyph instead of an empty cell.
//
// All icons live behind a single `<symbol>` SVG sprite the card
// component references via `<use>` so the rail renders the whole
// grid in a single DOM subtree. The sprite is intentionally a
// static string (no reactive state) so the renderer keeps it across
// re-renders.

import type { ContentTypeValue } from "./contentType.ts";

export type IconColor =
  | "primary"
  | "muted"
  | "inverted";

interface IconSpriteEntry {
  /** Stable id the `<use>` element references. */
  id: string;
  /** Accessible label the renderer mirrors on the icon's `aria-label`. */
  label: string;
}

const SPRITE_ENTRIES: Record<ContentTypeValue | "fallback", IconSpriteEntry> = {
  text: { id: "cv-icon-text", label: "Texto" },
  url: { id: "cv-icon-url", label: "URL" },
  email: { id: "cv-icon-email", label: "Email" },
  json: { id: "cv-icon-json", label: "JSON" },
  jwt: { id: "cv-icon-jwt", label: "JWT" },
  uuid: { id: "cv-icon-uuid", label: "UUID" },
  ipv4: { id: "cv-icon-ipv4", label: "IPv4" },
  ipv6: { id: "cv-icon-ipv6", label: "IPv6" },
  hex_color: { id: "cv-icon-hex-color", label: "Color hexadecimal" },
  html: { id: "cv-icon-html", label: "HTML" },
  file_path: { id: "cv-icon-file-path", label: "Ruta de archivo" },
  shell_command: { id: "cv-icon-shell-command", label: "Comando de shell" },
  sql: { id: "cv-icon-sql", label: "SQL" },
  code: { id: "cv-icon-code", label: "Código" },
  image: { id: "cv-icon-image", label: "Imagen" },
  fallback: { id: "cv-icon-fallback", label: "Texto" },
};

/**
 * Lookup the sprite id for a content type. Unknown values fall back
 * to the generic text sprite so the rail never renders an empty
 * icon.
 */
export function contentTypeIconId(value: string | null | undefined): string {
  if (typeof value !== "string" || value.length === 0) {
    return SPRITE_ENTRIES.fallback.id;
  }
  const known = (SPRITE_ENTRIES as Record<string, IconSpriteEntry | undefined>)[value];
  return (known ?? SPRITE_ENTRIES.fallback).id;
}

/**
 * Accessible label the renderer mirrors on the icon. The label is
 * the same as `contentTypeLabel` so screen readers announce the
 * type consistently with the textual badge the rail also renders.
 */
export function contentTypeIconLabel(value: string | null | undefined): string {
  if (typeof value !== "string" || value.length === 0) {
    return SPRITE_ENTRIES.fallback.label;
  }
  const known = (SPRITE_ENTRIES as Record<string, IconSpriteEntry | undefined>)[value];
  return (known ?? SPRITE_ENTRIES.fallback).label;
}

/**
 * Static SVG sprite the card rail mounts once. The sprite uses
 * `currentColor` so a single CSS rule (`.card-icon { color: ... }`)
 * controls the colour without re-rendering the grid.
 */
export const CONTENT_TYPE_ICON_SPRITE: string = `
<svg xmlns="http://www.w3.org/2000/svg" width="0" height="0" style="position:absolute" aria-hidden="true" focusable="false">
  <defs>
    <symbol id="cv-icon-text" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <path d="M5 6h14M5 12h14M5 18h10" />
    </symbol>
    <symbol id="cv-icon-url" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <path d="M9 15a4 4 0 0 1 0-6l2-2a4 4 0 0 1 6 6l-1 1" />
      <path d="M15 9a4 4 0 0 1 0 6l-2 2a4 4 0 0 1-6-6l1-1" />
    </symbol>
    <symbol id="cv-icon-email" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <rect x="3" y="5" width="18" height="14" rx="2" />
      <path d="m3 7 9 7 9-7" />
    </symbol>
    <symbol id="cv-icon-json" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <path d="M8 4c-2 0-3 1-3 3v2c0 1-1 2-2 2 1 0 2 1 2 2v2c0 2 1 3 3 3" />
      <path d="M16 4c2 0 3 1 3 3v2c0 1 1 2 2 2-1 0-2 1-2 2v2c0 2-1 3-3 3" />
    </symbol>
    <symbol id="cv-icon-jwt" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="12" cy="12" r="8" />
      <path d="M8 12h8M12 8v8" />
    </symbol>
    <symbol id="cv-icon-uuid" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="7" cy="7" r="1.5" />
      <circle cx="17" cy="7" r="1.5" />
      <circle cx="7" cy="17" r="1.5" />
      <circle cx="17" cy="17" r="1.5" />
      <path d="M7 8.5v7M17 8.5v7M8.5 7h7M8.5 17h7" />
    </symbol>
    <symbol id="cv-icon-ipv4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="12" cy="12" r="9" />
      <path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18" />
    </symbol>
    <symbol id="cv-icon-ipv6" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="12" cy="12" r="9" />
      <path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18" />
      <circle cx="12" cy="12" r="2.5" fill="currentColor" stroke="none" />
    </symbol>
    <symbol id="cv-icon-hex-color" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <path d="M12 3 4 8v8l8 5 8-5V8z" />
      <path d="M9 12h6M12 9v6" />
    </symbol>
    <symbol id="cv-icon-html" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <path d="m6 5-3 7 3 7M18 5l3 7-3 7M14 5l-4 14" />
    </symbol>
    <symbol id="cv-icon-file-path" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <path d="M4 6h12l4 4v8a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2z" />
      <path d="M16 6v4h4" />
      <path d="M7 14h10" />
    </symbol>
    <symbol id="cv-icon-shell-command" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <path d="m5 8 4 4-4 4M13 8l-4 4 4 4M12 5l1 14" />
    </symbol>
    <symbol id="cv-icon-sql" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <ellipse cx="12" cy="6" rx="7" ry="3" />
      <path d="M5 6v6c0 1.7 3.1 3 7 3s7-1.3 7-3V6M5 12v6c0 1.7 3.1 3 7 3s7-1.3 7-3v-6" />
    </symbol>
    <symbol id="cv-icon-code" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <path d="m7 8-4 4 4 4M17 8l4 4-4 4M14 5l-4 14" />
    </symbol>
    <symbol id="cv-icon-image" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <circle cx="8.5" cy="9.5" r="1.5" />
      <path d="m4 17 5-5 4 4 3-3 4 4" />
    </symbol>
    <symbol id="cv-icon-fallback" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
      <path d="M5 6h14M5 12h14M5 18h10" />
    </symbol>
  </defs>
</svg>`;

/**
 * Static SVG markup used as the generic source-application
 * fallback when the metadata bridge cannot deliver bytes. Mirrors
 * the design contract: a deterministic, locally-controlled glyph
 * the rail can render without ever loading a remote resource.
 */
export const APP_FALLBACK_ICON_SVG: string = `
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  <rect x="4" y="4" width="16" height="16" rx="3" />
  <path d="M9 9h6v6H9z" />
</svg>`;
