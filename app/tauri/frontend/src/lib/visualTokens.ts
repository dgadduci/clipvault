// Visual system tokens for the reorganized desktop.
//
// The shell must keep the dark theme already shipped with the inline
// panel and the cards. Defining the tokens in one module keeps every
// component consistent and gives us a single place to evolve the
// scale (responsive breakpoints, accent colour swap, density toggle)
// without scattering the values across every `.svelte` file.
//
// The contract is intentionally local: no external CSS framework, no
// runtime theme switcher, no font CDN. Tokens are constants the
// components consume through plain CSS custom properties.

export interface VisualTokens {
  fontFamily: string;
  fontSizeBody: string;
  fontSizeMuted: string;
  fontSizeTitle: string;
  fontSizeTitleLg: string;
  fontSizeTitleSm: string;
  fontSizeControl: string;
  fontSizeTag: string;
  fontSizePreview: string;
  radiusSm: string;
  radiusMd: string;
  radiusLg: string;
  surfaceBg: string;
  elevatedBg: string;
  borderColor: string;
  borderStrong: string;
  fg: string;
  fgMuted: string;
  fgError: string;
  fgOk: string;
  accent: string;
  accentHover: string;
  danger: string;
  dangerHover: string;
  modalOverlay: string;
  focusRing: string;
  /**
   * Stable visible height of the card rail. The collection sidebar
   * reads this token so the panel and the rail stay visually aligned
   * without either of them redefining the constant.
   */
  cardRailHeight: string;
}

export const VISUAL_TOKENS: VisualTokens = {
  fontFamily:
    '-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif',
  fontSizeBody: "0.9rem",
  fontSizeMuted: "0.78rem",
  fontSizeTitle: "1.5rem",
  fontSizeTitleLg: "1.75rem",
  fontSizeTitleSm: "0.95rem",
  fontSizeControl: "0.85rem",
  fontSizeTag: "0.65rem",
  fontSizePreview: "0.72rem",
  radiusSm: "6px",
  radiusMd: "10px",
  radiusLg: "12px",
  surfaceBg: "#0e1116",
  elevatedBg: "#161b22",
  borderColor: "#30363d",
  borderStrong: "#475569",
  fg: "#f0f4f8",
  fgMuted: "#94a3b8",
  fgError: "#f87171",
  fgOk: "#4ade80",
  accent: "#2563eb",
  accentHover: "#1d4ed8",
  danger: "#b91c1c",
  dangerHover: "#991b1b",
  modalOverlay: "rgba(8, 11, 16, 0.78)",
  focusRing: "rgba(37, 99, 235, 0.45)",
  // The rail exposes the height of a single visible card row plus the
  // padding and the size of its controls. The value is intentionally
  // close to the documented card size (240px) so the sidebar matches
  // the rail exactly.
  cardRailHeight: "calc(var(--cv-card-size, 240px) + 2.75rem)",
};

/**
 * Render the CSS custom property declarations for the visual system.
 * `App.svelte` injects the block once at the document root so every
 * component can read the values through `var(--cv-*, fallback)` and
 * the spec-mandated typography stays consistent across modals.
 */
export function visualTokenCss(): string {
  const t = VISUAL_TOKENS;
  return [
    `--cv-font-family: ${t.fontFamily};`,
    `--cv-body: ${t.fontSizeBody};`,
    `--cv-muted: ${t.fontSizeMuted};`,
    `--cv-title: ${t.fontSizeTitle};`,
    `--cv-title-lg: ${t.fontSizeTitleLg};`,
    `--cv-title-md: 1rem;`,
    `--cv-title-sm: ${t.fontSizeTitleSm};`,
    `--cv-control: ${t.fontSizeControl};`,
    `--cv-tag: ${t.fontSizeTag};`,
    `--cv-preview: ${t.fontSizePreview};`,
    `--cv-radius-sm: ${t.radiusSm};`,
    `--cv-radius-md: ${t.radiusMd};`,
    `--cv-radius-lg: ${t.radiusLg};`,
    `--cv-bg-surface: ${t.surfaceBg};`,
    `--cv-bg-elevated: ${t.elevatedBg};`,
    `--cv-border: ${t.borderColor};`,
    `--cv-border-strong: ${t.borderStrong};`,
    `--cv-fg: ${t.fg};`,
    `--cv-fg-muted: ${t.fgMuted};`,
    `--cv-fg-error: ${t.fgError};`,
    `--cv-fg-ok: ${t.fgOk};`,
    `--cv-accent: ${t.accent};`,
    `--cv-accent-hover: ${t.accentHover};`,
    `--cv-danger: ${t.danger};`,
    `--cv-danger-hover: ${t.dangerHover};`,
    `--cv-modal-overlay: ${t.modalOverlay};`,
    `--cv-focus-ring: ${t.focusRing};`,
    `--cv-card-rail-height: ${t.cardRailHeight};`,
  ].join(" ");
}