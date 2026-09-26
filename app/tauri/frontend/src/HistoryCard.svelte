<script lang="ts">
  /**
   * Compact card the rail renders for every recent capture.
   *
   * The card is square (240×240 CSS pixels, controlled by the
   * `--cv-card-size` token), with four logical regions:
   *
   *   - upper area: content-type icon (left), title (centred) and the
   *     inline editor that activates on double click, Enter on the
   *     focused title or F2, with accessible confirm/cancel icons;
   *     source-app icon and name (right);
   *   - metadata strip: elapsed time relative to `created_at` (left)
   *     and the canonical character / size counter for the payload
   *     (right). Both pieces of metadata come from pure helpers so
   *     they can be unit-tested without the card mounted;
   *   - middle area: a bounded, clipped plain-text preview of the
   *     captured content. Rich entries use the canonical plain text
   *     too — the rich preview is kept server-side for the paste
   *     path but is no longer rendered here;
   *   - lower action area: pin/unpin and the ellipsis menu button
   *     immediately to its left.
   *
   * The card never lets the preview reshape its own size: long text
   * is clipped with `text-overflow` and `line-clamp` and a thumbnail
   * is constrained with `object-fit: contain`. The rail stays a
   * uniform row of squares even when a capture overflows.
   *
   * An image or rich-text capture reuses this exact card — there is
   * no parallel gallery. Only the middle region differs, and every
   * action (pin/unpin, title editing, delete, paste, source-app icon)
   * behaves identically across all payload shapes. The rich paste
   * action remains available from the ellipsis menu so the captured
   * HTML/RTF bytes can still be re-published on demand.
   */
  import { createEventDispatcher, onDestroy, onMount } from "svelte";
  import type {
    Collection,
    EntryRecord,
    PeerImportedSourceAppPresentation,
    Tag,
  } from "./types";
  import {
    defaultCardTitle,
    validateTitle,
  } from "./lib/contentType";
  import {
    canonicalLabel as canonicalCodeLanguageLabel,
    renderHighlightedCode,
  } from "./lib/codeLanguageDetector";
  import {
    shouldRenderHighlightedPreview,
    shouldShowCodeLanguageBadge,
  } from "./lib/codeLanguageProjections";
  import {
    previewShortcutAccessibleLabel,
    previewShortcutLabel,
    type PreviewShortcutPlatform,
  } from "./lib/clipboardPreview";
  import {
    editTextShortcutAccessibleLabel,
    editTextShortcutKeyAttribute,
    editTextShortcutLabel,
    editTextShortcutPlatform,
    matchesEditTextShortcut as matchesEditTextShortcutHelper,
  } from "./lib/editTextShortcut";
  import {
    cardMenuPreviewShortcutAccessibleLabel,
    cardMenuPreviewShortcutKeyAttribute,
    cardMenuPreviewShortcutLabel,
    cardMenuStyle,
    computeCardMenuPosition,
    recomputeCardMenuPositionForActualHeight,
    type CardMenuPosition,
  } from "./lib/cardMenuPositioning.ts";
  import {
    APP_FALLBACK_ICON_SVG,
    CONTENT_TYPE_ICON_SPRITE,
    contentTypeIconId,
    contentTypeIconLabel,
  } from "./lib/contentTypeIcons";
  import {
    createIconResolver,
    type IconLoader,
    type IconResolver,
  } from "./lib/iconResolver";
  import {
    createClipboardAssetResolver,
    entryPreviewText,
    entryRawContent,
    hasRenderableImage,
    hasRenderableRichText,
    imageDimensionsLabel,
    isImageEntry,
    pasteMenuActionsFor,
  } from "./lib/clipboardAsset";
  import {
    sourceAppAccessibleLabel,
  } from "./lib/sourceAppFallback";
  import { formatElapsedTime, type ElapsedTime } from "./lib/elapsedTime";
  import { unicodeCount } from "./lib/unicodeCount";
  import { formatByteSize } from "./lib/byteSize";
  import {
    pasteEntryCommand,
    setEntryTitleCommand,
    sourceAppIconCommand,
  } from "./lib/tauri";
  import { collectionColor } from "./lib/collectionColor";
  import {
    COLLECTION_OVERFLOW_TOLERANCE_PX,
    computeOverflowCount,
    computeVisibleCollections,
    intrinsicCollectionRowWidth,
  } from "./lib/collectionChipLayout";
  import TagSelectorModal from "./TagSelectorModal.svelte";
  import CollectionSelectorModal from "./CollectionSelectorModal.svelte";
  import CollectionMembershipModal from "./CollectionMembershipModal.svelte";
  import EntryTextEditorModal from "./EntryTextEditorModal.svelte";
  import { isEditableTextEntry } from "./types";

  const IMPORTED_SOURCE_APP_FALLBACK_ICON_SVG =
    '<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><path d="M12 3v11m0 0 4-4m-4 4-4-4M5 15v4a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-4" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>';

  export let entry: EntryRecord;
  /** Present only while rendering the matching peer-bound import collection. */
  export let peerImportedSourceApp: PeerImportedSourceAppPresentation | null = null;
  export let onTogglePin: (entry: EntryRecord) => void = () => {};
  export let onRequestDelete: (entry: EntryRecord) => void = () => {};
  export let onAfterMutation: (entry: EntryRecord) => void = () => {};
  export let assignedTags: Tag[] = [];
  export let assignedCollections: Collection[] = [];
  export let allTags: Tag[] = [];
  export let allCollections: Collection[] = [];
  export let activeCollectionId: number | null = null;
  /**
   * Whether the per-entry organisation cache for this card has been
   * hydrated from SQLite. The **Editar tags** modal MUST refuse
   * **Guardar** while this is `false` so the user cannot write a
   * stale `[]` selection that would clobber persisted tags. The
   * state is forwarded by the rail from `App.svelte`'s
   * `entryOrganizationHydration` map.
   */
  export let entryOrganizationLoaded: boolean = false;
  /**
   * Coarse hydration state surfaced through the chip row so the user
   * sees a loading / error indicator next to the affected card
   * instead of a blank space the modal could mistake for an empty
   * tag set.
   */
  export let entryOrganizationState: "pending" | "loaded" | "error" = "pending";
  export let onAssignTags: (
    entry: EntryRecord,
    tagIds: number[],
  ) => Promise<void> = async () => {};
  export let onAssignCollections: (
    entry: EntryRecord,
    collectionIds: number[],
  ) => Promise<void> = async () => {};
  export let onRemoveFromCollection: (
    entry: EntryRecord,
    collectionId: number,
  ) => Promise<void> = async () => {};
  /**
   * Whether this card is the rail's currently selected entry. The
   * flag is local UI state: a `true` value exposes `aria-selected`,
   * paints the selected styling and shows the platform-aware preview
   * shortcut hint (the same matcher `ClipboardPreview` consumes).
   * Selection is purely visual; the card never writes the value back
   * to SQLite, the clipboard or a backend command.
   */
  export let selected: boolean = false;
  /**
   * Forward the user gesture that flipped selection. The rail owns
   * the canonical id so two cards racing a `mousedown`/`keydown`
   * cannot end up with two selections; the helper receives the id
   * the user just picked (or `null` to clear the selection).
   */
  export let onSelect: (entryId: number | null) => void = () => {};
  /**
   * Whether this card's ellipsis menu is currently open. The flag
   * is owned by the rail (`openCardId === entry.id`); the card is
   * the thin renderer and never persists or duplicates the value.
   * Treating the rail as the single source of truth is what lets
   * Escape, outside clicks and the rail's `closeAllMenus` helper
   * dismiss the popover without racing the card's local state.
   */
  export let menuOpen: boolean = false;
  /**
   * Optional callback the rail installs to receive the card's
   * root `<article>` element. The horizontal keyboard navigation
   * the rail exposes (`ArrowLeft` / `ArrowRight`) calls
   * `scrollIntoView({ block: "nearest", inline: "nearest" })` on
   * the freshly selected card so the user can see the card that
   * just became active without ever leaving the rail. The card
   * invokes the callback once per mount and once per unmount so
   * the rail's registry stays in lock-step with the DOM; the
   * callback is no-op by default so consumers that don't need the
   * element never have to think about it.
   */
  export let onCardRef: (el: HTMLElement | null) => void = () => {};

  const dispatch = createEventDispatcher<{
    "menu-toggle": { id: number; open: boolean };
    "preview-request": { id: number };
    "select-request": { id: number | null };
  }>();

  /** Local override of the persisted title so edits feel instant. */
  let localTitle: string | null = null;
  $: displayTitle =
    (localTitle ?? entry.title ?? "").trim().length > 0
      ? (localTitle ?? entry.title ?? "").trim()
      : defaultCardTitle(entry.content_type);

  /** Stable DOM id for the popover. The rail enforces the
   * single-open invariant; this card receives `menuOpen` from the
   * parent and only renders the popover when the flag is true. */
  let menuId = `card-menu-${entry.id}`;

  /**
   * Reference to the root `<article>` element. The rail mounts a
   * callback through `onCardRef` so it can drive
   * `scrollIntoView({ block: "nearest", inline: "nearest" })` on
   * the freshly selected card after an `ArrowLeft` / `ArrowRight`
   * press. We forward the element reactively so the rail's
   * registry stays in lock-step with the DOM through remounts and
   * entry swaps.
   */
  let cardArticleEl: HTMLElement | null = null;
  $: onCardRef(cardArticleEl);
  /**
   * Open the text editor when the rail forwards a
   * `card-edit-text-shortcut` event on this card's `<article>`.
   *
   * The desktop shell owns the keyboard matcher (`Cmd/Ctrl+E`) and
   * resolves the entry id; the rail re-dispatches the request on
   * the matching card's article so each card receives exactly one
   * event per shortcut press. The listener is added through
   * `addEventListener` so the Svelte type checker does not have to
   * know about the custom event name (Svelte 5 HTML props only
   * know the documented DOM event set); the listener is detached on
   * `onDestroy` so a remount can never leak a handler.
   *
   * The card MUST validate the dispatched `{ entryId }` against its
   * own `entry.id` so a stale, duplicate or mismatched request
   * cannot reopen a wrong modal. The rail already routes the
   * request through `cardEls.get(detail.entryId)` so the dispatch
   * targets the right article, but a future caller or a re-mounted
   * card could still surface a stale id; the card-side guard is the
   * last line of defence.
   */
  function handleCardEditTextShortcut(event: Event): void {
    const detail = (event as CustomEvent<{ entryId: number }>).detail;
    if (!detail || typeof detail.entryId !== "number") return;
    if (detail.entryId !== entry.id) {
      // Mismatched id: a stale dispatch reached this card or the
      // rail forwarded a request for a different entry. The card
      // MUST NOT open its modal in that case.
      return;
    }
    openTextEditor();
  }
  /**
   * Close the text editor when the rail broadcasts a
   * `card-edit-text-shortcut-close` event on this card's
   * `<article>` before opening the new target's modal. The
   * contract "at most one active edit modal" lives in the rail;
   * the card only flips `textEditorOpen` back to `false` when it
   * was actually open so the close event is a safe no-op on a card
   * that never opened its editor. The listener does not need to
   * inspect any payload: the request is directed at the rail level,
   * never at a specific target id.
   */
  function handleCardEditTextShortcutClose(): void {
    if (!textEditorOpen) return;
    // This is a rail-forced close while another card becomes the target;
    // do not restore focus to this obsolete card.
    textEditorReturnFocusTarget = null;
    textEditorOpen = false;
  }
  $: if (cardArticleEl) {
    cardArticleEl.addEventListener(
      "card-edit-text-shortcut",
      handleCardEditTextShortcut,
    );
    cardArticleEl.addEventListener(
      "card-edit-text-shortcut-close",
      handleCardEditTextShortcutClose,
    );
  }
  onDestroy(() => {
    if (cardArticleEl) {
      cardArticleEl.removeEventListener(
        "card-edit-text-shortcut",
        handleCardEditTextShortcut,
      );
      cardArticleEl.removeEventListener(
        "card-edit-text-shortcut-close",
        handleCardEditTextShortcutClose,
      );
    }
  });

  /** Title-editing state. */
  let editingTitle = false;
  let titleDraft = "";
  let titleError: string | null = null;
  let titleBusy = false;
  let titleInputEl: HTMLInputElement | null = null;

  $: iconRef = peerImportedSourceApp !== null
    ? peerImportedSourceApp.source_app_icon_ref
    : entry.source_app_icon_ref ?? null;
  $: sourceAppLabel = peerImportedSourceApp !== null
    ? `Aplicación fuente: ${peerImportedSourceApp.source_app_name?.trim() || "desconocida"}`
    : sourceAppAccessibleLabel(entry);
  /** Mirror of `iconRef` we update synchronously so the
   * `onDestroy` cleanup can release the cached blob URL of the
   * current entry even when the component is rebuilt before
   * Svelte flushes its reactive deps. */
  let lastIconRef: string | null = null;

  const tauriSourceAppIconLoader: IconLoader = {
    async loadIconBytes(ref: string): Promise<number[] | Uint8Array | null> {
      try {
        return await sourceAppIconCommand({ ref });
      } catch {
        return null;
      }
    },
  };

  const iconResolver: IconResolver = createIconResolver(tauriSourceAppIconLoader);
  let appIconUrl: string | null = null;
  /**
   * Sequence number for the in-flight `refreshAppIcon` invocation. The
   * card may re-mount or change `entry` while a previous Tauri bridge
   * call is still pending; the guard makes sure only the most recent
   * refresh writes to `appIconUrl` so the card never renders a stale
   * icon that belongs to the previous capture.
   */
  let iconRefreshToken = 0;

  async function refreshAppIcon(ref: string | null): Promise<void> {
    const token = ++iconRefreshToken;
    if (!ref) {
      if (token === iconRefreshToken) {
        appIconUrl = null;
      }
      return;
    }
    const resolution = await iconResolver.resolve(ref);
    if (token !== iconRefreshToken) {
      // A newer refresh has been scheduled while we awaited the
      // bridge; drop the result so we never overwrite the icon the
      // current entry already owns.
      return;
    }
    if (resolution.ok && resolution.url) {
      appIconUrl = resolution.url;
    } else {
      appIconUrl = null;
    }
  }

  function syncIconRef(ref: string | null): void {
    if (ref === lastIconRef) return;
    if (lastIconRef !== null) iconResolver.releaseFor(lastIconRef);
    lastIconRef = ref;
    void refreshAppIcon(ref);
  }

  $: syncIconRef(iconRef);

  /**
   * Truncate the preview for display. The card always shows the
   * first ~120 characters and trims whitespace so the layout stays
   * stable; the full text remains in the persisted record.
   *
   * For an image entry this returns a localised placeholder instead of
   * the backend's empty `content` sentinel, which must never reach the
   * DOM. The placeholder is only rendered by the accessible fallback
   * (the thumbnail replaces it when the bytes load).
   */
  $: previewText = entryPreviewText(entry);
  /**
   * Highlighted preview HTML the card renders when the entry has a
   * canonical, allowlist-accepted `code_language`. The computation
   * reuses the same `renderHighlightedCode` helper the shared
   * `ClipboardPreview` overlay consumes, so Desktop cards and the
   * overlay render the same syntax colours, tabs, indentation and
   * line breaks — the spec the
   * `clipboard-history-cards` change pins as a non-negotiable
   * contract between the two surfaces.
   *
   * The helper is gated by `shouldRenderHighlightedPreview` so a
   * non-code entry, an image, a rich text row or a payload without
   * a canonical language keeps the plain-text fallback path the
   * regression suite pins. The computation falls through to an
   * empty string when the entry does not qualify so the markup
   * below only mounts the highlighted branch when the helper
   * actually produced output.
   */
  $: highlightedPreviewHtml = (() => {
    if (!shouldRenderHighlightedPreview(entry)) return "";
    const raw = entryRawContent(entry);
    if (raw.length === 0) return "";
    return renderHighlightedCode(raw, entry.code_language).html;
  })();

  // ---------------------------------------------------------------
  // Metadata strip: elapsed time + payload size / character count.
  //
  // The strip is updated by a single timer local to the desktop so a
  // long-lived rail never re-fetches the assets or remonte the
  // cards — the elapsed label is re-derived in place at most once
  // per `METADATA_REFRESH_MS`. The helpers are pure and reusable.
  // ---------------------------------------------------------------

  const METADATA_REFRESH_MS = 30_000;
  let nowAnchor = Date.now();

  function refreshNowAnchor(): void {
    nowAnchor = Date.now();
  }

  let metadataTimer: ReturnType<typeof setInterval> | null = null;

  onMount(() => {
    refreshNowAnchor();
    metadataTimer = setInterval(refreshNowAnchor, METADATA_REFRESH_MS);
  });

  onDestroy(() => {
    if (metadataTimer !== null) {
      clearInterval(metadataTimer);
      metadataTimer = null;
    }
  });

  $: elapsed = formatElapsedTime(entry.created_at, new Date(nowAnchor));
  $: characterCount = unicodeCount(entry.content);
  $: byteSizeInfo = formatByteSize(entry.content_size);

  $: ageLabel = (() => {
    const e: ElapsedTime = elapsed;
    return {
      aria: e.accessible,
      visual: e.visual,
    };
  })();

  // ---------------------------------------------------------------
  // Image payload: thumbnail resolution.
  //
  // The card requests bytes through the same validated backend bridge
  // the source-app icon uses and renders them from a `blob:` URL. It
  // never receives, renders or logs the `asset_ref`, and it revokes the
  // URL when the entry changes or the card is destroyed.
  //
  // The thumbnail surface tracks an explicit three-state machine so a
  // pending load never flashes the "Imagen no disponible" fallback the
  // reader used to render while `thumbnailUrl` was still `null`:
  //
  //   - `"loading"` — a coherent `asset_ref` is present, the bridge
  //     round-trip is in flight and the card shows a non-error
  //     placeholder. This is the initial state for every fresh card
  //     that survived `hasRenderableImage`, including the bootstrap
  //     path after a restart.
  //   - `"loaded"` — the bridge returned bytes, a `blob:` URL was
  //     minted and the card renders the actual thumbnail. The URL is
  //     cached on the resolver so subsequent re-mounts of the same
  //     entry reuse it instead of triggering a second round-trip.
  //   - `"error"` — the bridge round-trip failed (asset missing,
  //     corrupt PNG, namespace violation, …). The card renders the
  //     accessible fallback. Stale responses are discarded via
  //     `thumbnailToken` so a late rejection cannot clobber the
  //     current card's `"loaded"` state when a different entry races
  //     in front of it.
  // ---------------------------------------------------------------

  /**
   * Coarse three-state machine for the thumbnail surface. `"loading"`
   * is the canonical state for any card that owns a coherent
   * `asset_ref` while the bridge round-trip is still pending; the card
   * never falls back to "Imagen no disponible" while the load is in
   * flight.
   */
  type ThumbnailState = "loading" | "loaded" | "error";

  $: isImage = isImageEntry(entry);
  $: assetRef = hasRenderableImage(entry) ? entry.asset_ref : null;
  $: imageDimensions = imageDimensionsLabel(entry);

  const assetResolver: IconResolver = createClipboardAssetResolver();
  let thumbnailUrl: string | null = null;
  /**
   * Coarse state of the thumbnail surface. Mirrors `thumbnailUrl`
   * but kept as its own reactive variable so the template can branch
   * on `"loading"` without having to inspect `thumbnailUrl === null`,
   * which is also the terminal state for a textual entry (no
   * thumbnail to begin with).
   *
   * The state is reassigned through a helper so the three
   * transitions (loading → loaded, loading → error, entry change
   * → loading) all go through the same code path and the
   * `thumbnailToken` guard cannot be bypassed by accident.
   *
   * The initial value mirrors the entry that mounted the card so the
   * very first paint never flashes the "Imagen no disponible"
   * fallback for a persisted, renderable image. A coherent image
   * row enters the `"loading"` branch immediately; a non-image
   * row, an image row missing the renderability metadata or a row
   * without a backing asset starts in `"error"`. A subsequent
   * `syncAssetRef` round then keeps the state in lock-step with the
   * bridge round-trip.
   */
  let thumbnailState: ThumbnailState = hasRenderableImage(entry)
    ? "loading"
    : "error";
  /**
   * Mirror of `assetRef` updated synchronously so the `onDestroy`
   * cleanup can revoke the blob URL of the current entry even when the
   * component is rebuilt before Svelte flushes its reactive deps.
   */
  let lastAssetRef: string | null = null;
  /**
   * Sequence guard, mirroring `iconRefreshToken`. Bumped on every
   * `syncAssetRef` invocation; a resolution that lands after a newer
   * round has been scheduled is discarded so the card never shows
   * another entry's thumbnail.
   */
  let thumbnailToken = 0;

  /**
   * Update the coarse state without violating the
   * `thumbnailToken` invariant. The helper is the only writer for
   * `thumbnailState` so a future change cannot accidentally clear
   * the guard or skip the stale-response branch.
   */
  function commitThumbnailState(next: ThumbnailState): void {
    thumbnailState = next;
  }

  async function refreshThumbnail(ref: string | null): Promise<void> {
    const token = ++thumbnailToken;
    if (!ref) {
      if (token === thumbnailToken) {
        thumbnailUrl = null;
        commitThumbnailState("error");
      }
      return;
    }
    // Mark the round as in-flight so the template shows the loading
    // placeholder instead of the "Imagen no disponible" fallback.
    // The transition runs synchronously before the bridge await so
    // a same-tick render cannot land on a `null` URL with no state.
    if (token === thumbnailToken) {
      commitThumbnailState("loading");
    }
    const resolution = await assetResolver.resolve(ref);
    if (token !== thumbnailToken) {
      // A newer refresh landed while we awaited the bridge; drop this
      // result so the card never shows another entry's thumbnail.
      return;
    }
    if (resolution.ok && resolution.url) {
      thumbnailUrl = resolution.url;
      commitThumbnailState("loaded");
    } else {
      thumbnailUrl = null;
      commitThumbnailState("error");
    }
  }

  function syncAssetRef(ref: string | null): void {
    if (ref === lastAssetRef) return;
    const previous = lastAssetRef;
    lastAssetRef = ref;
    if (previous) {
      // Reclaim the previous entry's blob URL before minting a new one.
      assetResolver.releaseFor(previous);
    }
    void refreshThumbnail(ref);
  }

  $: syncAssetRef(assetRef);

  /**
   * Drop a thumbnail the webview could not decode. The handler MUST
   * not clear `thumbnailState` to `"error"` when the card is mid-load
   * for a different entry — that race is what the regression
   * surfaced when the rail was rebuilt before a stale load finished.
   * The token guard prevents the late error from clobbering the
   * state the current card already committed.
   */
  function onThumbnailError(): void {
    if (!thumbnailUrl) return;
    if (lastAssetRef) {
      assetResolver.releaseFor(lastAssetRef);
    }
    thumbnailUrl = null;
    commitThumbnailState("error");
  }

  // ---------------------------------------------------------------
  // Rich-text metadata.
  //
  // The card no longer renders the rich preview — every card shows
  // the canonical plain text the same way, regardless of whether
  // the entry carries rich metadata. The rich metadata is still
  // surfaced here because the ellipsis menu gates the rich paste
  // action on it. The card never opens a blob URL or an iframe for
  // a rich row, so the rich preview bridge is dead code in the card
  // and only the menu's "Paste de texto enriquecido" path reaches
  // the backend's rich bytes.
  // ---------------------------------------------------------------

  $: isRich = hasRenderableRichText(entry);
  /**
   * Whether the card exposes the `Editar captura` action. The
   * predicate mirrors the backend guard so a stale frontend
   * cannot surface the action for an image or rich-text entry —
   * the storage layer will refuse the call anyway, but keeping
   * the menu honest avoids a misleading affordance.
   */
  $: canEditText = isEditableTextEntry(entry);

  // ---------------------------------------------------------------
  // Paste menu actions.
  //
  // The card exposes two paste actions in the ellipsis menu:
  //
  //   - "Paste de texto enriquecido": forwards `mode = "rich"`. The
  //     backend writes the available HTML / RTF representations and
  //     falls back to plain text with an explicit `pasted_plain_fallback`
  //     outcome when the host cannot publish rich text.
  //   - "Paste de texto plano": forwards `mode = "plain"`. The
  //     backend always writes only the canonical plain text after
  //     clearing every residual rich flavour.
  //
  // The rich action is disabled for entries without rich metadata; the
  // plain action is always enabled for entries that survived dedupe.
  // Both actions close the menu once, set a busy state to prevent a
  // double-invocation, and surface the typed outcome through the
  // `paste-error` channel without ever leaking rich bytes, hashes or
  // asset references.
  // ---------------------------------------------------------------

  type PasteErrorCopy = {
    title: string;
    body: string;
  };

  let pasteBusy = false;
  let pasteError: PasteErrorCopy | null = null;

  $: richPasteEnabled = isRich;
  /**
   * Paste actions the ellipsis menu renders for the current entry.
   *
   * The list is metadata-only: an image entry yields a single
   * "Paste" action whose `mode` is `null` (the Rust paste service
   * ignores the mode for image rows and runs the bitmap write), a
   * rich-text entry yields the rich + plain pair, and a plain-text
   * entry yields the same pair with the rich action visibly
   * disabled. The menu renders each item from this list so the
   * regression documented in `clipboard-history-cards` spec cannot
   * drift back in through a hard-coded button.
   */
  $: pasteMenuActions = pasteMenuActionsFor(entry, displayTitle, {
    richPasteEnabled,
    pasteBusy,
  });

  function closeMenuAfterAction(): void {
    // The rail owns the canonical menu state (`openCardId`). The
    // card just emits the toggle event so the rail can collapse the
    // active menu without a second source of truth racing the
    // popover.
    if (!menuOpen) return;
    dispatch("menu-toggle", { id: entry.id, open: false });
  }

  /**
   * Forward a preview request to the parent (`App.svelte` /
   * `HistoryCardRail.svelte`) so the Desktop opens exactly one
   * shared overlay. The helper closes the card menu first so the
   * user sees the menu dismiss exactly once; the event is the
   * single switch so pin / paste / delete / drag-and-drop /
   * title editing cannot reach the preview branch.
   */
  function requestPreview(): void {
    closeMenuAfterAction();
    dispatch("preview-request", { id: entry.id });
  }

  async function runPaste(mode: "plain" | "rich" | null): Promise<void> {
    if (pasteBusy) return;
    pasteBusy = true;
    pasteError = null;
    try {
      const response = await pasteEntryCommand({ id: entry.id, mode });
      closeMenuAfterAction();
      if (response.kind === "failed") {
        pasteError = {
          title: "No se pudo pegar la entrada",
          body: describePasteFailure(response),
        };
      } else if (response.kind === "capability_unavailable") {
        pasteError = {
          title: "Función no disponible",
          body: describeCapabilityFailure(response),
        };
      }
    } catch (error) {
      closeMenuAfterAction();
      pasteError = {
        title: "No se pudo pegar la entrada",
        body: error instanceof Error ? error.message : String(error),
      };
    } finally {
      pasteBusy = false;
    }
  }

  function describePasteFailure(response: {
    error_kind: string | null;
    message: string | null;
  }): string {
    switch (response.error_kind) {
      case "asset_read":
      case "asset_decode":
      case "asset_missing":
        return "No se pudo leer el contenido almacenado.";
      case "clipboard":
      case "clipboard_rich":
        return "El portapapeles rechazó la escritura.";
      case "paste":
        return "El pegado sintético no se pudo iniciar.";
      case "repository":
        return "La base de datos local no respondió.";
      case "not_found":
        return "La entrada ya no está disponible.";
      default:
        return response.message ?? "Inténtalo de nuevo.";
    }
  }

  function describeCapabilityFailure(response: {
    capability: string | null;
  }): string {
    switch (response.capability) {
      case "clipboard_write_rich_text":
        return "Esta sesión no puede escribir texto enriquecido.";
      case "clipboard_write":
      case "clipboard_write_image":
        return "Esta sesión no puede escribir en el portapapeles.";
      case "synthetic_paste":
        return "El pegado sintético no está disponible.";
      default:
        return "La plataforma no soporta esta acción.";
    }
  }

  function dismissPasteError(): void {
    pasteError = null;
  }

  function toggleMenu(): void {
    // The rail owns the canonical menu state. The card flips the
    // locally-rendered popover visibility through the toggle event
    // and waits for the next `menuOpen` prop to land before the
    // popover actually paints.
    if (menuOpen) {
      dispatch("menu-toggle", { id: entry.id, open: false });
      return;
    }
    titleError = null;
    dispatch("menu-toggle", { id: entry.id, open: true });
  }

  function startEditTitle(): void {
    titleDraft = entry.title ?? "";
    titleError = null;
    editingTitle = true;
    closeMenuAfterAction();
    queueMicrotask(() => {
      titleInputEl?.focus();
      titleInputEl?.select();
    });
  }

  function cancelEditTitle(): void {
    editingTitle = false;
    titleDraft = "";
    titleError = null;
  }

  function onTitleKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      event.preventDefault();
      void confirmEditTitle();
    } else if (event.key === "Escape") {
      event.preventDefault();
      cancelEditTitle();
    }
  }

  function onTitleContainerKeydown(event: KeyboardEvent): void {
    if (editingTitle) return;
    if (event.key === "F2" || event.key === "Enter") {
      event.preventDefault();
      startEditTitle();
    }
  }

  async function confirmEditTitle(): Promise<void> {
    const result = validateTitle(titleDraft);
    if (!result.ok) {
      titleError =
        result.reason === "too_long"
          ? "El título supera el máximo permitido."
          : "Título inválido.";
      return;
    }
    titleBusy = true;
    titleError = null;
    try {
      const response = await setEntryTitleCommand({
        id: entry.id,
        title: result.title,
      });
      if (response.kind === "updated") {
        localTitle = response.entry.title;
        entry = response.entry;
        editingTitle = false;
        titleDraft = "";
        onAfterMutation(entry);
      } else {
        // not_found: entry disappeared; bail out gracefully.
        editingTitle = false;
        titleDraft = "";
      }
    } catch (error) {
      titleError =
        error instanceof Error ? error.message : String(error);
    } finally {
      titleBusy = false;
    }
  }

  async function restoreDefaultTitle(): Promise<void> {
    titleBusy = true;
    titleError = null;
    try {
      const response = await setEntryTitleCommand({
        id: entry.id,
        title: null,
      });
      if (response.kind === "updated") {
        localTitle = null;
        entry = response.entry;
        onAfterMutation(entry);
      }
    } catch (error) {
      titleError = error instanceof Error ? error.message : String(error);
    } finally {
      titleBusy = false;
    }
    closeMenuAfterAction();
  }

  function handlePinClick(): void {
    onTogglePin(entry);
  }

  function handleDeleteClick(): void {
    onRequestDelete(entry);
    closeMenuAfterAction();
  }

  // ---------------------------------------------------------------
  // Tag / collection assignment flow.
  //
  // The card surfaces two accessible selectors through the
  // ellipsis menu: "Agregar tag" / "Editar tags" and "Agregar a
  // colección" / "Editar colecciones". Both selectors live in
  // dedicated modal components and persist their changes atomically
  // through the callbacks the parent supplies, so the card stays
  // free of clipboard payload, hashes and paths. The
  // "Quitar de esta colección" entry is only rendered when the card
  // is being viewed inside a non-`Historial` collection; the
  // `Historial` system collection falls back to the destructive
  // global delete flow the rest of the card already exposes.
  // ---------------------------------------------------------------

  $: assignedTagIds = assignedTags.map((tag) => tag.id);
  $: assignedCollectionIds = assignedCollections.map((c) => c.id);
  /**
   * User-collection chips the card renders inline below the tag
   * row. The protected `Historial` system collection is excluded
   * from this list so the inline chips never duplicate the
   * sidebar's `Historial` row; the membership modal surfaces the
   * complete set, including `Historial`, so the user can still see
   * every assignment in one place.
   */
  $: userCollections = assignedCollections.filter(
    (c) => c.kind !== "system",
  );
  /**
   * Whether the card has at least one inline-eligible chip. When
   * the entry only belongs to `Historial` the card MUST NOT
   * render the collection row or the overflow icon; the predicate
   * drives both branches so the empty-state stays consistent with
   * the spec.
   */
  $: hasInlineCollections = userCollections.length > 0;
  $: activeCollectionContext = (() => {
    if (activeCollectionId === null) return null;
    if (allCollections.length === 0) return null;
    const target = allCollections.find((c) => c.id === activeCollectionId);
    if (!target || target.kind === "system") return null;
    return target;
  })();

  let tagSelectorOpen = false;
  let collectionSelectorOpen = false;
  /**
   * Membership-modal state the overflow icon opens. The button is
   * the only entry point; the modal closes on Escape, backdrop
   * click, the close button or a programmatic `close` event the
   * shared `Modal` shell dispatches. The trigger element is
   * captured through `bind:this` and forwarded as `returnFocusTo`
   * so the focus returns to the icon when the dialog dismisses.
   */
  let membershipModalOpen = false;
  /**
   * Whether the inline text editor modal is open. The flag lives
   * on the card so the menu trigger can flip it through a single
   * `dispatch` path; the rail and the App stay free of per-card
   * editor state.
   */
  let textEditorOpen = false;
  /**
   * Focus destination for the currently open text editor. Normal
   * user-initiated closes return to this card; a rail-forced close while
   * switching targets clears it first so the old card cannot steal focus
   * from the new target.
   */
  let textEditorReturnFocusTarget: HTMLElement | null = null;
  /**
   * DOM handle on the inline collection chip row. The
   * `ResizeObserver` the card installs during `onMount` reads
   * `clientWidth` against this element so the overflow icon only
   * appears when the chips genuinely cannot be rendered inside the
   * card width. The visible row alone is not a sufficient
   * measurement source: once the row paints the trimmed subset, its
   * `scrollWidth` collapses to the trimmed width and the predicate
   * would never converge on the original overflow decision. The
   * hidden strip below is the authoritative geometry source.
   */
  let collectionChipsEl: HTMLDivElement | null = null;
  /**
   * DOM handle on the hidden measurement strip the card mounts
   * once per hydration. The strip is a *sibling* of the painted
   * `.collection-chips` row, not a descendant, so `bind:this`
   * captures it independently and the measurement pass queries
   * `[data-collection-chip-measure-item]` /
   * `[data-collection-overflow-measure-item]` against this
   * element directly. The previous implementation queried
   * `collectionChipsEl.querySelector("[data-collection-chip-measure]")`,
   * but the only element that carried that attribute was the
   * visible row itself, so the lookup collapsed to the visible
   * row and never reached the actual measurement spans.
   */
  let collectionMeasureStripEl: HTMLDivElement | null = null;
  /**
   * DOM handle on the overflow button itself. Forwarded to the
   * membership modal as `returnFocusTo` so the modal restores
   * focus on close. The card also reads it inside
   * `isInteractiveTarget` so the click never reaches the card
   * surface selection handler.
   */
  let collectionOverflowEl: HTMLButtonElement | null = null;
  /**
   * Cached per-chip intrinsic widths measured from a hidden
   * measurement strip the card mounts once during `onMount`.
   * Storing the values reactively lets Svelte recompute the
   * visible subset whenever the membership set changes; the strip
   * itself is rendered off-screen so it never affects the card
   * layout.
   */
  let collectionChipWidths: number[] = [];
  /**
   * Whether the inline chip row currently overflows its container.
   * `false` means every chip fits and the card MUST NOT render the
   * overflow button. `true` means at least one chip was clipped
   * and the card renders the visible subset plus the button.
   */
  let collectionOverflow = false;
  /**
   * Visible subset of `userCollections` the chip row paints. When
   * `collectionOverflow` is `false` the slice mirrors the full set
   * so the user sees every membership; when it is `true` the
   * slice is trimmed from the end so each visible chip plus the
   * overflow chip fits inside the card. The helper receives the
   * measured intrinsic width of the overflow chip so the last
   * visible chip never overlaps the `+N` indicator.
   */
  $: visibleUserCollections = computeVisibleCollections(
    userCollections,
    collectionChipWidths,
    collectionRowWidth,
    collectionOverflow,
    collectionOverflowChipWidth,
  );
  /**
   * Number of user collections the overflow chip hides at the
   * current visible subset. The count drives the chip text and
   * the membership modal description so the inline label and the
   * modal agree on the same number.
   */
  $: collectionOverflowCount = computeOverflowCount(
    userCollections,
    visibleUserCollections,
  );
  /**
   * Text rendered inside the overflow chip. The leading `+`
   * matches the documented `+N` indicator so the affordance reads
   * exactly the same as the `tag-chip.more` counter the card
   * already exposes for tags.
   */
  $: collectionOverflowLabel = `+${collectionOverflowCount}`;
  /**
   * Cached `clientWidth` of the chip row. Updated by the
   * `ResizeObserver` so the visible-subset recomputation has a
   * stable container width to size against — the row itself is
   * wrapped in `overflow: hidden`, so `scrollWidth` is the
   * intrinsic width of the content while `clientWidth` is the
   * available painting area.
   */
  let collectionRowWidth = 0;
  /**
   * Cached intrinsic width of the `+N` overflow chip. The value
   * comes from the hidden measurement strip the card mounts
   * during `onMount`, which renders the chip with the worst-case
   * text (`+{userCollections.length}`) so the reservation tracks
   * every realistic scenario. The width feeds `computeVisibleCollections`
   * directly so the responsive subset accounts for the actual
   * chip geometry instead of a fixed reservation.
   */
  let collectionOverflowChipWidth = 0;
  let orgBusy = false;
  let orgError: string | null = null;

  /**
   * Read each chip's intrinsic width from the hidden measurement
   * strip the card mounts during `onMount`. The strip is the
   * authoritative geometry source: it renders the chips with the
   * same CSS class so font, padding, border and gap match the
   * visible row, and only the position differs (`position:
   * absolute; visibility: hidden; pointer-events: none`).
   *
   * The strip is a sibling of the visible row, not a descendant,
   * so the lookup runs against `collectionMeasureStripEl`
   * directly — `collectionChipsEl.querySelector(...)` would only
   * ever find descendants of the painted row and would miss the
   * measurement spans. The previous implementation relied on that
   * wrong selector and silently produced an empty `chipWidths`
   * array, which is what hid the `+N` chip in the manual tests.
   */
  function readCollectionChipWidths(): void {
    if (!collectionMeasureStripEl) {
      collectionChipWidths = [];
      return;
    }
    const items = collectionMeasureStripEl.querySelectorAll<HTMLElement>(
      "[data-collection-chip-measure-item]",
    );
    collectionChipWidths = Array.from(items).map(
      (item) => item.getBoundingClientRect().width,
    );
    const overflowItem = collectionMeasureStripEl.querySelector<HTMLElement>(
      "[data-collection-overflow-measure-item]",
    );
    collectionOverflowChipWidth = overflowItem?.getBoundingClientRect().width ?? 0;
  }

  /**
   * Read the chip row's `clientWidth` and decide whether the row
   * currently overflows. The predicate compares the *intrinsic*
   * total of the full user-collection set against the available
   * card width — never the `scrollWidth` of a row that has
   * already been trimmed, because a trimmed subset cannot reveal
   * the original overflow decision.
   *
   * The intrinsic total is computed by `intrinsicCollectionRowWidth`
   * from the per-chip widths measured by `readCollectionChipWidths`
   * plus the reserved width of the overflow chip itself. If the
   * measurement strip has not yet reported a width (a race the
   * first paint can land in under WebKit/Tauri) the helper falls
   * back to `COLLECTION_OVERFLOW_BUTTON_PX` so the predicate stays
   * deterministic and the chip row still converges on the very
   * first render.
   */
  function detectCollectionOverflow(): void {
    if (!collectionChipsEl) {
      collectionRowWidth = 0;
      collectionOverflow = false;
      return;
    }
    collectionRowWidth = collectionChipsEl.clientWidth;
    if (!entryOrganizationLoaded || userCollections.length === 0) {
      collectionOverflow = false;
      return;
    }
    // When the measurement strip has not yet produced a width for
    // every chip (e.g. the first render before `ResizeObserver`
    // fires) the helper cannot compute a reliable intrinsic total
    // and the predicate must stay neutral. The next measurement
    // pass after the strip reports its first width resolves the
    // flag and the reactive block converges.
    if (collectionChipWidths.length !== userCollections.length) {
      collectionOverflow = false;
      return;
    }
    const intrinsicTotal = intrinsicCollectionRowWidth(
      collectionChipWidths,
      collectionOverflowChipWidth,
    );
    collectionOverflow =
      intrinsicTotal > collectionRowWidth + COLLECTION_OVERFLOW_TOLERANCE_PX;
  }

  /**
   * Recompute widths and overflow in sequence. The width pass MUST
   * run first so the visible-subset computation has the freshest
   * per-chip widths available when `visibleUserCollections` is
   * derived from `userCollections`, `collectionChipWidths` and
   * `collectionRowWidth`.
   */
  function recomputeCollectionLayout(): void {
    readCollectionChipWidths();
    detectCollectionOverflow();
  }

  /**
   * `ResizeObserver` the card installs while it is mounted. The
   * observer runs whenever the chip row resizes (a viewport
   * change, a font swap, a card-size token override) and the
   * `recomputeCollectionLayout` helper reacts so the visible
   * subset and the overflow flag stay in lock-step with the
   * current layout.
   *
   * The observer is detached on `onDestroy` and never reaches
   * into the singleton pointer drag controller or the document
   * event surface, so it cannot race the drag flow.
   */
  let collectionResizeObserver: ResizeObserver | null = null;

  function ensureCollectionResizeObserver(): void {
    if (typeof ResizeObserver === "undefined") return;
    if (collectionResizeObserver) return;
    collectionResizeObserver = new ResizeObserver(() => {
      recomputeCollectionLayout();
    });
  }

  function attachCollectionResizeObserver(): void {
    if (!collectionChipsEl) return;
    ensureCollectionResizeObserver();
    collectionResizeObserver?.observe(collectionChipsEl);
  }

  function detachCollectionResizeObserver(): void {
    if (!collectionResizeObserver) return;
    collectionResizeObserver.disconnect();
    collectionResizeObserver = null;
  }

  /**
   * Open the membership modal from the overflow button. The helper
   * is the single click target so future changes (a long-press
   * gesture, a programmatic open) cannot race the focus
   * bookkeeping. The modal stores the trigger through `bind:this`
   * above and returns focus to it on close.
   */
  function openMembershipModal(): void {
    membershipModalOpen = true;
  }

  function openTagSelector(): void {
    tagSelectorOpen = true;
    collectionSelectorOpen = false;
    orgError = null;
    closeMenuAfterAction();
  }

  function openCollectionSelector(): void {
    collectionSelectorOpen = true;
    tagSelectorOpen = false;
    orgError = null;
    closeMenuAfterAction();
  }

  /**
   * Open the text editor modal. The guard rejects ineligible
   * entries so a stale foreground cannot open the editor for an
   * image or a rich-text row even if the menu somehow exposes the
   * action; the storage layer is the second line of defence.
   */
  function openTextEditor(): void {
    if (!canEditText) return;
    textEditorReturnFocusTarget = cardArticleEl;
    textEditorOpen = true;
    closeMenuAfterAction();
  }

  /**
   * Close the editor after a user action and restore the card's keyboard
   * state. The explicit microtask runs after Svelte flushes the modal's
   * `open=false` update, so a stale Modal fallback cannot move focus to a
   * different card. Selecting the same entry gives the card the blue
   * navigation outline used by ArrowLeft / ArrowRight.
   */
  function closeTextEditor(): void {
    textEditorReturnFocusTarget = cardArticleEl;
    textEditorOpen = false;
    dispatchSelect(entry.id);
    queueMicrotask(() => {
      if (cardArticleEl && document.contains(cardArticleEl)) {
        cardArticleEl.focus();
      }
    });
  }

  /**
   * `Cmd/Ctrl+E` shortcut that opens the same modal as the menu
   * item.
   *
   * The matcher accepts the platform-specific binding the desktop
   * shell resolves through the diagnostics payload
   * (`Cmd+E` on macOS, `Ctrl+E` everywhere else). The card keeps a
   * per-card matcher (`matchesEditTextShortcut`) for compatibility
   * with the document-level listener the desktop installs in
   * capture phase: the document-level listener handles the shortcut
   * first; this matcher is the safety net the design allows so a
   * future caller can still trigger the modal from the card's own
   * `<article>` keyboard handler. The matcher delegates to the
   * platform-aware helper `matchesEditTextShortcutHelper` so the
   * shortcut, the visible label and the `aria-keyshortcuts` value
   * cannot drift apart.
   *
   * The shortcut MUST:
   *
   *   - be a no-op when the card is not eligible (`canEditText`)
   *     so an image or rich-text row never opens the editor;
   *   - be a no-op when the keyboard event target is itself an
   *     interactive surface (input, textarea, select, button,
   *     contenteditable, the ellipsis menu, the editor modal or the
   *     `+N` overflow chip) so the user can keep typing inside
   *     those controls without the card intercepting the key;
   *   - never reach into the singleton pointer drag controller
   *     or document / window: the matcher is wired on the card's
   *     own `<article>` keyboard handler so the existing
   *     `onCardKeydown` entry point already runs first and the
   *     `isInteractiveTarget` guard already excludes every drag
   *     surface;
   *   - never start a drag, flip the selection or change the
   *     focus while the shortcut opens the editor (the helper
   *     delegates to `openTextEditor`, which is the single menu
   *     entry point).
   *
   * `preventDefault` runs only when the shortcut is accepted so
   * typing `e` inside the search box (or anywhere else outside the
   * card) keeps its native behaviour.
   */
  const EDIT_TEXT_SHORTCUT_TESTID = "history-card-edit-text-shortcut";
  /**
   * Whether the focused card has a parent dialog/modal so the
   * matcher can ignore the shortcut while the user is interacting
   * with the editor itself. `closest('[role="dialog"]')` also
   * catches future modals without needing per-modal wiring.
   */
  function isInsideModalTarget(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false;
    if (target.closest('[role="dialog"]')) return true;
    return false;
  }
  /**
   * Resolve the platform the card's per-card matcher should use.
   * The card cannot read the diagnostics payload directly so it
   * delegates to the `editTextShortcutPlatform` helper, which
   * routes the same string the desktop shell inspects through
   * `platformFromDiagnostics`. The matcher, the visible label and
   * the `aria-keyshortcuts` attribute all consult the same value so
   * the menu hint cannot drift away from the binding the listener
   * accepts.
   */
  function editTextCardPlatform(): ReturnType<typeof editTextShortcutPlatform> {
    return editTextShortcutPlatform(null);
  }
  $: editTextShortcutCardPlatform = editTextCardPlatform();
  $: editTextShortcutLabelText = editTextShortcutLabel(
    editTextShortcutCardPlatform,
  );
  $: editTextShortcutKeyAttributeText = editTextShortcutKeyAttribute(
    editTextShortcutCardPlatform,
  );
  $: editTextShortcutAccessibleText = editTextShortcutAccessibleLabel(
    editTextShortcutCardPlatform,
  );
  function matchesEditTextShortcut(event: KeyboardEvent): boolean {
    return matchesEditTextShortcutHelper(event, editTextShortcutCardPlatform);
  }

  async function handleTagsSave(
    event: CustomEvent<{ tagIds: number[] }>,
  ): Promise<void> {
    // The modal dispatches `save` with the full set of selected ids
    // (the union of pre-existing and freshly created). The card
    // awaits the parent's persistence promise so the modal closes
    // only after the SQLite transaction commits; a backend failure
    // keeps the modal open with the error banner visible instead of
    // swallowing the rejection.
    await persistTagSelection(event.detail.tagIds);
    tagSelectorOpen = false;
  }

  async function handleCollectionsSave(
    event: CustomEvent<{ collectionIds: number[] }>,
  ): Promise<void> {
    await persistCollectionSelection(event.detail.collectionIds);
    collectionSelectorOpen = false;
  }

  function handleCancelSelector(): void {
    tagSelectorOpen = false;
    collectionSelectorOpen = false;
  }

  async function persistTagSelection(
    tagIds: number[],
  ): Promise<void> {
    if (orgBusy) return;
    orgBusy = true;
    orgError = null;
    try {
      // Awaits the parent's Promise so a backend failure propagates
      // as a rejected promise; the caller (`handleTagsSave`) closes
      // the modal only on resolution. A second click is a no-op
      // because `orgBusy` blocks the early return.
      await onAssignTags(entry, tagIds);
    } catch (error) {
      orgError = error instanceof Error ? error.message : String(error);
    } finally {
      orgBusy = false;
    }
  }

  async function persistCollectionSelection(
    collectionIds: number[],
  ): Promise<void> {
    if (orgBusy) return;
    orgBusy = true;
    orgError = null;
    try {
      await onAssignCollections(entry, collectionIds);
    } catch (error) {
      orgError = error instanceof Error ? error.message : String(error);
    } finally {
      orgBusy = false;
    }
  }

  async function removeFromActiveCollection(): Promise<void> {
    if (activeCollectionContext === null) return;
    if (orgBusy) return;
    orgBusy = true;
    try {
      await onRemoveFromCollection(entry, activeCollectionContext.id);
      closeMenuAfterAction();
    } catch (error) {
      orgError = error instanceof Error ? error.message : String(error);
    } finally {
      orgBusy = false;
    }
  }

  function onMenuKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      // Escape closes the menu regardless of where the focus sits
      // inside the popover. `preventDefault` and `stopPropagation`
      // are only called once we know the menu is open, so a typing
      // surface outside the popover keeps its default Escape
      // behaviour. The rail's document-level listener mirrors this
      // branch so closing the menu from outside the popover and
      // from inside the popover produce the same observable state.
      event.preventDefault();
      closeMenuAfterAction();
    }
  }

  /**
   * Reference to the ellipsis trigger button. The popover reads
   * its `getBoundingClientRect()` to anchor itself inside the
   * viewport without depending on the card's clipping context.
   * `bind:this` keeps the reference in lock-step with the rendered
   * DOM node so a remount can re-anchor the popover the same tick
   * the trigger paints.
   */
  let menuTriggerEl: HTMLButtonElement | null = null;

  /**
   * Reference to the popover element itself, captured the moment
   * Svelte mounts it. The handle lets `recomputeMenuPosition`
   * measure the popover's real height after the first render so
   * the bottom edge can be pinned to `trigger.top` even when the
   * popover's content height is smaller than the clamped
   * `max-height` the helper initially produced. Without this
   * re-pass the menu would visibly float above the trigger when
   * the viewport is tall.
   */
  let menuEl: HTMLDivElement | null = null;

  /** Live `position: fixed` style for the menu popover. The helper
   * `cardMenuStyle` formats the `CardMenuPosition` so the menu
   * template stays a thin renderer. */
  let menuPositionStyle = "";
  let menuFlippedAbove = false;
  let menuScrollable = false;

  /**
   * Compute the popover's viewport-aware rectangle. The helper is
   * called when the menu opens, when the window resizes and when
   * the scroll surface moves; the rail owns the outside-click
   * guard and the Escape cleanup.
   *
   * `getBoundingClientRect` and `innerWidth` / `innerHeight` are
   * the only DOM hooks the helper consults; the position math lives
   * in `lib/cardMenuPositioning.ts` so the regression suite can
   * exercise it without mounting Svelte.
   *
   * The first pass computes a viewport-aware rectangle from the
   * trigger's bounding box; the second pass re-anchors the popover
   * against its real rendered height so `popover.bottom === trigger.top`
   * (when the menu flipped above) holds even when the popover is
   * shorter than the available height. `requestAnimationFrame` is
   * used to guarantee the layout pass has run before the second
   * measurement; the rail's window listener re-runs the helper
   * whenever the viewport or scroll surface changes.
   */
  function recomputeMenuPosition(): void {
    if (!menuOpen) {
      menuPositionStyle = "";
      menuFlippedAbove = false;
      menuScrollable = false;
      return;
    }
    if (
      !menuTriggerEl ||
      typeof menuTriggerEl.getBoundingClientRect !== "function"
    ) {
      menuPositionStyle = "";
      return;
    }
    const rect = menuTriggerEl.getBoundingClientRect();
    const viewport = {
      width: typeof window === "undefined" ? 1024 : window.innerWidth,
      height: typeof window === "undefined" ? 720 : window.innerHeight,
    };
const position: CardMenuPosition = computeCardMenuPosition(rect, viewport);
    menuPositionStyle = cardMenuStyle(position);
    menuFlippedAbove = position.flippedAbove;
    menuScrollable = position.scrollable;
    // Second pass: re-anchor using the popover's real height. The
    // popover is `position: fixed` and shrinks to its content, so
    // its measured height may be smaller than `position.maxHeight`;
    // the helper computes the popover's bottom from
    // `position.maxHeight`, but the visible bottom lives at
    // `position.top + menuEl.getBoundingClientRect().height`. The
    // re-anchoring helper keeps the viewport clamps and the
    // `CARD_MENU_TRIGGER_GAP` invariant intact, so the popover's
    // actual bottom edge lands on `trigger.top` when the menu
    // flipped above and on `trigger.bottom` when it dropped below.
    // `requestAnimationFrame` waits for the layout pass to run so
    // `getBoundingClientRect` returns the rendered geometry instead
    // of a zero height for an unmounted popover.
    requestAnimationFrame(() => {
      if (!menuEl || typeof menuEl.getBoundingClientRect !== "function") {
        return;
      }
      const popoverHeight = menuEl.getBoundingClientRect().height;
      if (popoverHeight <= 0) return;
      const corrected = recomputeCardMenuPositionForActualHeight(
        position,
        rect,
        viewport,
        popoverHeight,
      );
      if (
        Math.abs(corrected.top - position.top) > 0.5 ||
        Math.abs(corrected.maxHeight - position.maxHeight) > 0.5
      ) {
        menuPositionStyle = cardMenuStyle(corrected);
      }
    });
  }

  /** Window-level listeners that keep the popover anchored when
   * the rail scrolls or the viewport resizes. The listeners are
   * added lazily when the menu opens and removed when it closes
   * so the rail cannot leak handlers between menu activations. */
  let menuWindowListeners: (() => void) | null = null;

  function attachMenuWindowListeners(): void {
    if (typeof window === "undefined") return;
    if (menuWindowListeners !== null) return;
    const handler = () => recomputeMenuPosition();
    window.addEventListener("resize", handler);
    window.addEventListener("scroll", handler, true);
    menuWindowListeners = () => {
      window.removeEventListener("resize", handler);
      window.removeEventListener("scroll", handler, true);
    };
  }

  function detachMenuWindowListeners(): void {
    if (menuWindowListeners === null) return;
    menuWindowListeners();
    menuWindowListeners = null;
  }

  // Re-anchor the popover on every viewport / scroll surface change
  // while it is open. The reactive block re-derives the style as
  // soon as `menuOpen` flips so the `{#if menuOpen}` block mounts the
  // popover with the correct inline `style` from its very first
  // paint — there is no flash at `(0, 0)` while the microtask waits
  // for the next tick. The post-render pass (the `requestAnimationFrame`
  // inside `recomputeMenuPosition`) still runs on the next frame to
  // re-anchor against the real rendered height, but the menu is
  // already visible at the right place when the very first paint
  // lands.
  $: if (menuOpen) {
    attachMenuWindowListeners();
    recomputeMenuPosition();
  } else {
    detachMenuWindowListeners();
    menuPositionStyle = "";
    menuFlippedAbove = false;
    menuScrollable = false;
  }

  /**
   * Keyboard affordance for the focused card. The matcher accepts
   * the platform-aware preview shortcut (`Cmd+Enter` on macOS,
   * `Ctrl+Enter` elsewhere) and forwards a `preview-request` to
   * the parent so the rail opens the shared overlay. Inputs,
   * textareas, the title editor, the menu and other interactive
   * controls must intercept the event before it reaches the card
   * (the `drag-and-drop` handler and the menu / pin buttons do).
   *
   * The matcher is imported lazily through `onMount` so the
   * card remains testable in a non-Tauri context. The function
   * stays a no-op when the platform is unresolved; the parent
   * owns the dispatcher.
   */
  let previewMatcher:
    | ((event: KeyboardEvent, platform: PreviewShortcutPlatform) => boolean)
    | null = null;
  let previewPlatform: PreviewShortcutPlatform = "other";

  onMount(() => {
    void import("./lib/searchShortcut.ts")
      .then(({ searchShortcutPlatform: derivePlatform }) => {
        previewPlatform = derivePlatform(null);
      })
      .catch(() => {
        previewPlatform = "other";
      });
    void import("./lib/clipboardPreview.ts").then(({ matchesPreviewShortcut }) => {
      previewMatcher = matchesPreviewShortcut;
    });
    // Recompute the inline chip widths and the overflow flag once
    // the DOM has settled, then start the ResizeObserver so the
    // card stays responsive to layout changes (window resize, font
    // swap, accessibility-driven scale changes). The observer is
    // detached on `onDestroy` so the card cannot leak a handler
    // across remounts.
    recomputeCollectionLayout();
    attachCollectionResizeObserver();
  });

  function isInteractiveTarget(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false;
    const tag = target.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
    if (target.isContentEditable) return true;
    if (tag === "BUTTON") return true;
    if (target.closest("[role='menu']")) return true;
    if (target.closest("[role='menuitem']")) return true;
    if (target.closest(".menu")) return true;
    if (target.closest(".title-input")) return true;
    if (target.closest("[data-testid='history-card-collections-overflow']")) {
      return true;
    }
    if (target.closest(".collection-chips")) return true;
    return false;
  }

  function onCardKeydown(event: KeyboardEvent): void {
    if (isInteractiveTarget(event.target)) return;
    if (matchesEditTextShortcut(event)) {
      // The shortcut only opens the editor for an eligible entry,
      // when the focus is on the card surface itself (not on an
      // interactive child, the menu, the editor modal or another
      // dialog). `preventDefault` runs only inside this branch so
      // typing `e` outside the card keeps its native behaviour and
      // the shortcut never races the existing `Escape` /
      // `Cmd/Ctrl+Enter` flows.
      if (!canEditText) return;
      if (isInsideModalTarget(event.target)) return;
      event.preventDefault();
      openTextEditor();
      return;
    }
    if (!previewMatcher) return;
    if (event.key === "Escape") {
      // Escape clears the selection only when the rail has focus
      // (the document-level listener the rail installs also routes
      // Escape to the same callback). The shortcut MUST NOT swallow
      // Escape when the user is editing the title, browsing the menu
      // or typing inside an input — `isInteractiveTarget` already
      // covers those branches.
      if (selected) {
        event.preventDefault();
        dispatchSelect(null);
      }
      return;
    }
    if (!previewMatcher(event, previewPlatform)) return;
    // The preview shortcut only fires for the currently selected
    // card. A card that is not selected ignores the shortcut so
    // `Cmd/Ctrl+Enter` cannot silently open a preview for an entry
    // the user did not pick.
    if (!selected) return;
    event.preventDefault();
    requestPreview();
  }

  /**
   * Forward the user's selection gesture to the rail owner. The
   * helper centralises the dispatch path so the click handler and
   * the `Escape` shortcut share one branch.
   */
  function dispatchSelect(next: number | null): void {
    onSelect(next);
    dispatch("select-request", { id: next });
  }

  /**
   * Click handler the card surface installs. The handler routes
   * through the same `isInteractiveTarget` guard the keyboard
   * shortcut uses so a click on a pin / menu / title / tag /
   * collection / paste / delete control keeps its existing action
   * and never accidentally flips the selection.
   *
   * Clicking the non-interactive surface of the currently selected
   * card clears the selection; clicking any other card moves the
   * selection to that card. The helper never writes the value to
   * SQLite or any backend command — the rail is the single source of
   * truth.
   */
  function onCardSurfaceClick(event: MouseEvent): void {
    if (isInteractiveTarget(event.target)) return;
    dispatchSelect(selected ? null : entry.id);
  }

  // Dragging is delegated to the singleton pointer controller installed by
  // App.svelte. Keeping the card itself non-draggable prevents the unreliable
  // HTML5 WebKit lifecycle from competing with pointer hit-testing.
  onDestroy(() => {
    if (lastIconRef) {
      iconResolver.releaseFor(lastIconRef);
    }
    // Detach the ResizeObserver the card installed during
    // `onMount`; otherwise the observer would keep firing against
    // a detached DOM node and leak the closure.
    detachCollectionResizeObserver();
    // Revoke every blob URL the card minted for its payload assets
    // (image only — rich preview is no longer rendered) so a
    // long-lived rail does not leak memory as the user scrolls.
    assetResolver.release();
  });
</script>

<article
  class="card"
  class:menu-open={menuOpen}
  class:card-selected={selected}
  data-testid="history-card"
  data-entry-id={entry.id}
  data-selected={selected ? "true" : "false"}
  aria-label={displayTitle}
  aria-selected={selected}
  draggable="false"
  tabindex={menuOpen || editingTitle ? -1 : 0}
  bind:this={cardArticleEl}
  on:click={(event) => onCardSurfaceClick(event)}
  on:keydown={(event) => onCardKeydown(event)}
>
  <header class="card-header">
    <span
      class="type"
      data-testid="history-card-type"
      data-content-type={entry.content_type}
      title={`Tipo: ${contentTypeIconLabel(entry.content_type)}`}
      aria-label={`Tipo: ${contentTypeIconLabel(entry.content_type)}`}
    >
      <svg
        aria-hidden="true"
        focusable="false"
        width="20"
        height="20"
      >
        <use href="#{contentTypeIconId(entry.content_type)}" />
      </svg>
      <span class="visually-hidden">{contentTypeIconLabel(entry.content_type)}</span>
    </span>
    <div
      class="title"
      data-testid="history-card-title"
      title={displayTitle}
      tabindex={editingTitle ? -1 : 0}
      role="button"
      aria-disabled={editingTitle}
      aria-label={
        editingTitle
          ? "Editar título"
          : `Editar título ${displayTitle} (doble clic, Enter o F2)`
      }
      on:keydown={onTitleContainerKeydown}
      on:dblclick={() => startEditTitle()}
    >
      {#if editingTitle}
        <input
          type="text"
          class="title-input"
          bind:this={titleInputEl}
          bind:value={titleDraft}
          aria-label="Editar título"
          maxlength="120"
          on:keydown={onTitleKeydown}
        />
        <button
          type="button"
          class="icon-inline title-confirm"
          on:click={() => void confirmEditTitle()}
          aria-label="Confirmar título"
          title="Confirmar (Enter)"
          data-testid="history-card-title-confirm"
          disabled={titleBusy}
        >
          <svg
            aria-hidden="true"
            focusable="false"
            width="14"
            height="14"
            viewBox="0 0 14 14"
          >
            <path
              d="M3 7.5l2.6 2.6L11 4.5"
              stroke="currentColor"
              stroke-width="1.6"
              fill="none"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </button>
        <button
          type="button"
          class="icon-inline title-cancel"
          on:click={cancelEditTitle}
          aria-label="Cancelar título"
          title="Cancelar (Escape)"
          data-testid="history-card-title-cancel"
        >
          <svg
            aria-hidden="true"
            focusable="false"
            width="14"
            height="14"
            viewBox="0 0 14 14"
          >
            <path
              d="M3 3l8 8M11 3l-8 8"
              stroke="currentColor"
              stroke-width="1.6"
              fill="none"
              stroke-linecap="round"
            />
          </svg>
        </button>
        {#if titleError}
          <span class="title-error" role="alert">{titleError}</span>
        {/if}
      {:else}
        {displayTitle}
      {/if}
    </div>
    <span
      class="source-app"
      data-testid="history-card-source-app"
      title={sourceAppLabel}
      aria-label={sourceAppLabel}
      data-imported-source-app={peerImportedSourceApp !== null ? "true" : "false"}
    >
      {#if appIconUrl}
        <img
          src={appIconUrl}
          alt=""
          class="source-app-icon"
          aria-hidden="true"
          data-testid="history-card-source-app-icon"
          on:error={() => {
            appIconUrl = null;
            if (iconRef) {
              iconResolver.releaseFor(iconRef);
            }
          }}
        />
      {:else}
        <span
          class="source-app-fallback"
          aria-hidden="true"
          data-testid="history-card-source-app-fallback"
        >
          {#if peerImportedSourceApp !== null}
            {@html IMPORTED_SOURCE_APP_FALLBACK_ICON_SVG}
          {:else}
            {@html APP_FALLBACK_ICON_SVG}
          {/if}
        </span>
      {/if}
      <span class="visually-hidden" data-testid="history-card-source-app-accessible">
        {sourceAppLabel}
      </span>
    </span>
  </header>

  <p class="metadata" data-testid="history-card-metadata">
    <span
      class="metadata-age"
      data-testid="history-card-age"
      aria-label={ageLabel.aria}
      title={ageLabel.aria}
    >
      <svg
        aria-hidden="true"
        focusable="false"
        width="12"
        height="12"
        viewBox="0 0 12 12"
      >
        <path
          d="M6 1.5a4.5 4.5 0 1 1 0 9 4.5 4.5 0 0 1 0-9Zm0 1.5a3 3 0 1 0 0 6 3 3 0 0 0 0-6Zm.75 1.6v2.05l1.6 1.6"
          stroke="currentColor"
          stroke-width="1"
          fill="none"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
      <span>{ageLabel.visual}</span>
    </span>
    <span
      class="metadata-size"
      data-testid="history-card-size"
      data-content-size={entry.content_size}
      aria-label={isImage
        ? `Tamaño del payload: ${byteSizeInfo.accessible}`
        : `Carácteres totales: ${characterCount}`}
      title={isImage ? byteSizeInfo.accessible : `Carácteres: ${characterCount}`}
    >
      {#if isImage}
        <svg
          aria-hidden="true"
          focusable="false"
          width="12"
          height="12"
          viewBox="0 0 12 12"
        >
          <rect
            x="1.5"
            y="2.5"
            width="9"
            height="7"
            rx="1.4"
            stroke="currentColor"
            stroke-width="1"
            fill="none"
          />
          <circle cx="4" cy="5" r="0.9" fill="currentColor" />
          <path
            d="M9.5 8.5 7 6.2 4.5 8.5"
            stroke="currentColor"
            stroke-width="1"
            fill="none"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
        <span>{byteSizeInfo.visual}</span>
      {:else}
        <svg
          aria-hidden="true"
          focusable="false"
          width="12"
          height="12"
          viewBox="0 0 12 12"
        >
          <path
            d="M2.5 3h7M2.5 6h7M2.5 9h4"
            stroke="currentColor"
            stroke-width="1"
            fill="none"
            stroke-linecap="round"
          />
        </svg>
        <span
          data-testid="history-card-character-count"
          data-character-count={characterCount}
        >
          {characterCount} car.
        </span>
      {/if}
    </span>
  </p>

  {#if isImage}
    <div
      class="thumbnail-area"
      data-testid="history-card-thumbnail-area"
      data-content-type="image"
      data-thumbnail-state={thumbnailState}
    >
      {#if thumbnailState === "loaded" && thumbnailUrl}
        <img
          src={thumbnailUrl}
          class="thumbnail"
          data-testid="history-card-thumbnail"
          alt={imageDimensions
            ? `Imagen capturada de ${imageDimensions} píxeles`
            : "Imagen capturada"}
          on:error={onThumbnailError}
        />
      {:else if thumbnailState === "loading"}
        <!--
          Loading placeholder. Rendered while the bridge round-trip is
          in flight so the card never flashes the "Imagen no
          disponible" fallback for a persisted asset that has not
          finished loading. The placeholder keeps the same square the
          thumbnail will fill, surfaces the known dimensions when the
          metadata round-trip succeeded and uses the Image glyph as an
          accessible hint instead of an error.
        -->
        <div
          class="thumbnail-loading"
          role="img"
          aria-label={imageDimensions
            ? `Cargando imagen (${imageDimensions})`
            : "Cargando imagen"}
          aria-live="polite"
          data-testid="history-card-thumbnail-loading"
        >
          <svg aria-hidden="true" focusable="false" width="32" height="32">
            <use href="#{contentTypeIconId('image')}" />
          </svg>
          {#if imageDimensions}
            <span class="thumbnail-loading-text">{imageDimensions}</span>
          {/if}
        </div>
      {:else}
        <!--
          Accessible fallback for a missing, invalid or undecodable
          asset. It shows the Image glyph and the known dimensions —
          never raw bytes, never the internal asset reference and never
          the empty `content` sentinel. Only rendered when the bridge
          round-trip actually failed; a pending load keeps the loading
          placeholder visible.
        -->
        <div
          class="thumbnail-fallback"
          role="img"
          aria-label={imageDimensions
            ? `Imagen no disponible (${imageDimensions})`
            : "Imagen no disponible"}
          data-testid="history-card-thumbnail-fallback"
        >
          <svg aria-hidden="true" focusable="false" width="32" height="32">
            <use href="#{contentTypeIconId('image')}" />
          </svg>
          <span class="thumbnail-fallback-text">Imagen no disponible</span>
        </div>
      {/if}
    </div>
  {:else}
    {#if shouldRenderHighlightedPreview(entry)}
      <pre
        class="preview preview-code"
        data-testid="history-card-preview"
        data-content-type={entry.content_type}
        data-code-language={entry.code_language ?? ""}
        data-preview-kind="code"
        aria-label="Contenido capturado"
      >{@html highlightedPreviewHtml}</pre>
    {:else}
      <pre
        class="preview"
        data-testid="history-card-preview"
        data-content-type={isRich ? "rich_text" : "text"}
        aria-label="Contenido capturado"
      >{previewText}</pre>
    {/if}
  {/if}

  {#if shouldShowCodeLanguageBadge(entry)}
    <p
      class="card-code-language"
      data-testid="history-card-code-language"
      data-code-language={entry.code_language}
    >
      Código · {canonicalCodeLanguageLabel(entry.code_language)}
    </p>
  {/if}

  {#if pasteError}
    <p
      class="paste-error"
      role="alert"
      data-testid="history-card-paste-error"
    >
      <strong>{pasteError.title}</strong>
      <span>{pasteError.body}</span>
      <button
        type="button"
        class="paste-error-dismiss"
        data-testid="history-card-paste-error-dismiss"
        aria-label="Cerrar aviso"
        on:click={dismissPasteError}
      >
        ×
      </button>
    </p>
  {/if}

  {#if entryOrganizationState === "pending"}
    <p
      class="tag-organization-pending"
      data-testid="history-card-tag-organization-pending"
      aria-live="polite"
    >
      Cargando tags…
    </p>
  {:else if entryOrganizationState === "error"}
    <p
      class="tag-organization-error"
      role="status"
      data-testid="history-card-tag-organization-error"
    >
      No se pudieron cargar los tags de esta entrada.
    </p>
  {:else if assignedTags.length > 0}
    <ul
      class="tag-chips"
      data-testid="history-card-tag-chips"
      aria-label="Tags de la entrada"
    >
      {#each assignedTags.slice(0, 2) as tag (tag.id)}
        <li
          class="tag-chip"
          data-testid="history-card-tag-chip"
          data-tag-id={tag.id}
        >
          {tag.display_name}
        </li>
      {/each}
      {#if assignedTags.length > 2}
        <li
          class="tag-chip more"
          data-testid="history-card-tag-more"
          aria-label={`${assignedTags.length - 2} tags adicionales`}
        >
          +{assignedTags.length - 2}
        </li>
      {/if}
    </ul>
  {/if}

  {#if entryOrganizationLoaded && hasInlineCollections}
    <!--
      Hidden measurement strip used by the responsive overflow pass.
      Renders every user collection as a chip with the same CSS
      class the visible row uses so font, padding and border match
      the painted output. The strip is positioned off-screen with
      `pointer-events: none` so it never interacts with the user,
      never participates in the singleton drag controller's hit
      tests (it carries no `data-testid="history-card"` ancestor)
      and never grows the card geometry. The trailing
      `data-collection-overflow-measure-item` span paints the
      overflow chip with the worst-case text (`+{userCollections.length}`)
      so the helper can read its real width instead of using a
      fixed reservation — this is what makes the subset
      computation converge to the canonical `+N` value.

      The strip is a *sibling* of the visible row, not a
      descendant, and the card binds it through
      `bind:this={collectionMeasureStripEl}` so the measurement
      pass queries the spans directly. The previous implementation
      tried to reach the strip from the visible row via
      `querySelector("[data-collection-chip-measure]")`, but the
      attribute only landed on the visible row itself — the
      lookup silently collapsed to the visible row and produced an
      empty widths array, which is what hid the `+N` chip in the
      manual tests.
    -->
    <div
      class="collection-chips-measure"
      data-testid="history-card-collection-chips-measure"
      data-collection-chip-strip={entry.id}
      aria-hidden="true"
      bind:this={collectionMeasureStripEl}
    >
      {#each userCollections as collection (collection.id)}
        <span
          class="collection-chip-measure"
          data-collection-chip-measure-item={collection.id}
        >
          {collection.name}
        </span>
      {/each}
      <span
        class="collection-overflow-measure"
        data-collection-overflow-measure-item={userCollections.length}
      >
        +{userCollections.length}
      </span>
    </div>
    <div
      class="collection-chips"
      data-testid="history-card-collection-chips"
      data-entry-id={entry.id}
      aria-label={assignedCollections.map((c) => c.name).join(", ")}
      bind:this={collectionChipsEl}
    >
      {#each visibleUserCollections as collection (collection.id)}
        {@const labelColor = collectionColor(collection.color_hex)}
        <span
          class="collection-chip"
          data-testid="history-card-collection-chip"
          data-collection-id={collection.id}
          data-color={collection.color_hex}
          style="color: {labelColor};"
          title={collection.name}
        >
          {collection.name}
        </span>
      {/each}
      {#if collectionOverflow && collectionOverflowCount > 0}
        <!--
          Overflow chip rendered with the documented `+N` indicator.
          The chip reuses the same visual treatment as the
          `tag-chip.more` (`+N`) affordance the card already paints
          for tags and follows the same padding / radius / border
          contract the rest of the row enforces. The chip is the
          only interactive surface in the row and is the entry point
          the `CollectionMembershipModal` reads to render the full
          membership list, including `Historial`.

          The chip's intrinsic width feeds back into the visible
          subset computation through `collectionOverflowChipWidth`
          so the `+N` value tracks the actual row width after each
          resize instead of being driven by a fixed number of
          memberships.
        -->
        <button
          type="button"
          class="tag-chip more collection-overflow"
          data-testid="history-card-collections-overflow"
          data-entry-id={entry.id}
          data-overflow-count={collectionOverflowCount}
          aria-haspopup="dialog"
          aria-label={`Ver las ${assignedCollections.length} colecciones de la captura`}
          aria-expanded={membershipModalOpen}
          title="Ver todas las colecciones"
          bind:this={collectionOverflowEl}
          on:click={openMembershipModal}
        >
          {collectionOverflowLabel}
        </button>
      {/if}
    </div>
  {/if}

  {#if orgError}
    <p
      class="org-error"
      role="alert"
      data-testid="history-card-org-error"
    >
      {orgError}
    </p>
  {/if}

  <footer class="card-actions">
    {#if selected}
      <span
        class="preview-hint"
        data-testid="history-card-preview-hint"
        data-preview-platform={previewPlatform}
        title={previewShortcutAccessibleLabel(previewPlatform)}
        aria-label={previewShortcutAccessibleLabel(previewPlatform)}
        aria-keyshortcuts={previewPlatform === "macos"
          ? "Meta+Enter"
          : "Control+Enter"}
      >
        <span class="preview-hint-label">Preview</span>
        <span class="preview-hint-keys" aria-hidden="true">
          {previewShortcutLabel(previewPlatform)}
        </span>
      </span>
    {/if}
    <button
      type="button"
      class="pin"
      aria-pressed={entry.is_pinned}
      title={entry.is_pinned ? "Desanclar" : "Anclar"}
      aria-label={entry.is_pinned ? `Desanclar entrada ${displayTitle}` : `Anclar entrada ${displayTitle}`}
      data-testid="history-card-pin"
      data-pinned={entry.is_pinned ? "true" : "false"}
      on:click={handlePinClick}
    >
      {#if entry.is_pinned}
        <svg
          aria-hidden="true"
          focusable="false"
          width="18"
          height="18"
          viewBox="0 0 24 24"
          data-testid="history-card-pin-filled"
        >
          <!--
            Filled diagonal chincheta. The whole silhouette is
            traced by a single `<path>` (drawn vertically, then
            tilted 45° around the viewport centre by the
            `<g transform="rotate(45 12 12)">` group) so the
            wide rounded head, the short neck, the tapered body
            and the sharp tip read as one continuous thumbtack.
            The previous circle + line + polygon composition was
            a magnifying glass; the new path keeps the wide
            rounded head in the upper-right, the tip in the
            lower-left and the silhouette inside the dark button
            background so the yellow fill stays legible. No
            star, bookmark, check, heart, emoji or textual
            fallback is rendered.
          -->
          <g transform="rotate(45 12 12)">
            <path
              d="M 3 8 C 3 1 21 1 21 8 C 21 10.5 19 11.5 16 11.5 L 12 22 L 8 11.5 C 5 11.5 3 10.5 3 8 Z"
              fill="currentColor"
              stroke="currentColor"
              stroke-width="0.6"
              stroke-linejoin="round"
            />
          </g>
        </svg>
      {:else}
        <svg
          aria-hidden="true"
          focusable="false"
          width="18"
          height="18"
          viewBox="0 0 24 24"
          data-testid="history-card-pin-outline"
        >
          <!--
            Outline diagonal chincheta. Same path as the filled
            variant rendered with `fill="none"` and a single
            `currentColor` stroke so the unpinned affordance stays
            visually distinct from the pinned one while keeping the
            exact same tilted silhouette and the sharp tip in the
            lower-left. No star, bookmark, check, heart, emoji or
            textual fallback is rendered.
          -->
          <g transform="rotate(45 12 12)">
            <path
              d="M 3 8 C 3 1 21 1 21 8 C 21 10.5 19 11.5 16 11.5 L 12 22 L 8 11.5 C 5 11.5 3 10.5 3 8 Z"
              fill="none"
              stroke="currentColor"
              stroke-width="1.4"
              stroke-linejoin="round"
              stroke-linecap="round"
            />
          </g>
        </svg>
      {/if}
    </button>
    <button
      type="button"
      class="menu-trigger"
      aria-haspopup="menu"
      aria-expanded={menuOpen}
      aria-label={`Más acciones para ${displayTitle}`}
      title="Más acciones"
      data-testid="history-card-menu-trigger"
      bind:this={menuTriggerEl}
      on:click={toggleMenu}
    >
      ⋯
    </button>
    {#if menuOpen}
      <div
        class="menu"
        class:menu-above={menuFlippedAbove}
        class:menu-scrollable={menuScrollable}
        role="menu"
        id={menuId}
        aria-label={`Acciones de ${displayTitle}`}
        data-testid="history-card-menu"
        data-menu-flip={menuFlippedAbove ? "above" : "below"}
        data-menu-scrollable={menuScrollable ? "true" : "false"}
        style={menuPositionStyle}
        bind:this={menuEl}
        on:keydown={onMenuKeydown}
      >
        <button
          type="button"
          role="menuitem"
          class="menu-item"
          data-testid="history-card-edit-title"
          on:click={() => startEditTitle()}
          disabled={titleBusy}
        >
          Editar título
        </button>
<button
        type="button"
        role="menuitem"
        class="menu-item"
        data-testid="history-card-restore-title"
        on:click={() => void restoreDefaultTitle()}
        disabled={titleBusy || (entry.title ?? "") === ""}
      >
        Restaurar título
      </button>
      {#if canEditText}
        <button
          type="button"
          role="menuitem"
          class="menu-item"
          data-testid="history-card-edit-text"
          aria-label={`Editar captura ${displayTitle}`}
          aria-keyshortcuts={editTextShortcutKeyAttributeText}
          title={editTextShortcutAccessibleText}
          on:click={openTextEditor}
          disabled={titleBusy}
        >
          <span class="menu-item-label">Editar captura</span>
          <span
            class="menu-item-shortcut"
            data-testid={EDIT_TEXT_SHORTCUT_TESTID}
            aria-hidden="true"
          >
            {editTextShortcutLabelText}
          </span>
        </button>
      {/if}
      <button
        type="button"
        role="menuitem"
        class="menu-item preview-action"
        data-testid="history-card-preview"
        data-shortcut-platform={previewPlatform}
        aria-keyshortcuts={cardMenuPreviewShortcutKeyAttribute(previewPlatform)}
        title={cardMenuPreviewShortcutAccessibleLabel(previewPlatform)}
        on:click={requestPreview}
      >
        <span class="menu-item-label">Previsualizar</span>
        <span
          class="menu-item-shortcut"
          data-testid="history-card-preview-shortcut"
          aria-hidden="true"
        >
          {cardMenuPreviewShortcutLabel(previewPlatform)}
        </span>
      </button>
      <button
        type="button"
        role="menuitem"
        class="menu-item"
        data-testid="history-card-add-tags"
        on:click={openTagSelector}
      >
        {assignedTags.length > 0 ? "Editar tags" : "Agregar tag"}
      </button>
        <button
          type="button"
          role="menuitem"
          class="menu-item"
          data-testid="history-card-add-collections"
          on:click={openCollectionSelector}
        >
          {assignedCollections.length > 0
            ? "Editar colecciones"
            : "Agregar a colección"}
        </button>
        {#if activeCollectionContext !== null}
          <button
            type="button"
            role="menuitem"
            class="menu-item"
            data-testid="history-card-remove-from-collection"
            on:click={() => void removeFromActiveCollection()}
          >
            Quitar de esta colección
          </button>
        {/if}
        {#each pasteMenuActions as pasteAction (pasteAction.testId)}
          <button
            type="button"
            role="menuitem"
            class="menu-item"
            data-testid={pasteAction.testId}
            aria-label={pasteAction.ariaLabel}
            disabled={pasteAction.disabled}
            title={pasteAction.tooltip}
            on:click={() => void runPaste(pasteAction.mode)}
          >
            {pasteAction.label}
          </button>
        {/each}
        <button
          type="button"
          role="menuitem"
          class="menu-item danger delete-action"
          data-testid="history-card-delete"
          aria-label={`Eliminar ${displayTitle}`}
          title={`Eliminar entrada · ${displayTitle}`}
          data-cv-danger="card-delete"
          on:click={handleDeleteClick}
        >
          <svg
            aria-hidden="true"
            focusable="false"
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M5 8h14" />
            <path d="M10 8V6a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v2" />
            <path d="M7 8l.6 11.2a2 2 0 0 0 2 1.8h4.8a2 2 0 0 0 2-1.8L17 8" />
            <path d="M10 11.5v5" />
            <path d="M14 11.5v5" />
          </svg>
          <span class="visually-hidden">{`Eliminar ${displayTitle}`}</span>
        </button>
      </div>
    {/if}
  </footer>
</article>

<TagSelectorModal
  open={tagSelectorOpen}
  candidateTags={allTags}
  initialSelection={assignedTagIds}
  loaded={entryOrganizationLoaded}
  on:save={handleTagsSave}
  on:cancel={handleCancelSelector}
/>

<CollectionSelectorModal
  open={collectionSelectorOpen}
  candidateCollections={allCollections}
  initialSelection={assignedCollectionIds}
  loaded={entryOrganizationLoaded}
  on:save={handleCollectionsSave}
  on:cancel={handleCancelSelector}
/>

<CollectionMembershipModal
  open={membershipModalOpen}
  entryId={entry.id}
  displayTitle={displayTitle}
  assignedCollections={assignedCollections}
  returnFocusTo={collectionOverflowEl}
  on:close={() => { membershipModalOpen = false; }}
/>

<EntryTextEditorModal
  open={textEditorOpen}
  {entry}
  {displayTitle}
  returnFocusTo={textEditorReturnFocusTarget}
  on:close={closeTextEditor}
/>

{@html CONTENT_TYPE_ICON_SPRITE}

<style>
  .card {
    flex: 0 0 var(--cv-card-size, 240px);
    width: var(--cv-card-size, 240px);
    height: var(--cv-card-size, 240px);
    box-sizing: border-box;
    display: grid;
    grid-template-rows: auto auto 1fr auto;
    gap: 0.4rem;
    padding: 0.6rem 0.75rem 0.65rem;
    background: #161b22;
    border: 1px solid #30363d;
    border-radius: 12px;
    overflow: hidden;
    position: relative;
    user-select: none;
    -webkit-user-select: none;
    touch-action: none;
    transition: border-color 120ms ease, box-shadow 120ms ease,
      background 120ms ease;
  }

  /*
   * Visual selection state the rail drives through the `selected`
   * prop. The contract is `entry.id === selectedEntryId` (strict);
   * the same predicate powers `aria-selected`, the preview-shortcut
   * hint and the `data-selected` attribute so the four indicators
   * can never drift. The previous baseline painted the class on the
   * DOM but never declared the rule so the user only ever saw the
   * default card chrome and could not tell which row was active —
   * the regression surfaced as "clicking card 3 still selects card 1".
   * The accent border, the soft ring and the lighter background are
   * the same palette the Quick Paste row uses so the two surfaces
   * read as the same affordance.
   */
  .card.card-selected {
    border-color: rgba(96, 165, 250, 0.85);
    background: rgba(96, 165, 250, 0.08);
    box-shadow:
      0 0 0 1px rgba(96, 165, 250, 0.45),
      0 6px 18px rgba(15, 23, 42, 0.55);
  }

  /*
   * The native WebView focus ring is theme-dependent and can be red
   * in some GTK/WebKit configurations. Suppress it for an unselected
   * card so stale DOM focus never looks like a second state marker;
   * the selected focus rule below supplies the single blue cue.
   */
  .card:focus {
    outline: none;
  }

  /*
   * Keep programmatic and keyboard focus on the same blue navigation
   * cue, but only for the rail's current selection. After editing,
   * browser focus can remain on the previous card while ArrowLeft /
   * ArrowRight moves `selectedEntryId`; scoping this rule prevents
   * that residual focus from painting a second selected card.
   */
  .card.card-selected:focus-visible {
    border-color: rgba(96, 165, 250, 0.85);
    outline: 2px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.85));
    outline-offset: 1px;
    box-shadow:
      0 0 0 1px rgba(96, 165, 250, 0.45),
      0 6px 18px rgba(15, 23, 42, 0.55);
  }

  /*
   * Visual cue for an open ellipsis menu. The pin button keeps its
   * pressed / unpinned palette through `:global` rules below; this
   * rule only nudges the card chrome so the user can tell which row
   * owns the popover even when the popover itself sits over a
   * different row (the popover is `position: fixed` and can render
   * anywhere in the viewport). The previous baseline declared
   * `class:menu-open` on the `<article>` without a rule so the cue
   * was effectively invisible.
   */
  .card.menu-open {
    border-color: rgba(148, 163, 184, 0.65);
    box-shadow: 0 0 0 1px rgba(148, 163, 184, 0.35);
  }

  /*
   * Selection wins over the open-menu cue so a card that owns both
   * flags keeps the blue accent the keyboard shortcut requires.
   */
  .card.card-selected.menu-open {
    border-color: rgba(96, 165, 250, 0.85);
    box-shadow:
      0 0 0 1px rgba(96, 165, 250, 0.45),
      0 6px 18px rgba(15, 23, 42, 0.55);
  }

  .card-header {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 0.5rem;
    color: #93c5fd;
  }

  .type {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.5rem;
    height: 1.5rem;
    border-radius: 6px;
    background: rgba(147, 197, 253, 0.12);
  }

  .title {
    margin: 0;
    font-size: var(--cv-control, 0.85rem);
    line-height: 1.2;
    font-weight: 600;
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: #f0f4f8;
    border: 0;
    background: transparent;
    border-radius: 6px;
    padding: 0.1rem 0.35rem;
    display: flex;
    align-items: center;
    gap: 0.25rem;
    justify-content: center;
    cursor: pointer;
  }
  .title:focus-visible {
    outline: 1px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.45));
    outline-offset: 0;
  }

  .title-input {
    flex: 1 1 auto;
    min-width: 0;
    font: inherit;
    font-size: var(--cv-control, 0.85rem);
    background: #0e1116;
    color: #f0f4f8;
    border: 1px solid #2563eb;
    border-radius: 6px;
    padding: 0.15rem 0.35rem;
    box-sizing: border-box;
  }
  .icon-inline {
    flex: 0 0 auto;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 6px;
    width: 1.45rem;
    height: 1.45rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    padding: 0;
  }
  .icon-inline:focus-visible {
    outline: 1px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.45));
  }
  .title-confirm {
    color: #4ade80;
  }
  .title-cancel {
    color: #94a3b8;
  }

  .title-error {
    display: block;
    flex: 1 1 100%;
    margin-top: 0.25rem;
    color: #f87171;
    font-size: var(--cv-meta, 0.7rem);
    font-weight: 500;
  }

  .source-app {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: #94a3b8;
  }

  .source-app-icon,
  .source-app-fallback {
    /*
     * The previous baseline was `1.1rem`. The spec mandates a 50%
     * increase so the application identity remains visible at a
     * glance; `1.65rem` is exactly 50% larger than `1.1rem`. The
     * square aspect ratio, `object-fit: contain`, the
     * `border-radius`, the accessible name and the icon resolver
     * all stay intact — the change is presentation-only.
     */
    width: 1.65rem;
    height: 1.65rem;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.06);
    object-fit: contain;
    flex: 0 0 auto;
  }

  .source-app-fallback {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: #94a3b8;
  }

  :global(.source-app-fallback svg) {
    width: 100%;
    height: 100%;
  }

  .metadata {
    margin: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.4rem;
    padding: 0.1rem 0.45rem;
    border-radius: 6px;
    background: rgba(148, 163, 184, 0.08);
    border: 1px solid rgba(148, 163, 184, 0.15);
    color: #cbd5f5;
    font-size: var(--cv-meta, 0.7rem);
    font-variant-numeric: tabular-nums;
  }
  .metadata-age,
  .metadata-size {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    min-width: 0;
  }
  .metadata-age {
    color: #93c5fd;
  }
  .metadata-size {
    color: #cbd5f5;
    justify-self: end;
  }

  .preview {
    margin: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: var(--cv-preview, 0.72rem);
    line-height: 1.35;
    color: #cbd5f5;
    background: rgba(0, 0, 0, 0.18);
    border-radius: 6px;
    padding: 0.5rem;
    overflow: hidden;
    display: -webkit-box;
    line-clamp: 5;
    -webkit-line-clamp: 5;
    -webkit-box-orient: vertical;
    white-space: pre-wrap;
    word-break: break-word;
  }

  /*
   * The code-preview variant of `.preview`. The card reuses the
   * `renderHighlightedCode` helper the shared `ClipboardPreview`
   * overlay consumes, so the markup carries the syntax colours
   * the overlay paints. The whitespace contract is identical to
   * the plain-text branch: `white-space: pre-wrap` and
   * `tab-size: 4` keep the original tabs, indentation and
   * line breaks visible. The `overflow: hidden` plus the
   * `-webkit-line-clamp` rules bound the card geometry so a long
   * capture never widens or grows the rail.
   */
  .preview-code {
    padding: 0.5rem 0.6rem;
    tab-size: 4;
    -moz-tab-size: 4;
    background: rgba(0, 0, 0, 0.28);
    border: 1px solid rgba(147, 197, 253, 0.18);
  }

  .preview-code :global(.hljs-keyword),
  .preview-code :global(.hljs-built_in),
  .preview-code :global(.hljs-type) {
    color: #93c5fd;
  }

  .preview-code :global(.hljs-string),
  .preview-code :global(.hljs-attr) {
    color: #f9a8d4;
  }

  .preview-code :global(.hljs-number),
  .preview-code :global(.hljs-literal) {
    color: #fcd34d;
  }

  .preview-code :global(.hljs-comment) {
    color: #94a3b8;
    font-style: italic;
  }

  .preview-code :global(.hljs-title),
  .preview-code :global(.hljs-function),
  .preview-code :global(.hljs-class),
  .preview-code :global(.hljs-name) {
    color: #5eead4;
  }

  .preview-code :global(.hljs-variable),
  .preview-code :global(.hljs-params) {
    color: #f0f4f8;
  }

  .card-code-language {
    /* The compact language badge shares the documented
     * `--cv-tag` token the tag chips use so the card metadata stays
     * visually consistent. The badge is decorative — the canonical
     * `data-code-language` attribute carries the language the UI
     * helpers, the bridge and the persistence layer consume. */
    margin: 0;
    padding: 0.1rem 0.45rem;
    align-self: flex-start;
    border-radius: 4px;
    background: rgba(94, 234, 212, 0.15);
    color: #5eead4;
    font-size: var(--cv-tag, 0.65rem);
    font-weight: 500;
    letter-spacing: 0.02em;
  }

  /*
   * The thumbnail occupies exactly the same grid row as `.preview`, so
   * an image card and a text card keep identical outer dimensions. The
   * `min-height: 0` is required for the image to shrink inside the
   * grid row instead of pushing the footer out of the square.
   */
  .thumbnail-area {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 0;
    overflow: hidden;
    background: rgba(0, 0, 0, 0.18);
    border-radius: 6px;
  }

  .paste-error {
    position: absolute;
    inset: auto 0.5rem 2.6rem 0.5rem;
    margin: 0;
    padding: 0.45rem 0.6rem;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    border-radius: 6px;
    background: rgba(248, 113, 113, 0.15);
    border: 1px solid rgba(248, 113, 113, 0.45);
    color: #fecaca;
    font-size: var(--cv-meta, 0.7rem);
    line-height: 1.25;
    z-index: 5;
  }

  .paste-error strong {
    font-size: var(--cv-body, 0.9rem);
    font-weight: 600;
    color: #fee2e2;
  }

  .paste-error-dismiss {
    position: absolute;
    top: 0.2rem;
    right: 0.3rem;
    background: transparent;
    color: inherit;
    border: 0;
    font-size: var(--cv-body, 0.9rem);
    line-height: 1;
    cursor: pointer;
    padding: 0.1rem 0.3rem;
    border-radius: 4px;
  }

  .paste-error-dismiss:hover {
    background: rgba(248, 113, 113, 0.25);
  }

  .thumbnail {
    max-width: 100%;
    max-height: 100%;
    /* Never crop or distort: the whole capture stays visible inside
       the fixed square. */
    object-fit: contain;
    display: block;
  }

  .thumbnail-fallback {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    width: 100%;
    height: 100%;
    color: #94a3b8;
    text-align: center;
    padding: 0.35rem;
    box-sizing: border-box;
  }

  .thumbnail-fallback-text {
    font-size: var(--cv-meta, 0.7rem);
    line-height: 1.2;
  }

  /*
   * Loading placeholder rendered while the bridge round-trip is in
   * flight. The shape matches the fallback so the card keeps the same
   * outer square while the bytes travel; only the colour and copy
   * change, so a regression that drops the loading state back into
   * the error fallback is caught by the data-thumbnail-state and
   * data-testid hooks.
   */
  .thumbnail-loading {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    width: 100%;
    height: 100%;
    color: #93c5fd;
    text-align: center;
    padding: 0.35rem;
    box-sizing: border-box;
    background: rgba(147, 197, 253, 0.06);
    border: 1px dashed rgba(147, 197, 253, 0.35);
    border-radius: 6px;
    opacity: 0.85;
  }

  .thumbnail-loading-text {
    font-size: var(--cv-meta, 0.7rem);
    line-height: 1.2;
    font-variant-numeric: tabular-nums;
  }

  .card-actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 0.4rem;
    position: relative;
  }

  /*
   * Compact preview-shortcut hint that surfaces only for the
   * currently selected card. The pill lives in the footer flow,
   * aligned to the left, so it reads as a continuation of the
   * card instead of an overlay on the pin or the menu. The
   * `margin-right: auto` keeps pin and menu-trigger pinned to the
   * right edge while the hint takes the remaining width on the
   * left; the `max-width` reserves enough room for both controls
   * so the fixed `--cv-card-size` footprint never has to grow.
   * The hint uses the platform-aware label the shared
   * `ClipboardPreview` matcher consumes so the visible glyph and
   * the keyboard shortcut cannot drift apart.
   */
  .preview-hint {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.1rem 0.4rem;
    border-radius: 999px;
    background: rgba(96, 165, 250, 0.18);
    border: 1px solid rgba(96, 165, 250, 0.45);
    color: #cbd5f5;
    font-size: var(--cv-tag, 0.65rem);
    line-height: 1;
    pointer-events: none;
    user-select: none;
    -webkit-user-select: none;
    max-width: calc(100% - 5rem);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    margin-right: auto;
    flex: 0 1 auto;
    min-width: 0;
  }
  .preview-hint-label {
    font-weight: 600;
    color: #93c5fd;
  }
  .preview-hint-keys {
    font-weight: 700;
    color: #f0f4f8;
    font-variant-numeric: tabular-nums;
  }

  .card-actions :global(.pin),
  .card-actions :global(.menu-trigger) {
    background: #1f2937;
    color: inherit;
    border: 0;
    border-radius: 6px;
    cursor: pointer;
    padding: 0.25rem 0.55rem;
    font-size: var(--cv-control, 0.85rem);
    line-height: 1;
    /*
     * The pin button hosts a 18x18 SVG so the chincheta stays
     * legible without spilling past the rail. We pin both
     * action buttons to the same min-height so the ellipsis
     * trigger and the pin stay aligned when the chincheta path
     * is bigger than the text line-box of `.menu-trigger`.
     */
    min-height: 1.65rem;
    box-sizing: border-box;
  }

  /*
   * Pin button: outlined local chincheta glyph when unpinned, filled
   * (filled + highlighted) when the entry is pinned. The button
   * background stays dark in both states so the yellow fill stays
   * legible on the dark surface and the outline keeps its lavender
   * tint. The SVG itself is the only visual element rendered inside
   * the button — no glyphs, no emojis, no external resources.
   */
  .card-actions :global(.pin) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: #aab4c8;
    transition: color 0.15s ease, background 0.15s ease;
  }
  .card-actions :global(.pin[aria-pressed="true"]) {
    color: #f5c542;
  }
  .card-actions :global(.pin:hover:not(:disabled)) {
    background: rgba(170, 180, 200, 0.18);
    color: #c9d1de;
  }
  .card-actions :global(.pin[aria-pressed="true"]:hover:not(:disabled)) {
    background: rgba(245, 197, 66, 0.18);
    color: #ffd34d;
  }
  .card-actions :global(.pin:focus-visible) {
    outline: 2px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.45));
    outline-offset: 2px;
  }
  .card-actions :global(.pin:disabled) {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .card-actions :global(.menu-trigger[aria-expanded="true"]) {
    background: #2563eb;
    color: white;
  }

  .menu {
    /*
     * The popover is portalised through `position: fixed` (the
     * inline `style` string `cardMenuStyle` produces). The card's
     * own `overflow: hidden` clipping context cannot intercept a
     * fixed-position descendant because the card does not create a
     * containing block for fixed elements (no transform / filter /
     * perspective / will-change on any ancestor). The geometry
     * fields (top, left, width, max-height) and the `z-index`
     * value live on the inline style so the helper is the single
     * source of truth for the popover rectangle; the CSS class
     * only carries the chrome (background, border, padding,
     * shadow, layout) so the popover never doubles up the
     * `position` declaration.
     */
    display: flex;
    flex-direction: column;
    min-width: 11rem;
    background: #0e1116;
    border: 1px solid #30363d;
    border-radius: 8px;
    padding: 0.25rem;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.45);
  }

  .menu-item {
    background: transparent;
    color: inherit;
    border: 0;
    text-align: left;
    padding: 0.4rem 0.6rem;
    border-radius: 6px;
    cursor: pointer;
    font-size: var(--cv-control, 0.85rem);
  }

  .menu-item:hover:not(:disabled) {
    background: #1f2937;
  }

  /*
   * Delete menu item. The textual "Eliminar" label is intentionally
   * hidden — the icon already speaks the action and the parent owns
   * the destructive confirmation modal. The accessible name and the
   * `title` tooltip keep the entry discoverable for screen-reader and
   * mouse-only users. The danger colour is the documented surface
   * the rail relies on to telegraph the destructive branch.
   */
  .menu-item.danger.delete-action {
    color: var(--cv-danger, #b91c1c);
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    font-weight: 600;
    justify-content: center;
    padding: 0.4rem 0.55rem;
  }
  .menu-item.danger.delete-action:hover:not(:disabled) {
    background: rgba(185, 28, 28, 0.18);
    color: var(--cv-danger-hover, #991b1b);
  }
  .menu-item.danger.delete-action:focus-visible {
    outline: 2px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.45));
    outline-offset: 1px;
  }

  .menu-item:disabled {
    color: #94a3b8;
    cursor: not-allowed;
  }

  /*
   * Layout for menu items that surface a label + a keyboard shortcut
   * hint (currently the `Editar captura` and `Previsualizar` items).
   * The container turns into a flex row so the hint sits flush on
   * the right edge of the popover; the shortcut span keeps the
   * monospace treatment the rest of the desktop uses for keyboard
   * hints so the typography is consistent with the `preview-hint`
   * chrome the card already paints when the user picks a row.
   */
  .menu-item-label {
    flex: 1 1 auto;
  }
  .menu-item-shortcut {
    flex: 0 0 auto;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: var(--cv-meta, 0.7rem);
    color: var(--cv-fg-muted, #94a3b8);
    padding-left: 0.75rem;
  }

  .tag-chips {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
    align-items: center;
  }
  .tag-chip {
    background: rgba(147, 197, 253, 0.12);
    color: #93c5fd;
    border: 1px solid #30363d;
    border-radius: 999px;
    padding: 0.05rem 0.45rem;
    font-size: var(--cv-tag, 0.65rem);
    line-height: 1.1;
    max-width: 7rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tag-chip.more {
    background: rgba(255, 255, 255, 0.06);
    color: #cbd5f5;
  }
  /*
   * Collection chip row the `clipboard-history-cards` spec
   * requires. The row sits directly below the tag chips and
   * renders every assigned user collection using the
   * collection's persisted `color_hex` as its text colour; the
   * protected `Historial` system row is excluded from this row
   * and is only surfaced through the membership modal so the
   * inline chips never duplicate the sidebar's `Historial` row.
   * The fallback helper `collectionColor` guarantees a valid
   * value even for legacy rows that pre-date the colour
   * migration.
   *
   * The row never wraps: every chip is rendered with `flex: 0 0
   * auto` and the parent uses `overflow: hidden` so the visible
   * subset painted by `visibleUserCollections` is the canonical
   * surface. The full label list is exposed through the
   * `aria-label` attribute so assistive technology can announce
   * every membership name in spite of the visual clip.
   *
   * When the chips cannot all fit, the row paints the
   * `.collection-overflow` button the card binds to
   * `collectionOverflowEl`. The button is the only interactive
   * surface in the row; the chips themselves remain decorative
   * (`pointer-events: none`) so the card's existing
   * drag-and-drop, pin, paste and ellipsis-menu flows are
   * unaffected.
   */
  .collection-chips {
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: nowrap;
    align-items: center;
    gap: 0.25rem;
    overflow: hidden;
    max-width: 100%;
    user-select: none;
    -webkit-user-select: none;
  }
  .collection-chip {
    flex: 0 0 auto;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(148, 163, 184, 0.35);
    border-radius: 999px;
    padding: 0.05rem 0.45rem;
    font-size: var(--cv-tag, 0.65rem);
    font-weight: 600;
    letter-spacing: 0.02em;
    line-height: 1.1;
    max-width: 7rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    /* The chip itself is decorative; only the overflow button is
     * interactive. Routing the pointer through to the chip would
     * race the drag controller and re-emit selection clicks. */
    pointer-events: none;
  }
  /*
   * Overflow chip rendered with the documented `+N` indicator
   * the responsive subset computation produces. The chip reuses
   * the `.tag-chip.more` rule the card already exposes for tag
   * chips so the two indicators read as the same affordance; the
   * extra `.collection-overflow` selector only contributes the
   * interactive surface (cursor, focus ring, hover) and the
   * button reset the row needs. Padding, border, radius and
   * typography all come from `.tag-chip.more`.
   */
  .collection-overflow {
    flex: 0 0 auto;
    font-family: inherit;
    line-height: 1.1;
    text-align: center;
    cursor: pointer;
    transition:
      background-color 0.15s ease-out,
      border-color 0.15s ease-out,
      color 0.15s ease-out;
  }
  .collection-overflow:hover {
    background: rgba(148, 163, 184, 0.2);
    color: #f0f4f8;
    border-color: rgba(148, 163, 184, 0.55);
  }
  .collection-overflow:focus-visible {
    outline: 2px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.45));
    outline-offset: 1px;
  }
  /*
   * Hidden measurement strip the responsive overflow pass
   * consults. The strip is positioned absolutely off-screen and
   * carries `aria-hidden="true"` so it never reaches assistive
   * technology; the `pointer-events: none` rule guarantees the
   * pointer drag controller's hit-tests never land on the
   * measurement rows. The chips inside the strip share the
   * `.collection-chip-measure` class so the offsetWidth the
   * helper measures mirrors the painted chip's geometry. The
   * trailing `.collection-overflow-measure` span mirrors the
   * `.tag-chip.more` chrome so its measured width matches the
   * painted overflow chip.
   */
  .collection-chips-measure {
    position: absolute;
    top: -9999px;
    left: -9999px;
    visibility: hidden;
    pointer-events: none;
    user-select: none;
    -webkit-user-select: none;
    /* The measurement strip renders the chips with the same
     * flex layout the visible row uses so the cached widths
     * match the painted chips. We deliberately avoid
     * `display: none` so the chips keep their offsetWidth. */
    display: flex;
    flex-wrap: nowrap;
    align-items: center;
    gap: 0.25rem;
  }
  .collection-chip-measure {
    flex: 0 0 auto;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(148, 163, 184, 0.35);
    border-radius: 999px;
    padding: 0.05rem 0.45rem;
    font-size: var(--cv-tag, 0.65rem);
    font-weight: 600;
    letter-spacing: 0.02em;
    line-height: 1.1;
    max-width: 7rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /*
   * Mirror of the painted overflow chip inside the measurement
   * strip. The same `.tag-chip.more` styling applies so the
   * measured width is the actual painted width. The row reads it
   * through `data-collection-overflow-measure-item` so the
   * `computeVisibleCollections` helper can use the real
   * reservation instead of a fixed fallback.
   */
  .collection-overflow-measure {
    flex: 0 0 auto;
    background: rgba(255, 255, 255, 0.06);
    color: #cbd5f5;
    border: 1px solid #30363d;
    border-radius: 999px;
    padding: 0.05rem 0.45rem;
    font-size: var(--cv-tag, 0.65rem);
    font-weight: 600;
    letter-spacing: 0.02em;
    line-height: 1.1;
    white-space: nowrap;
  }
  .org-error {
    margin: 0;
    padding: 0.35rem 0.5rem;
    border-radius: 6px;
    background: rgba(248, 113, 113, 0.15);
    border: 1px solid rgba(248, 113, 113, 0.45);
    color: #fee2e2;
    font-size: var(--cv-meta, 0.7rem);
  }

  /*
   * Hydration indicator rendered in place of the chip row while the
   * per-entry organisation cache is in flight. The card deliberately
   * does NOT show an empty chip row here — an empty array would let
   * the modal open with `initialSelection = []` and clobber the
   * persisted tags the next time the user pressed **Guardar**.
   */
  .tag-organization-pending,
  .tag-organization-error {
    margin: 0;
    padding: 0.2rem 0.45rem;
    border-radius: 6px;
    font-size: var(--cv-meta, 0.7rem);
    line-height: 1.2;
    border: 1px dashed #30363d;
  }
  .tag-organization-pending {
    color: #94a3b8;
    font-style: italic;
    background: rgba(148, 163, 184, 0.06);
  }
  .tag-organization-error {
    color: #fecaca;
    border-color: rgba(248, 113, 113, 0.45);
    background: rgba(248, 113, 113, 0.08);
  }

  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    padding: 0;
    overflow: hidden;
    clip: rect(0 0 0 0);
    border: 0;
  }
</style>
