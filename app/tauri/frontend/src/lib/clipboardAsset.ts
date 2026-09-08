/**
 * Frontend bridge for persisted clipboard payload assets.
 *
 * The module deliberately owns **no** resolver logic of its own: it
 * reuses `createIconResolver` from `lib/iconResolver.ts`, which already
 * implements the "bytes -> Blob -> blob: URL -> revoke" lifecycle and
 * its cache. Only two things are new here:
 *
 * 1. a loader that routes through `clipboardAssetCommand` instead of the
 *    application-icon commands;
 * 2. small metadata-only predicates so a component can decide whether an
 *    entry is an image or a rich-text row without ever inspecting
 *    `content`.
 *
 * ## Why the predicates matter
 *
 * The backend keeps `content` `NOT NULL` and stores an empty sentinel
 * for an image row. Rendering that sentinel as text would be a
 * user-visible bug, so every UI surface must branch on
 * `content_type === "image"` and, before requesting bytes, on
 * `hasRenderableImage`. An `image` row whose metadata is incomplete is
 * treated as broken and shows the accessible fallback — never the
 * sentinel, never the reference, never the bytes.
 *
 * The same rule applies to rich-text rows: `content` is the canonical
 * plain text and the card MUST NOT use it as the preview when a
 * sanitised rich preview is available. The plain text stays as the
 * accessible fallback the card renders when the preview bridge fails.
 *
 * ## Privacy
 *
 * `asset_ref` and `rich_*_ref` are opaque, relative, locally-controlled
 * references. They are only ever forwarded back to the backend command;
 * they are never rendered, never logged and never placed in a DOM
 * attribute. The original HTML and RTF bytes the source application
 * published never cross the Tauri boundary through this bridge — only
 * the sanitised preview the core produced does.
 */

import {
  createIconResolver,
  type IconLoader,
  type IconResolver,
} from "./iconResolver.ts";
import { clipboardAssetCommand, richTextPreviewCommand } from "./tauri.ts";
import type { EntryRecord } from "../types.ts";

/** Stable `content_type` value the backend uses for raster images. */
export const IMAGE_CONTENT_TYPE = "image";

/** Namespace prefix every valid clipboard asset reference carries. */
export const CLIPBOARD_ASSET_PREFIX = "clipboard/";

/** Namespace prefix every valid rich-text asset reference carries. */
export const RICH_TEXT_ASSET_PREFIX = "rich-text/";

/** Extension every sanitised rich-text preview carries. */
export const RICH_TEXT_PREVIEW_SUFFIX = ".preview.html";

/** MIME type the rich preview blob carries so the sandboxed iframe
 * interprets the sanitised bytes as HTML. */
export const RICH_TEXT_PREVIEW_MIME = "text/html;charset=utf-8";

/**
 * Loader for clipboard payload assets. Mirrors the icon loaders: a
 * backend rejection collapses to `null` so the resolver produces the
 * fallback instead of surfacing the underlying error to the card.
 */
export const tauriClipboardAssetLoader: IconLoader = {
  async loadIconBytes(ref: string): Promise<number[] | Uint8Array | null> {
    try {
      return await clipboardAssetCommand({ ref });
    } catch {
      return null;
    }
  },
};

/**
 * Loader for sanitised rich-text previews. Same shape and contract as
 * `tauriClipboardAssetLoader`; the response is decoded as UTF-8 by the
 * resolver because the preview is HTML text, not a binary blob.
 */
export const tauriRichTextPreviewLoader: IconLoader = {
  async loadIconBytes(ref: string): Promise<number[] | Uint8Array | null> {
    try {
      return await richTextPreviewCommand({ ref });
    } catch {
      return null;
    }
  },
};

/**
 * Build a resolver for clipboard payload assets.
 *
 * The returned resolver is the same implementation the settings panel
 * uses for application icons — same cache semantics, same `release` /
 * `releaseFor` contract — wired to the clipboard asset command. Callers
 * MUST call `releaseFor(ref)` (or `release()`) when the owning card is
 * destroyed so the `blob:` URL does not leak until the window reloads.
 */
export function createClipboardAssetResolver(
  loader: IconLoader = tauriClipboardAssetLoader,
): IconResolver {
  return createIconResolver(loader);
}

/**
 * Build a resolver for sanitised rich-text previews. The returned
 * resolver reuses the icon-resolver implementation so the card treats
 * the preview bytes exactly like any other cached asset: it mints a
 * `blob:` URL when the bytes load and revokes it through `release` /
 * `releaseFor`.
 *
 * The MIME type is `text/html;charset=utf-8` so the sandboxed iframe
 * the card renders interprets the bytes as HTML. Without the right
 * MIME the browser would treat the blob as a binary download and the
 * preview would be blank.
 */
export function createRichTextPreviewResolver(
  loader: IconLoader = tauriRichTextPreviewLoader,
): IconResolver {
  return createIconResolver(loader, { mimeType: RICH_TEXT_PREVIEW_MIME });
}

/**
 * Whether the entry is an image row. Metadata-only: never inspects
 * `content`.
 */
export function isImageEntry(
  entry: Pick<EntryRecord, "content_type">,
): boolean {
  return entry.content_type === IMAGE_CONTENT_TYPE;
}

/**
 * Whether the entry is a *coherent* image row whose bytes are worth
 * requesting.
 *
 * Mirrors `EntryRecord::is_renderable_image` on the Rust side: the row
 * must be an image, carry a non-empty reference inside the clipboard
 * namespace, declare a MIME type and report positive dimensions.
 * Anything else renders the accessible fallback without a round-trip.
 */
export function hasRenderableImage(
  entry: Pick<
    EntryRecord,
    "content_type" | "asset_ref" | "mime_type" | "payload_width" | "payload_height"
  >,
): boolean {
  if (!isImageEntry(entry)) return false;
  const ref = entry.asset_ref;
  if (typeof ref !== "string" || ref.length === 0) return false;
  if (!ref.startsWith(CLIPBOARD_ASSET_PREFIX)) return false;
  if (typeof entry.mime_type !== "string" || entry.mime_type.length === 0) {
    return false;
  }
  const { payload_width: width, payload_height: height } = entry;
  if (typeof width !== "number" || width <= 0) return false;
  if (typeof height !== "number" || height <= 0) return false;
  return true;
}

/**
 * Whether the entry is a *coherent* rich-text row the card can render
 * as a sanitised preview.
 *
 * Mirrors `EntryRecord::has_rich_text` on the Rust side: the row must
 * expose at least one of the original rich references (HTML or RTF)
 * and carry the sanitised preview reference the card bridge serves.
 * The preview reference is the only one the card ever consumes — the
 * original HTML/RTF refs are kept for the paste path and stay inside
 * Rust.
 */
export function hasRenderableRichText(
  entry: Pick<
    EntryRecord,
    "rich_text_hash" | "rich_html_ref" | "rich_rtf_ref" | "rich_preview_ref"
  >,
): boolean {
  if (typeof entry.rich_text_hash !== "string" || entry.rich_text_hash.length === 0) {
    return false;
  }
  const html = entry.rich_html_ref;
  const rtf = entry.rich_rtf_ref;
  const hasHtml = typeof html === "string" && html.length > 0;
  const hasRtf = typeof rtf === "string" && rtf.length > 0;
  if (!hasHtml && !hasRtf) return false;
  const preview = entry.rich_preview_ref;
  if (typeof preview !== "string" || preview.length === 0) return false;
  if (!preview.startsWith(RICH_TEXT_ASSET_PREFIX)) return false;
  if (!preview.endsWith(RICH_TEXT_PREVIEW_SUFFIX)) return false;
  return true;
}

/**
 * Accessible description of an image entry's intrinsic size, or `null`
 * when the dimensions are unknown. Metadata only — no bytes, no path.
 */
export function imageDimensionsLabel(
  entry: Pick<EntryRecord, "payload_width" | "payload_height">,
): string | null {
  const { payload_width: width, payload_height: height } = entry;
  if (typeof width !== "number" || typeof height !== "number") return null;
  if (width <= 0 || height <= 0) return null;
  return `${width}×${height}`;
}

/**
 * One paste action the ellipsis menu exposes for a card.
 *
 * The shape is intentionally self-describing: the caller renders each
 * item from its own fields (label, testid, aria-label, tooltip,
 * disabled, mode). A single `mode` covers the three documented shapes:
 *
 * - `null` (image row, the legacy quick-paste default that the Rust
 *   side maps to [`PasteMode::Plain`]);
 * - `"plain"` (textual paste, only useful for a textual entry);
 * - `"rich"` (HTML / RTF paste, only useful for a rich-text entry).
 *
 * The helper that builds these (`pasteMenuActionsFor`) is the only
 * source of truth for which actions the menu renders — every other
 * surface (unit tests, future rail layout) must go through it so the
 * image-card regression documented in the
 * `clipboard-history-cards` spec cannot drift back in.
 */
export interface PasteMenuAction {
  kind: "image-paste" | "text-rich-paste" | "text-plain-paste";
  /** Visible label rendered inside the menu button. */
  label: string;
  /** Stable test selector the regression suite pins. */
  testId: string;
  /**
   * Mode forwarded to `pasteEntryCommand`. `null` for an image so the
   * Rust side runs the legacy quick-paste path that ignores the mode
   * for image rows and writes the bitmap through `write_image`.
   */
  mode: "plain" | "rich" | null;
  /** Accessible label announced by screen readers. */
  ariaLabel: string;
  /** Hover tooltip explaining what the action does. */
  tooltip: string;
  /** `disabled` flag the menu button binds directly. */
  disabled: boolean;
}

/**
 * Inputs the menu helper needs to compute the disabled / tooltip
 * flags. The component is the only caller; the helper has no other
 * state to inspect.
 */
export interface PasteMenuOptions {
  /** Whether the current entry carries rich metadata. */
  richPasteEnabled: boolean;
  /** Whether a paste round-trip is already in flight. */
  pasteBusy: boolean;
}

/** Accessible label fragments the menu helper reuses verbatim. */
const PASTE_RICH_LABEL = "Pegar texto enriquecido";
const PASTE_PLAIN_LABEL = "Pegar texto plano";
const PASTE_IMAGE_LABEL = "Pegar imagen";

/**
 * Build the list of paste actions the card's ellipsis menu exposes for
 * the given entry.
 *
 * The contract is documented in the
 * `clipboard-history-cards` spec:
 *
 * - an image row returns **exactly one** action whose label is
 *   `Paste` and whose `mode` is `null` so the Rust paste service
 *   takes the image branch regardless of the legacy textual mode;
 * - a rich-text row returns the two textual actions, with `Paste de
 *   texto enriquecido` enabled when the entry carries rich metadata;
 * - a plain-text row returns the two textual actions, with the rich
 *   one visibly disabled because the source never exposed HTML/RTF.
 *
 * The helper is pure: it never inspects clipboard content, asset
 * references or content hashes, so it can be exercised by the
 * regression suite without leaking anything.
 */
export function pasteMenuActionsFor(
  entry: Pick<EntryRecord, "content_type">,
  title: string,
  options: PasteMenuOptions,
): PasteMenuAction[] {
  if (isImageEntry(entry)) {
    return [
      {
        kind: "image-paste",
        label: "Paste",
        testId: "history-card-paste",
        mode: null,
        ariaLabel: `${PASTE_IMAGE_LABEL} de ${title}`,
        tooltip: "Pegar la imagen capturada.",
        disabled: false,
      },
    ];
  }
  const richTooltip = options.richPasteEnabled
    ? "Pegar la entrada conservando el formato."
    : "Esta entrada no tiene texto enriquecido.";
  return [
    {
      kind: "text-rich-paste",
      label: "Paste de texto enriquecido",
      testId: "history-card-paste-rich",
      mode: "rich",
      ariaLabel: `${PASTE_RICH_LABEL} de ${title}`,
      tooltip: richTooltip,
      disabled: !options.richPasteEnabled || options.pasteBusy,
    },
    {
      kind: "text-plain-paste",
      label: "Paste de texto plano",
      testId: "history-card-paste-plain",
      mode: "plain",
      ariaLabel: `${PASTE_PLAIN_LABEL} de ${title}`,
      tooltip: "Pegar únicamente el texto plano.",
      disabled: options.pasteBusy,
    },
  ];
}

/**
 * Preview text a list row should show for an entry.
 *
 * For a textual entry this is the collapsed, truncated content, exactly
 * as before. For an image entry it is a localised placeholder plus the
 * dimensions when known — the empty `content` sentinel is never
 * returned, so no surface can accidentally render it. The function
 * intentionally returns the plain-text content for rich-text rows: the
 * card uses it as the accessible fallback when the rich preview bridge
 * fails, and as the placeholder when the rich preview is still loading.
 */
export function entryPreviewText(
  entry: Pick<
    EntryRecord,
    | "content"
    | "content_type"
    | "asset_ref"
    | "mime_type"
    | "payload_width"
    | "payload_height"
  >,
  maxLength = 120,
): string {
  if (isImageEntry(entry)) {
    const dimensions = imageDimensionsLabel(entry);
    return dimensions ? `Imagen ${dimensions}` : "Imagen";
  }
  const trimmed = (entry.content ?? "").replace(/\s+/g, " ").trim();
  if (trimmed.length === 0) return "(vacío)";
  return trimmed.length > maxLength
    ? `${trimmed.slice(0, maxLength - 3)}…`
    : trimmed;
}

/**
 * Full canonical text of an entry, never truncated and with the
 * whitespace the source application produced preserved verbatim.
 *
 * The Quick Paste preview overlay and the Desktop preview overlay
 * are documented to render the complete capture (not the truncated
 * row preview) so sharing the `maxLength` helper would silently
 * regress the overlay to the same fragment the row renders. The
 * helper also keeps the original whitespace — `LF` / `CRLF` line
 * separators, consecutive tab characters, leading indentation and
 * consecutive empty lines — so the `<pre>` the overlay mounts
 * (with `white-space: pre-wrap`) renders the captured block
 * byte-for-byte. The renderer pairs this string with
 * `escapeForPreview` so the preview never re-introduces HTML
 * active content.
 *
 * The whitespace contract is intentionally identical to
 * `entryRawContent` so the two surfaces — Desktop preview and
 * Quick Paste preview — share one implementation. The helpers
 * stay distinct because `entryRawContent` is the typed accessor
 * the highlight.js renderer reads (and therefore must not be
 * collapsed by the helper itself), while `entryFullPreviewText`
 * is the named entry-point the shared `<ClipboardPreview>` overlay
 * consults for both the highlighted and the plain-text fallback
 * branches.
 *
 * For an image row the helper returns the empty `content` sentinel
 * for backwards compatibility; callers rendering an image preview
 * should branch on `isImageEntry` instead of inspecting the text.
 */
export function entryFullPreviewText(
  entry: Pick<EntryRecord, "content" | "content_type" | "asset_ref">,
): string {
  if (isImageEntry(entry)) return "";
  return entry.content ?? "";
}

/**
 * Raw, unmodified textual content of an entry.
 *
 * The helper is the source of truth the shared `ClipboardPreview`
 * overlay feeds to `renderHighlightedCode` so the highlighted
 * preview keeps the original tabs, newlines, indentation and
 * empty lines the source application produced. The helper never
 * collapses whitespace runs and never trims the string: a Python
 * block with four spaces of indentation, a JavaScript snippet
 * with literal `\t` characters or a multi-line block with empty
 * lines must reach the highlighter unchanged so the same
 * characters can reach the `<pre>` element the helper renders
 * inside.
 *
 * The whitespace contract is intentionally aligned with
 * `entryFullPreviewText`: both helpers return the unmodified
 * payload so Desktop and Quick Paste render the same block. The
 * helpers stay distinct because `entryRawContent` is the typed
 * accessor the highlight.js renderer reads (and therefore must not
 * be collapsed by the helper itself), while `entryFullPreviewText`
 * is the named entry-point the shared `<ClipboardPreview>` overlay
 * consults for both the highlighted and the plain-text fallback
 * branches.
 *
 * The helper is metadata-only by construction: it reads the same
 * `content` field the rest of the preview pipeline consumes and
 * never inspects asset references, hashes, paths or absolute
 * filenames. For an image row the helper returns the empty
 * `content` sentinel for backwards compatibility; callers that
 * build a code preview must branch on `isImageEntry` first so the
 * sentinel never leaks into the highlighted output.
 */
export function entryRawContent(
  entry: Pick<EntryRecord, "content" | "content_type">,
): string {
  if (isImageEntry(entry)) return "";
  return entry.content ?? "";
}

/**
 * Escape a plain-text capture into a safe representation the
 * preview overlay can render inside a `<pre>` element without
 * re-introducing the active content the spec explicitly forbids.
 *
 * The overlay MUST NEVER execute scripts, fire event handlers or
 * navigate through HTML anchors; escaping every character —
 * including `&`, `<`, `>` and the quote marks — closes the entire
 * HTML / script injection surface and keeps the preview safe even
 * for hostile captures. The exact mapped characters and their
 * replacements are stable so existing tests can assert the
 * mapping byte-for-byte.
 */
export function escapeForPreview(input: string): string {
  return input
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}
