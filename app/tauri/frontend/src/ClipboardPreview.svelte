<script lang="ts">
  /**
   * Shared capture preview for the Desktop and Quick Paste surfaces.
   *
   * Text and rich-text previews use an in-window overlay. Native image
   * previews use the same component in standalone page mode, without an inner
   * overlay. Image previews also expose persistent quarter-turn controls.
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
   *     as a Svelte text node, so the visible characters stay intact
   *     while the framework safely escapes markup;
   *   - resolves image rows through the existing
   *     `createClipboardAssetResolver` and `clipboardAssetCommand`,
   *     keeping the loading / loaded / error three-state machine
   *     and the stale-response guard that the rail and Quick Paste
   *     thumbnails already enforce;
   *   - revokes every blob URL through `releaseFor` / `release` so
   *     a long-lived window does not leak memory;
   *   - closes only from the close button or `Escape`, and returns
   *     focus to the trigger element when the parent provides one;
   *   - never copies, pastes, pins, tags or deletes an entry. Only the two
   *     explicit image rotation controls update an image capture.
   *
   * The contract documented in the `desktop-card-preview` design
   * pins:
   *
   *   - "stale responses are discarded" → every `resolve` round
   *     bumps a per-instance token; only the freshest result wins;
   *   - "open and close is leak-free" → the `onDestroy` hook
   *     revokes the current blob URL and clears the token map;
   *   - "only one preview open at a time" → the parent owns the
   *     visibility state; the component renders either a modal dialog or
   *     standalone page when `entry` is not `null` and nothing otherwise;
   *   - "return focus" → when `trigger` is provided, the
   *     close path calls `trigger.focus()` (best-effort) so the
   *     user lands back on the affordance they used.
   */
  import { onDestroy, onMount, tick } from "svelte";
  import CircularLoadingIndicator from "./CircularLoadingIndicator.svelte";
  import type { EntryRecord } from "./types.ts";
  import {
    CONTENT_TYPE_ICON_SPRITE,
    contentTypeIconId,
    contentTypeTranslationKey,
  } from "./lib/contentTypeIcons.ts";
  import { sourceAppAccessibleLabel } from "./lib/sourceAppFallback.ts";
  import { formatElapsedTime } from "./lib/elapsedTime.ts";
  import { localeStore, t } from "./lib/localization.ts";
  import {
    createClipboardAssetResolver,
    entryFullPreviewText,
    entryRawContent,
    hasRenderableImage,
    isImageEntry,
  } from "./lib/clipboardAsset.ts";
  import { rotateImageCaptureCommand } from "./lib/tauri.ts";
  import {
    fitPreviewImage,
    type PreviewImageSize,
  } from "./lib/previewImageSizing.ts";
  import {
    IMAGE_PREVIEW_ZOOM_MAX,
    IMAGE_PREVIEW_ZOOM_MIN,
    IMAGE_PREVIEW_ZOOM_STEP,
    scaleImagePreviewZoom,
  } from "./lib/imagePreviewZoom.ts";
  import type { ImageRotationDirection } from "./types.ts";
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
  /** Render image content as the page of a native preview window, without a modal overlay. */
  export let standaloneImage: boolean = false;

  const assetResolver: IconResolver = createClipboardAssetResolver();

  type ImageState = "idle" | "loading" | "loaded" | "error";
  let imageState: ImageState = "idle";
  let imageUrl: string | null = null;
  let imageAssetRef: string | null = null;
  let imageToken = 0;
  let visibleEntry: EntryRecord | null = null;
  let previewToken = 0;
  let imageRotationBusy = false;
  let imageRotationRefreshing = false;
  let imageRotationError = false;
  let imageZoomScale = 1;
  let imageZoomGestureStartScale = 1;
  let canZoomCurrentImage = false;
  let previewImageSize: PreviewImageSize | null = null;
  let previewCardWidth: number | null = null;
  let previewCardHeight: number | null = null;
  let overlayEl: HTMLDivElement | null = null;
  let cardEl: HTMLDivElement | null = null;
  let headerEl: HTMLElement | null = null;
  let bodyEl: HTMLDivElement | null = null;
  let footerEl: HTMLElement | null = null;
  let rotationErrorEl: HTMLParagraphElement | null = null;

  function cssPixels(value: string): number {
    const parsed = Number.parseFloat(value);
    return Number.isFinite(parsed) ? parsed : 0;
  }

  function updatePreviewImageSize(currentEntry = visibleEntry): void {
    if (!currentEntry || !hasRenderableImage(currentEntry)) {
      previewImageSize = null;
      previewCardWidth = null;
      previewCardHeight = null;
      return;
    }
    if (!overlayEl || !cardEl || !headerEl || !bodyEl || !footerEl) return;

    const overlayRect = overlayEl.getBoundingClientRect();
    const overlayStyle = getComputedStyle(overlayEl);
    const cardStyle = getComputedStyle(cardEl);
    const bodyStyle = getComputedStyle(bodyEl);
    const rotationErrorStyle = rotationErrorEl
      ? getComputedStyle(rotationErrorEl)
      : null;
    const overlayHorizontalInsets =
      cssPixels(overlayStyle.paddingLeft) + cssPixels(overlayStyle.paddingRight);
    const overlayVerticalInsets =
      cssPixels(overlayStyle.paddingTop) + cssPixels(overlayStyle.paddingBottom);
    const cardHorizontalChrome =
      cssPixels(cardStyle.borderLeftWidth) +
      cssPixels(cardStyle.borderRightWidth) +
      cssPixels(bodyStyle.paddingLeft) +
      cssPixels(bodyStyle.paddingRight);
    const cardVerticalChrome =
      cssPixels(cardStyle.borderTopWidth) +
      cssPixels(cardStyle.borderBottomWidth) +
      cssPixels(bodyStyle.paddingTop) +
      cssPixels(bodyStyle.paddingBottom) +
      (rotationErrorEl?.getBoundingClientRect().height ?? 0) +
      cssPixels(rotationErrorStyle?.marginTop ?? "0") +
      cssPixels(rotationErrorStyle?.marginBottom ?? "0") +
      headerEl.getBoundingClientRect().height +
      footerEl.getBoundingClientRect().height;
    const imageAreaWidth = standaloneImage
      ? Math.max(
          0,
          bodyEl.clientWidth -
            cssPixels(bodyStyle.paddingLeft) -
            cssPixels(bodyStyle.paddingRight),
        )
      : Math.max(0, overlayRect.width - overlayHorizontalInsets - cardHorizontalChrome);
    const imageAreaHeight = standaloneImage
      ? Math.max(
          0,
          bodyEl.clientHeight -
            cssPixels(bodyStyle.paddingTop) -
            cssPixels(bodyStyle.paddingBottom),
        )
      : Math.max(0, overlayRect.height - overlayVerticalInsets - cardVerticalChrome);

    const fitted = fitPreviewImage(
      currentEntry.payload_width ?? 0,
      currentEntry.payload_height ?? 0,
      imageAreaWidth,
      imageAreaHeight,
      standaloneImage ? null : 1,
    );
    if (fitted) {
      previewImageSize = fitted;
      previewCardWidth = standaloneImage ? null : fitted.width + cardHorizontalChrome;
      previewCardHeight = standaloneImage ? null : fitted.height + cardVerticalChrome;
    }
  }

  function handlePreviewResize(): void {
    if (imageState === "loaded") updatePreviewImageSize();
  }

  function syncEntry(): void {
    if (entry === null) {
      if (visibleEntry !== null) {
        imageToken += 1;
        if (imageAssetRef) assetResolver.releaseFor(imageAssetRef);
        visibleEntry = null;
        previewToken += 1;
        imageRotationBusy = false;
        imageRotationRefreshing = false;
        imageRotationError = false;
        imageZoomScale = 1;
        imageZoomGestureStartScale = 1;
        previewImageSize = null;
        previewCardWidth = null;
        previewCardHeight = null;
        seedImageState();
      }
      return;
    }
    if (visibleEntry?.id !== entry.id) {
      imageToken += 1;
      if (imageAssetRef) assetResolver.releaseFor(imageAssetRef);
      visibleEntry = entry;
      previewToken += 1;
      imageRotationBusy = false;
      imageRotationRefreshing = false;
      imageRotationError = false;
      imageZoomScale = 1;
      imageZoomGestureStartScale = 1;
      previewImageSize = null;
      previewCardWidth = null;
      previewCardHeight = null;
      seedImageState();
    }
  }

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
    if (!visibleEntry) {
      imageState = "idle";
      imageUrl = null;
      imageAssetRef = null;
      return;
    }
    if (!isImageEntry(visibleEntry) || !hasRenderableImage(visibleEntry)) {
      imageState = isImageEntry(visibleEntry) ? "error" : "idle";
      imageUrl = null;
      imageAssetRef = null;
      return;
    }
    imageState = "loading";
    imageAssetRef = visibleEntry.asset_ref ?? null;
  }

  $: entry, syncEntry();

  async function refreshImage(assetRef: string | null): Promise<void> {
    const token = ++imageToken;
    if (!assetRef) {
      imageUrl = null;
      if (token === imageToken) {
        imageState = "error";
        imageRotationRefreshing = false;
      }
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
      await tick();
      if (token !== imageToken) return;
      updatePreviewImageSize();
      imageUrl = resolution.url;
      imageState = "loaded";
    } else {
      imageUrl = null;
      previewImageSize = null;
      previewCardWidth = null;
      previewCardHeight = null;
      imageState = "error";
    }
    imageRotationRefreshing = false;
  }

  $: if (visibleEntry && imageState === "loading" && imageAssetRef) {
    void refreshImage(imageAssetRef);
  }

  function onImageError(): void {
    if (!imageAssetRef) {
      imageUrl = null;
      previewImageSize = null;
      imageState = "error";
      return;
    }
    assetResolver.releaseFor(imageAssetRef);
    imageUrl = null;
    previewImageSize = null;
    previewCardWidth = null;
    previewCardHeight = null;
    imageState = "error";
  }

  async function rotateImage(direction: ImageRotationDirection): Promise<void> {
    if (!visibleEntry || imageRotationBusy || imageState !== "loaded") return;
    const entryId = visibleEntry.id;
    const token = previewToken;
    const previousAssetRef = imageAssetRef;
    imageRotationBusy = true;
    imageRotationError = false;

    try {
      const result = await rotateImageCaptureCommand({ entryId, direction });
      if (token !== previewToken || visibleEntry?.id !== entryId) return;
      if (result.kind !== "updated") {
        imageRotationError = true;
        void tick().then(() => updatePreviewImageSize());
        return;
      }

      imageToken += 1;
      if (previousAssetRef) assetResolver.releaseFor(previousAssetRef);
      imageUrl = null;
      visibleEntry = result.entry;
      imageAssetRef = result.entry.asset_ref ?? null;
      imageRotationRefreshing = Boolean(
        imageAssetRef && hasRenderableImage(result.entry),
      );
      imageState = imageRotationRefreshing ? "loading" : "error";
    } catch {
      if (token === previewToken && visibleEntry?.id === entryId) {
        imageRotationError = true;
        void tick().then(() => updatePreviewImageSize());
      }
    } finally {
      if (token === previewToken && visibleEntry?.id === entryId) {
        imageRotationBusy = false;
      }
    }
  }

  function isStandaloneImagePreview(): boolean {
    return standaloneImage && visibleEntry !== null && isImageEntry(visibleEntry);
  }

  function canZoomImage(): boolean {
    return canZoomCurrentImage;
  }

  $: canZoomCurrentImage =
    standaloneImage &&
    visibleEntry !== null &&
    isImageEntry(visibleEntry) &&
    imageState === "loaded" &&
    !imageRotationBusy &&
    !imageRotationRefreshing;

  function zoomImageBy(factor: number): void {
    if (!canZoomImage()) return;
    imageZoomScale = scaleImagePreviewZoom(imageZoomScale, factor);
  }

  function onImageZoomWheel(event: WheelEvent): void {
    if (!isStandaloneImagePreview() || (!event.ctrlKey && !event.metaKey)) return;
    event.preventDefault();
    if (!canZoomImage()) return;

    const factor = Math.exp(-event.deltaY * 0.002);
    imageZoomScale = scaleImagePreviewZoom(imageZoomScale, factor);
  }

  function onImageGestureStart(event: Event): void {
    if (!isStandaloneImagePreview()) return;
    event.preventDefault();
    imageZoomGestureStartScale = imageZoomScale;
  }

  function onImageGestureChange(event: Event): void {
    if (!isStandaloneImagePreview()) return;
    event.preventDefault();
    if (!canZoomImage()) return;

    const gestureScale = (event as Event & { scale?: number }).scale;
    if (typeof gestureScale !== "number" || !Number.isFinite(gestureScale)) return;
    imageZoomScale = scaleImagePreviewZoom(imageZoomGestureStartScale, gestureScale);
  }

  function onImageGestureEnd(event: Event): void {
    if (!isStandaloneImagePreview()) return;
    event.preventDefault();
  }

  function close(): void {
    onClose?.();
  }

  function onOverlayKeydown(event: KeyboardEvent): void {
    if (!standaloneImage && event.key === "Escape") {
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
    window.addEventListener("resize", handlePreviewResize);
    if (standaloneImage) {
      document.addEventListener("wheel", onImageZoomWheel, {
        capture: true,
        passive: false,
      });
      document.addEventListener("gesturestart", onImageGestureStart, {
        capture: true,
        passive: false,
      });
      document.addEventListener("gesturechange", onImageGestureChange, {
        capture: true,
        passive: false,
      });
      document.addEventListener("gestureend", onImageGestureEnd, {
        capture: true,
        passive: false,
      });
    }
    // Move focus into the preview so keyboard users can reach its controls and
    // Escape works immediately in the standalone native window.
    void tick().then(() => {
      if (standaloneImage) overlayEl?.focus();
      else closeButtonEl?.focus();
    });
  });

  onDestroy(() => {
    window.removeEventListener("resize", handlePreviewResize);
    if (standaloneImage) {
      document.removeEventListener("wheel", onImageZoomWheel, true);
      document.removeEventListener("gesturestart", onImageGestureStart, true);
      document.removeEventListener("gesturechange", onImageGestureChange, true);
      document.removeEventListener("gestureend", onImageGestureEnd, true);
    }
    // Revoke the current image's blob URL so a remount does not
    // leak the previous overlay's bytes; the resolver cache still
    // serves the URL for any consumer that already holds a
    // reference to it.
    if (imageAssetRef) {
      assetResolver.releaseFor(imageAssetRef);
    }
    imageUrl = null;
    imageState = "idle";
    previewImageSize = null;
    previewCardWidth = null;
    previewCardHeight = null;
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
   * dismissal path (Escape or close button).
   */
  function handleClose(): void {
    restoreFocus();
    close();
  }

  $: fullText = visibleEntry == null ? "" : entryFullPreviewText(visibleEntry);
  /**
   * Raw textual content of the entry, with no whitespace collapse and
   * no trim. The highlighted preview feeds THIS string into
   * `renderHighlightedCode` so the same `\n`, `\r\n`, `\t` and empty
   * lines the source application produced survive both the
   * highlighting and the subsequent `<pre>` mount. The
   * `entryFullPreviewText` helper above preserves the same
   * whitespace characters so the plain-text path renders the
   * captured block byte-for-byte, the same way the highlighted
   * branch does — both surfaces consult the same canonical helper.
   */
  $: rawText = visibleEntry == null ? "" : entryRawContent(visibleEntry);
  $: highlightedHtml = (() => {
    if (visibleEntry == null || !shouldRenderHighlightedPreview(visibleEntry) || rawText.length === 0) {
      return null;
    }
    return renderHighlightedCode(rawText, visibleEntry.code_language).html;
  })();
  $: titleLabel =
    visibleEntry == null
      ? ""
      : (visibleEntry.title ?? "").trim().length > 0
        ? (visibleEntry.title ?? "").trim()
        : $t(contentTypeTranslationKey(visibleEntry.content_type));
  $: derivedAccessibleLabel =
    accessibleLabel ?? (visibleEntry == null ? "" : $t("preview.accessible", { title: titleLabel }));
  $: sourceLabel =
    visibleEntry == null ? "" : sourceAppAccessibleLabel(visibleEntry, $t);
  $: typeLabel =
    visibleEntry == null ? "" : $t(contentTypeTranslationKey(visibleEntry.content_type));
  $: codeLanguageLabel =
    visibleEntry == null || !shouldShowCodeLanguageBadge(visibleEntry)
      ? ""
      : canonicalCodeLanguageLabel(visibleEntry.code_language);
</script>

{#if visibleEntry != null}
  {@const record = visibleEntry}
  <div
    bind:this={overlayEl}
    class="cv-preview-overlay"
    class:cv-preview-standalone={standaloneImage}
    role={standaloneImage ? "main" : "dialog"}
    aria-modal={standaloneImage ? undefined : "true"}
    aria-label={derivedAccessibleLabel}
    tabindex="-1"
    data-testid={standaloneImage ? `${testIdPrefix}-surface` : `${testIdPrefix}-overlay`}
    data-entry-id={record.id}
    data-content-type={record.content_type}
    on:keydown={onOverlayKeydown}
  >
    <div
      bind:this={cardEl}
      class="cv-preview-card"
      class:cv-preview-card-image={isImageEntry(record)}
      class:cv-preview-card-standalone={standaloneImage}
      style={previewCardWidth !== null && previewCardHeight !== null
        ? `width: ${previewCardWidth}px; height: ${previewCardHeight}px;`
        : undefined}
      data-testid="{testIdPrefix}-card"
      data-content-type={record.content_type}
      tabindex="-1"
    >
      <header class="cv-preview-header" bind:this={headerEl}>
        <span
          class="cv-preview-type"
          data-testid="{testIdPrefix}-type"
          data-content-type={record.content_type}
          aria-hidden="true"
          title={$t("history.card.content_type", { type: typeLabel })}
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
        <div class="cv-preview-header-actions">
          {#if isImageEntry(record)}
            <button
              type="button"
              class="cv-preview-rotate"
              data-testid="{testIdPrefix}-rotate-counterclockwise"
              aria-label={$t("preview.rotate_counterclockwise")}
              title={$t("preview.rotate_counterclockwise")}
              disabled={imageRotationBusy || imageState !== "loaded"}
              on:click={() => void rotateImage("counterclockwise")}
            >
              <svg aria-hidden="true" focusable="false" width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <path d="M3 7v6h6" />
                <path d="M21 17a9 9 0 0 0-15.5-6.5L3 13" />
              </svg>
            </button>
            <button
              type="button"
              class="cv-preview-rotate"
              data-testid="{testIdPrefix}-rotate-clockwise"
              aria-label={$t("preview.rotate_clockwise")}
              title={$t("preview.rotate_clockwise")}
              disabled={imageRotationBusy || imageState !== "loaded"}
              on:click={() => void rotateImage("clockwise")}
            >
              <svg aria-hidden="true" focusable="false" width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <path d="M21 7v6h-6" />
                <path d="M3 17a9 9 0 0 1 15.5-6.5L21 13" />
              </svg>
            </button>
            {#if standaloneImage}
              <button
                type="button"
                class="cv-preview-zoom"
                data-testid="{testIdPrefix}-zoom-in"
                aria-label={$t("preview.zoom_in")}
                title={$t("preview.zoom_in")}
                disabled={!canZoomCurrentImage || imageZoomScale >= IMAGE_PREVIEW_ZOOM_MAX}
                on:click={() => zoomImageBy(IMAGE_PREVIEW_ZOOM_STEP)}
              >
                <svg aria-hidden="true" focusable="false" width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                  <circle cx="10.8" cy="10.8" r="6.8" />
                  <path d="m16 16 4.5 4.5" />
                  <path d="M10.8 7.8v6" />
                  <path d="M7.8 10.8h6" />
                </svg>
              </button>
              <button
                type="button"
                class="cv-preview-zoom"
                data-testid="{testIdPrefix}-zoom-out"
                aria-label={$t("preview.zoom_out")}
                title={$t("preview.zoom_out")}
                disabled={!canZoomCurrentImage || imageZoomScale <= IMAGE_PREVIEW_ZOOM_MIN}
                on:click={() => zoomImageBy(1 / IMAGE_PREVIEW_ZOOM_STEP)}
              >
                <svg aria-hidden="true" focusable="false" width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                  <circle cx="10.8" cy="10.8" r="6.8" />
                  <path d="m16 16 4.5 4.5" />
                  <path d="M7.8 10.8h6" />
                </svg>
              </button>
            {/if}
          {/if}
          {#if !standaloneImage}
            <button
              type="button"
              class="cv-preview-close"
              data-testid="{testIdPrefix}-close"
              aria-label={$t("preview.close")}
              title={$t("preview.close_escape")}
              bind:this={closeButtonEl}
              on:click={handleClose}
            >
              ×
            </button>
          {/if}
        </div>
      </header>
      <div
        class="cv-preview-body"
        class:cv-preview-body-image={isImageEntry(record)}
        class:cv-preview-body-image-standalone={standaloneImage}
        bind:this={bodyEl}
        data-testid="{testIdPrefix}-body"
        data-content-type={record.content_type}
        aria-busy={imageRotationBusy || imageRotationRefreshing}
      >
        {#if isImageEntry(record)}
          {#if imageRotationBusy || imageRotationRefreshing}
            {#if imageUrl && imageState === "loaded"}
              <CircularLoadingIndicator
                label={$t("preview.loading_image")}
                testId={`${testIdPrefix}-image-rotation-spinner`}
                variant="overlay"
              />
            {:else}
              <div class="cv-preview-image-refreshing">
                <CircularLoadingIndicator
                  label={$t("preview.loading_image")}
                  testId={`${testIdPrefix}-image-rotation-spinner`}
                  variant="overlay"
                />
              </div>
            {/if}
          {/if}
          {#if imageRotationError}
            <p class="cv-preview-image-rotation-error" bind:this={rotationErrorEl} role="alert" data-testid="{testIdPrefix}-rotation-error">
              {$t("preview.image_rotation_error")}
            </p>
          {/if}
          {#if imageUrl && imageState === "loaded"}
            <img
              class="cv-preview-image"
              src={imageUrl}
              alt={titleLabel}
              data-testid="{testIdPrefix}-image"
              style:transform={standaloneImage ? `scale(${imageZoomScale})` : "none"}
              style={previewImageSize
                ? `width: ${previewImageSize.width}px; height: ${previewImageSize.height}px;`
                : undefined}
              on:error={onImageError}
            />
          {:else if record.asset_ref && hasRenderableImage(record) && imageState === "loading"}
            {#if !imageRotationBusy && !imageRotationRefreshing}
              <p
                class="cv-preview-image-unavailable"
                data-testid="{testIdPrefix}-image-loading"
              >
                {$t("preview.loading_image")}
              </p>
            {/if}
          {:else if !imageRotationBusy && !imageRotationRefreshing}
            <p
              class="cv-preview-image-unavailable"
              data-testid="{testIdPrefix}-image-unavailable"
            >
              {$t("preview.unavailable")}
            </p>
          {/if}
        {:else if fullText.length === 0}
          <p
            class="cv-preview-text-empty"
            data-testid="{testIdPrefix}-text-empty"
          >
            {$t("preview.empty_capture")}
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
          >{fullText}</pre>
        {/if}
      </div>
      <footer class="cv-preview-footer" class:cv-preview-footer-hidden={standaloneImage} bind:this={footerEl}>
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
            {$t("preview.code_language", { language: codeLanguageLabel })}
          </span>
        {/if}
        <span
          class="cv-preview-elapsed"
          data-testid="{testIdPrefix}-elapsed"
        >
          {formatElapsedTime(record.created_at, new Date(), $localeStore).visual}
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

  .cv-preview-standalone {
    position: relative;
    inset: auto;
    display: block;
    width: 100%;
    height: 100%;
    padding: 0;
    overflow: hidden;
    background: #0e1116;
    z-index: auto;
  }

  .cv-preview-card-standalone {
    width: 100%;
    height: 100%;
    max-width: 100%;
    max-height: 100%;
    border: 0;
    border-radius: 0;
    box-shadow: none;
    grid-template-rows: auto minmax(0, 1fr);
  }

  .cv-preview-card-image {
    position: relative;
    box-sizing: border-box;
    width: fit-content;
    max-width: 100%;
    grid-template-rows: auto auto auto;
  }

  .cv-preview-card-standalone.cv-preview-card-image {
    width: 100%;
    max-width: 100%;
    grid-template-rows: auto minmax(0, 1fr);
  }

  .cv-preview-standalone .cv-preview-header {
    grid-template-columns: minmax(0, 1fr) auto;
  }

  .cv-preview-standalone .cv-preview-type {
    display: none;
  }

  .cv-preview-standalone .cv-preview-body-image-standalone {
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    max-width: none;
    max-height: none;
    padding: 10px;
    overflow: hidden;
  }

  .cv-preview-footer-hidden {
    display: none;
  }

  .cv-preview-header {
    display: grid;
    grid-template-columns: 18px minmax(0, 1fr) auto;
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

  .cv-preview-header-actions {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
  }

  .cv-preview-rotate {
    width: 26px;
    height: 26px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: #94a3b8;
    cursor: pointer;
  }

  .cv-preview-rotate:hover:not(:disabled) {
    background: rgba(148, 163, 184, 0.18);
    color: #f0f4f8;
  }

  .cv-preview-rotate:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .cv-preview-rotate:focus-visible {
    outline: 1px solid #2563eb;
    outline-offset: 1px;
  }

  .cv-preview-zoom {
    width: 26px;
    height: 26px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: #94a3b8;
    cursor: pointer;
  }

  .cv-preview-zoom:hover:not(:disabled) {
    background: rgba(148, 163, 184, 0.18);
    color: #f0f4f8;
  }

  .cv-preview-zoom:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .cv-preview-zoom:focus-visible {
    outline: 1px solid #2563eb;
    outline-offset: 1px;
  }

  .cv-preview-body {
    position: relative;
    padding: 0.6rem 0.7rem;
    overflow: auto;
    background: rgba(0, 0, 0, 0.18);
    color: #cbd5f5;
    min-height: 0;
  }

  .cv-preview-body-image {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    width: fit-content;
    height: fit-content;
    max-width: 100%;
    max-height: 100%;
    padding: 10px;
    overflow: hidden;
  }

  .cv-preview-image-rotation-error {
    margin: 0 0 0.5rem;
    max-width: 100%;
    overflow-wrap: anywhere;
    text-align: center;
    color: #fca5a5;
    font-size: var(--cv-caption, 0.72rem);
  }

  .cv-preview-image-refreshing {
    position: relative;
    min-height: min(16rem, 50vh);
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
    flex: 0 0 auto;
    object-fit: contain;
    border-radius: 6px;
    transform-origin: center center;
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
