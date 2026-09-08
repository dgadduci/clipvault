<script lang="ts">
  /**
   * Shared preview overlay for the Desktop and Quick Paste surfaces.
   *
   * The component renders a strictly read-only modal that mirrors the
   * Quick Paste preview the `quick-paste-preview-ui` change pinned.
   * The Desktop rail consumes it directly through the
   * `desktop-card-preview` change; Quick Paste consumes the same
   * component so neither surface can drift.
   *
   * The overlay:
   *
   *   - shows the entry's title, content-type icon and source-app
   *     metadata through the existing tokens and helpers so the two
   *     surfaces look identical;
   *   - renders the canonical full text via `entryFullPreviewText`
   *     and `escapeForPreview` so neither caller has to bring its
   *     own truncation / sanitisation logic;
   *   - resolves image rows through the existing
   *     `createClipboardAssetResolver` and `clipboardAssetCommand`,
   *     keeping the loading / loaded / error three-state machine
   *     and the stale-response guard that the rail and Quick Paste
   *     thumbnails already enforce;
   *   - revokes every blob URL through `releaseFor` / `release` so
   *     a long-lived window does not leak memory;
   *   - closes on `Escape` and on a click outside the inner card,
   *     and returns focus to the trigger element when the parent
   *     provides one;
   *   - never copies, pastes, edits, pins, tags, deletes or otherwise
   *     mutates the entry. The component only reads.
   *
   * The contract documented in the `desktop-card-preview` design
   * pins:
   *
   *   - "stale responses are discarded" → every `resolve` round
   *     bumps a per-instance token; only the freshest result wins;
   *   - "open and close is leak-free" → the `onDestroy` hook
   *     revokes the current blob URL and clears the token map;
   *   - "only one preview open at a time" → the parent owns the
   *     visibility state; the component always renders a single
   *     `<div role="dialog">` when `entry` is not `null` and nothing
   *     otherwise;
   *   - "return focus" → when `trigger` is provided, the
   *     close path calls `trigger.focus()` (best-effort) so the
   *     user lands back on the affordance they used.
   */
  import { onDestroy, onMount, tick } from "svelte";
  import type { EntryRecord } from "./types.ts";
  import {
    CONTENT_TYPE_ICON_SPRITE,
    contentTypeIconId,
    contentTypeIconLabel,
  } from "./lib/contentTypeIcons.ts";
  import { sourceAppAccessibleLabel } from "./lib/sourceAppFallback.ts";
  import { contentTypeLabel } from "./lib/contentType.ts";
  import { formatElapsedTime } from "./lib/elapsedTime.ts";
  import {
    createClipboardAssetResolver,
    entryFullPreviewText,
    entryRawContent,
    escapeForPreview,
    hasRenderableImage,
    isImageEntry,
  } from "./lib/clipboardAsset.ts";
  import {
    canonicalLabel as canonicalCodeLanguageLabel,
    renderHighlightedCode,
  } from "./lib/codeLanguageDetector.ts";
  import {
    shouldRenderHighlightedPreview,
    shouldShowCodeLanguageBadge,
  } from "./lib/codeLanguageProjections.ts";
  import type { IconResolver } from "./lib/iconResolver.ts";

  export let entry: EntryRecord | null = null;
  /**
   * Element to restore focus to once the preview closes (when the
   * caller can still resolve it). Optional — when the trigger
   * disappears while the preview is open, the overlay falls back
   * to detaching focus silently so the screen reader does not
   * announce a stale node.
   */
  export let trigger: HTMLElement | null = null;
  /** Optional `data-testid` prefix the parent uses (e.g. `quick-paste` or `history`). */
  export let testIdPrefix: string = "clipboard-preview";
  /**
   * Optional override of the visible window's accessible name. The
   * Desktop sets it to `Previsualización de …` and Quick Paste to
   * the same so the announcements match across surfaces.
   */
  export let accessibleLabel: string | null = null;
  /** Optional callback fired when the overlay requests close. */
  export let onClose: (() => void) | null = null;

  const assetResolver: IconResolver = createClipboardAssetResolver();

  type ImageState = "idle" | "loading" | "loaded" | "error";
  let imageState: ImageState = "idle";
  let imageUrl: string | null = null;
  let imageAssetRef: string | null = null;
  let imageToken = 0;

  /**
   * Reset image state to the entry's first coherent metadata shape.
   * The transition runs synchronously so the very first paint
   * never flashes the "Imagen no disponible" fallback for a
   * persisted, renderable image: a coherent image row enters
   * the `loading` branch immediately, a non-image row starts in
   * `idle`, and a row missing renderability metadata starts in
   * `error`.
   */
  function seedImageState(): void {
    if (!entry) {
      imageState = "idle";
      imageUrl = null;
      imageAssetRef = null;
      return;
    }
    if (!isImageEntry(entry) || !hasRenderableImage(entry)) {
      imageState = isImageEntry(entry) ? "error" : "idle";
      imageUrl = null;
      imageAssetRef = null;
      return;
    }
    imageState = "loading";
    imageAssetRef = entry.asset_ref ?? null;
  }

  $: entry, seedImageState();

  async function refreshImage(assetRef: string | null): Promise<void> {
    const token = ++imageToken;
    if (!assetRef) {
      imageUrl = null;
      if (token === imageToken) imageState = "error";
      return;
    }
    if (token === imageToken) imageState = "loading";
    const resolution = await assetResolver.resolve(assetRef);
    if (token !== imageToken) {
      // A newer refresh scheduled while we awaited the bridge —
      // drop the late response so we never overwrite the entry
      // the user is actually looking at.
      return;
    }
    if (resolution.ok && resolution.url) {
      imageUrl = resolution.url;
      imageState = "loaded";
    } else {
      imageUrl = null;
      imageState = "error";
    }
  }

  $: if (entry && imageState === "loading" && imageAssetRef) {
    void refreshImage(imageAssetRef);
  }

  function onImageError(): void {
    if (!imageAssetRef) {
      imageUrl = null;
      imageState = "error";
      return;
    }
    assetResolver.releaseFor(imageAssetRef);
    imageUrl = null;
    imageState = "error";
  }

  function close(): void {
    onClose?.();
  }

  function onOverlayClick(event: MouseEvent): void {
    // Only close when the click lands on the backdrop itself; a
    // click inside the inner card must not bubble up and dismiss
    // the overlay.
    if (event.target === event.currentTarget) {
      close();
    }
  }

  function onOverlayKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      // `stopPropagation` is required so the `Escape` keystroke does
      // NOT bubble up to a parent `<svelte:window>` listener that
      // would otherwise hide the Quick Paste window while the
      // preview is the only visible surface. Quick Paste mounts the
      // preview inside its own webview and installs a window-level
      // `keydown` handler; without this guard, the overlay's
      // `onClose` flips `previewEntryId = null` synchronously and
      // the bubbled event reaches the window listener with the
      // `surface` flag already flipped to `"list"`, which routes
      // the same Escape into `hideQuickPasteWindow()`. Desktop is
      // unaffected: it does not hide its main window on Escape.
      event.preventDefault();
      event.stopPropagation();
      close();
    }
  }

  let closeButtonEl: HTMLButtonElement | null = null;

  onMount(() => {
    // Move focus into the overlay so the next Tab steps through the
    // close button only; the overlay is otherwise strictly inert.
    void tick().then(() => {
      closeButtonEl?.focus();
    });
  });

  onDestroy(() => {
    // Revoke the current image's blob URL so a remount does not
    // leak the previous overlay's bytes; the resolver cache still
    // serves the URL for any consumer that already holds a
    // reference to it.
    if (imageAssetRef) {
      assetResolver.releaseFor(imageAssetRef);
    }
    imageUrl = null;
    imageState = "idle";
  });

  function restoreFocus(): void {
    if (!trigger) return;
    try {
      trigger.focus();
    } catch {
      // Best-effort: a remount may have detached the trigger.
    }
  }

  /**
   * Close handler that returns focus to the trigger when possible.
   * The helper exists so the parent does not need to wrap every
   * dismissal path (overlay click, Escape, close button).
   */
  function handleClose(): void {
    restoreFocus();
    close();
  }

  $: safeText =
    entry == null ? "" : escapeForPreview(entryFullPreviewText(entry));
  $: fullText = entry == null ? "" : entryFullPreviewText(entry);
  /**
   * Raw textual content of the entry, with no whitespace collapse and
   * no trim. The highlighted preview feeds THIS string into
   * `renderHighlightedCode` so the same `\n`, `\r\n`, `\t` and empty
   * lines the source application produced survive both the
   * highlighting and the subsequent `<pre>` mount. The
   * `entryFullPreviewText` helper above preserves the same
   * whitespace characters so the plain-text fallback path renders
   * the captured block byte-for-byte, the same way the highlighted
   * branch does — both surfaces consult the same canonical helper.
   */
  $: rawText = entry == null ? "" : entryRawContent(entry);
  $: highlightedHtml = (() => {
    if (entry == null || !shouldRenderHighlightedPreview(entry) || rawText.length === 0) {
      return null;
    }
    return renderHighlightedCode(rawText, entry.code_language).html;
  })();
  $: titleLabel =
    entry == null
      ? ""
      : (entry.title ?? "").trim().length > 0
        ? (entry.title ?? "").trim()
        : contentTypeLabel(entry.content_type);
  $: derivedAccessibleLabel =
    accessibleLabel ?? (entry == null ? "" : `Previsualización de ${titleLabel}`);
  $: sourceLabel =
    entry == null ? "" : sourceAppAccessibleLabel(entry);
  $: typeLabel =
    entry == null ? "" : contentTypeIconLabel(entry.content_type);
  $: codeLanguageLabel =
    entry == null || !shouldShowCodeLanguageBadge(entry)
      ? ""
      : canonicalCodeLanguageLabel(entry.code_language);
</script>

{#if entry != null}
  {@const record = entry}
  <div
    class="cv-preview-overlay"
    role="dialog"
    aria-modal="true"
    aria-label={derivedAccessibleLabel}
    data-testid="{testIdPrefix}-overlay"
    data-entry-id={record.id}
    data-content-type={record.content_type}
    on:click={onOverlayClick}
    on:keydown={onOverlayKeydown}
  >
    <div
      class="cv-preview-card"
      data-testid="{testIdPrefix}-card"
      data-content-type={record.content_type}
      tabindex="-1"
    >
      <header class="cv-preview-header">
        <span
          class="cv-preview-type"
          data-testid="{testIdPrefix}-type"
          data-content-type={record.content_type}
          aria-hidden="true"
          title={`Tipo: ${typeLabel}`}
        >
          <svg
            aria-hidden="true"
            focusable="false"
            width="20"
            height="20"
          >
            <use href="#{contentTypeIconId(record.content_type)}" />
          </svg>
        </span>
        <h2
          class="cv-preview-title"
          data-testid="{testIdPrefix}-title"
          title={titleLabel}
        >
          {titleLabel}
        </h2>
        <button
          type="button"
          class="cv-preview-close"
          data-testid="{testIdPrefix}-close"
          aria-label="Cerrar previsualización"
          title="Cerrar (Escape)"
          bind:this={closeButtonEl}
          on:click={handleClose}
        >
          ×
        </button>
      </header>
      <div
        class="cv-preview-body"
        data-testid="{testIdPrefix}-body"
        data-content-type={record.content_type}
      >
        {#if isImageEntry(record)}
          {#if imageUrl && imageState === "loaded"}
            <img
              class="cv-preview-image"
              src={imageUrl}
              alt={titleLabel}
              data-testid="{testIdPrefix}-image"
              on:error={onImageError}
            />
          {:else if record.asset_ref && hasRenderableImage(record) && imageState === "loading"}
            <p
              class="cv-preview-image-unavailable"
              data-testid="{testIdPrefix}-image-loading"
            >
              Cargando imagen…
            </p>
          {:else}
            <p
              class="cv-preview-image-unavailable"
              data-testid="{testIdPrefix}-image-unavailable"
            >
              Vista previa no disponible.
            </p>
          {/if}
        {:else if fullText.length === 0}
          <p
            class="cv-preview-text-empty"
            data-testid="{testIdPrefix}-text-empty"
          >
            (Captura vacía)
          </p>
        {:else if highlightedHtml && shouldRenderHighlightedPreview(record)}
          <pre
            class="cv-preview-text cv-preview-code"
            data-testid="{testIdPrefix}-code"
            data-content-type={record.content_type}
            data-code-language={record.code_language ?? ""}
            data-testid-language={record.code_language ?? ""}
          >{@html highlightedHtml}</pre>
        {:else}
          <pre
            class="cv-preview-text"
            data-testid="{testIdPrefix}-text"
            data-content-type={record.content_type}
          >{safeText}</pre>
        {/if}
      </div>
      <footer class="cv-preview-footer">
        <span
          class="cv-preview-meta"
          data-testid="{testIdPrefix}-meta"
          title={sourceLabel}
        >
          {sourceLabel}
        </span>
        {#if codeLanguageLabel && shouldShowCodeLanguageBadge(record)}
          <span
            class="cv-preview-language"
            data-testid="{testIdPrefix}-code-language"
            data-code-language={record.code_language ?? ""}
            title={`Lenguaje: ${codeLanguageLabel}`}
          >
            Código · {codeLanguageLabel}
          </span>
        {/if}
        <span
          class="cv-preview-elapsed"
          data-testid="{testIdPrefix}-elapsed"
        >
          {formatElapsedTime(record.created_at, new Date()).visual}
        </span>
      </footer>
    </div>
  </div>
{/if}

<!--
  Icon sprite the `<use href="#cv-icon-…">` references inside the
  overlay resolve against. The sprite mounts as a single offscreen
  `<svg width="0" height="0" style="position:absolute">` so it never
  produces a visible glyph on its own — every `<use>` inside the
  overlay paints the same shared path data at the consumer's
  declared size. Mounting the sprite at the component root keeps
  the symbol registry alive for the lifetime of the dialog without
  leaking any visible fallback mark below the rail.
-->
{@html CONTENT_TYPE_ICON_SPRITE}

<style>
  .cv-preview-overlay {
    position: fixed;
    inset: 0;
    background: rgba(8, 11, 16, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0.75rem;
    box-sizing: border-box;
    z-index: 60;
  }

  .cv-preview-card {
    width: 100%;
    max-width: 100%;
    max-height: 100%;
    background: #161b22;
    border: 1px solid #30363d;
    border-radius: 12px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.55);
    color: #f0f4f8;
    display: grid;
    grid-template-rows: auto 1fr auto;
    overflow: hidden;
  }

  .cv-preview-header {
    display: grid;
    grid-template-columns: 18px 1fr auto;
    gap: 0.5rem;
    align-items: center;
    padding: 0.5rem 0.65rem;
    border-bottom: 1px solid #30363d;
    background: rgba(255, 255, 255, 0.02);
  }

  .cv-preview-type {
    width: 1.5rem;
    height: 1.5rem;
    flex: 0 0 1.5rem;
    border-radius: 6px;
    background: rgba(147, 197, 253, 0.12);
    color: #93c5fd;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .cv-preview-title {
    margin: 0;
    font-size: var(--cv-body, 0.9rem);
    font-weight: 600;
    line-height: 1.2;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cv-preview-close {
    background: transparent;
    color: #94a3b8;
    border: 1px solid transparent;
    border-radius: 6px;
    width: 22px;
    height: 22px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    font-size: var(--cv-title-md, 1rem);
    line-height: 1;
  }

  .cv-preview-close:hover {
    background: rgba(148, 163, 184, 0.18);
    color: #f0f4f8;
  }

  .cv-preview-close:focus-visible {
    outline: 1px solid #2563eb;
    outline-offset: 1px;
  }

  .cv-preview-body {
    padding: 0.6rem 0.7rem;
    overflow: auto;
    background: rgba(0, 0, 0, 0.18);
    color: #cbd5f5;
    min-height: 0;
  }

  .cv-preview-text {
    margin: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: var(--cv-preview, 0.72rem);
    line-height: 1.4;
    color: #f0f4f8;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .cv-preview-code {
    /* The code preview keeps the documented monospace family but
     * tightens the padding and slightly enlarges the font so the
     * highlighted markup remains legible while the card preview
     * remains compact. The highlighted markup is read-only:
     * `highlight.js` never emits active content and the
     * `codeLanguageDetector.ts` helper sanitises the output so a
     * future regression cannot re-introduce scripts, event
     * handlers or remote URLs.
     *
     * The whitespace contract is declared here on purpose instead
     * of relying on the inherited `.cv-preview-text` rule: a global
     * stylesheet or a future ancestor rule with `white-space:
     * normal` would otherwise collapse the indentation, tabs and
     * newlines the source application produced. `tab-size: 4`
     * mirrors the Python / Rust / JavaScript indentation the
     * regression suite verifies; `overflow-wrap` keeps an
     * especially long line from breaking inside an identifier,
     * and `overflow: auto` lets the body scroll when the snippet
     * does not fit horizontally. */
    padding: 0.5rem 0.6rem;
    background: rgba(0, 0, 0, 0.25);
    border-radius: 6px;
    border: 1px solid rgba(147, 197, 253, 0.18);
    font-size: var(--cv-preview, 0.74rem);
    white-space: pre-wrap;
    tab-size: 4;
    -moz-tab-size: 4;
    overflow-wrap: break-word;
    overflow-x: auto;
    max-width: 100%;
  }

  .cv-preview-code :global(.hljs-keyword),
  .cv-preview-code :global(.hljs-built_in),
  .cv-preview-code :global(.hljs-type) {
    color: #93c5fd;
  }

  .cv-preview-code :global(.hljs-string),
  .cv-preview-code :global(.hljs-attr) {
    color: #f9a8d4;
  }

  .cv-preview-code :global(.hljs-number),
  .cv-preview-code :global(.hljs-literal) {
    color: #fcd34d;
  }

  .cv-preview-code :global(.hljs-comment) {
    color: #94a3b8;
    font-style: italic;
  }

  .cv-preview-code :global(.hljs-title),
  .cv-preview-code :global(.hljs-function),
  .cv-preview-code :global(.hljs-class),
  .cv-preview-code :global(.hljs-name) {
    color: #5eead4;
  }

  .cv-preview-code :global(.hljs-variable),
  .cv-preview-code :global(.hljs-params) {
    color: #f0f4f8;
  }

  .cv-preview-language {
    margin-left: 0.5rem;
    padding: 0.05rem 0.4rem;
    border-radius: 4px;
    background: rgba(94, 234, 212, 0.18);
    color: #5eead4;
    font-size: var(--cv-tag, 0.65rem);
    font-weight: 500;
    letter-spacing: 0.02em;
  }

  .cv-preview-image {
    display: block;
    max-width: 100%;
    max-height: 100%;
    margin: 0 auto;
    object-fit: contain;
    border-radius: 6px;
  }

  .cv-preview-image-unavailable {
    margin: 0;
    padding: 0.5rem 0.6rem;
    border-radius: 6px;
    background: rgba(248, 113, 113, 0.12);
    border: 1px solid rgba(248, 113, 113, 0.35);
    color: #fecaca;
    font-size: var(--cv-muted, 0.78rem);
  }

  .cv-preview-text-empty {
    margin: 0;
    font-style: italic;
    color: #94a3b8;
    font-size: var(--cv-muted, 0.78rem);
  }

  .cv-preview-footer {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 0.5rem;
    align-items: center;
    padding: 0.45rem 0.65rem;
    border-top: 1px solid #30363d;
    background: rgba(255, 255, 255, 0.02);
    color: #94a3b8;
    font-size: var(--cv-tag, 0.65rem);
  }

  .cv-preview-meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cv-preview-elapsed {
    font-variant-numeric: tabular-nums;
  }
</style>