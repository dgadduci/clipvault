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
  import type { Collection, EntryRecord, Tag } from "./types";
  import {
    defaultCardTitle,
    validateTitle,
  } from "./lib/contentType";
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
  import TagSelectorModal from "./TagSelectorModal.svelte";
  import CollectionSelectorModal from "./CollectionSelectorModal.svelte";

  export let entry: EntryRecord;
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

  const dispatch = createEventDispatcher<{
    "menu-toggle": { id: number; open: boolean };
  }>();

  /** Local override of the persisted title so edits feel instant. */
  let localTitle: string | null = null;
  $: displayTitle =
    (localTitle ?? entry.title ?? "").trim().length > 0
      ? (localTitle ?? entry.title ?? "").trim()
      : defaultCardTitle(entry.content_type);

  /** Ellipsis-menu state. The rail enforces a single-open invariant. */
  let menuOpen = false;
  let menuId = `card-menu-${entry.id}`;

  /** Title-editing state. */
  let editingTitle = false;
  let titleDraft = "";
  let titleError: string | null = null;
  let titleBusy = false;
  let titleInputEl: HTMLInputElement | null = null;

  $: iconRef = entry.source_app_icon_ref ?? null;
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
    menuOpen = false;
    dispatch("menu-toggle", { id: entry.id, open: false });
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
    menuOpen = !menuOpen;
    if (menuOpen) {
      titleError = null;
    }
    dispatch("menu-toggle", { id: entry.id, open: menuOpen });
  }

  function startEditTitle(): void {
    titleDraft = entry.title ?? "";
    titleError = null;
    editingTitle = true;
    menuOpen = false;
    dispatch("menu-toggle", { id: entry.id, open: false });
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
    menuOpen = false;
    dispatch("menu-toggle", { id: entry.id, open: false });
  }

  function handlePinClick(): void {
    onTogglePin(entry);
  }

  function handleDeleteClick(): void {
    onRequestDelete(entry);
    menuOpen = false;
    dispatch("menu-toggle", { id: entry.id, open: false });
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
  $: activeCollectionContext = (() => {
    if (activeCollectionId === null) return null;
    if (allCollections.length === 0) return null;
    const target = allCollections.find((c) => c.id === activeCollectionId);
    if (!target || target.kind === "system") return null;
    return target;
  })();

  let tagSelectorOpen = false;
  let collectionSelectorOpen = false;
  let orgBusy = false;
  let orgError: string | null = null;

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
      event.preventDefault();
      menuOpen = false;
      dispatch("menu-toggle", { id: entry.id, open: false });
    }
  }

  // Dragging is delegated to the singleton pointer controller installed by
  // App.svelte. Keeping the card itself non-draggable prevents the unreliable
  // HTML5 WebKit lifecycle from competing with pointer hit-testing.
  onDestroy(() => {
    if (lastIconRef) {
      iconResolver.releaseFor(lastIconRef);
    }
    // Revoke every blob URL the card minted for its payload assets
    // (image only — rich preview is no longer rendered) so a
    // long-lived rail does not leak memory as the user scrolls.
    assetResolver.release();
  });
</script>

<article
  class="card"
  data-testid="history-card"
  data-entry-id={entry.id}
  aria-label={displayTitle}
  draggable="false"
>
  <header class="card-header">
    <span class="type" data-testid="history-card-type">
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
      title={sourceAppAccessibleLabel(entry)}
      aria-label={sourceAppAccessibleLabel(entry)}
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
            if (entry.source_app_icon_ref) {
              iconResolver.releaseFor(entry.source_app_icon_ref);
            }
          }}
        />
      {:else}
        <span
          class="source-app-fallback"
          aria-hidden="true"
          data-testid="history-card-source-app-fallback"
        >
          {@html APP_FALLBACK_ICON_SVG}
        </span>
      {/if}
      <span class="visually-hidden" data-testid="history-card-source-app-accessible">
        {sourceAppAccessibleLabel(entry)}
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
    <pre
      class="preview"
      data-testid="history-card-preview"
      data-content-type={isRich ? "rich_text" : "text"}
      aria-label="Contenido capturado"
    >{previewText}</pre>
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
      on:click={toggleMenu}
    >
      ⋯
    </button>
    {#if menuOpen}
      <div
        class="menu"
        role="menu"
        id={menuId}
        aria-label={`Acciones de ${displayTitle}`}
        data-testid="history-card-menu"
        on:keydown={onMenuKeydown}
      >
        <button
          type="button"
          role="menuitem"
          class="menu-item"
          data-testid="history-card-restore-title"
          on:click={() => void restoreDefaultTitle()}
          disabled={titleBusy}
        >
          Restaurar título
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
    font-size: 0.85rem;
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
    font-size: 0.85rem;
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
    font-size: 0.7rem;
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
    font-size: 0.7rem;
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
    font-size: 0.72rem;
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
    font-size: 0.7rem;
    line-height: 1.25;
    z-index: 5;
  }

  .paste-error strong {
    font-size: 0.75rem;
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
    font-size: 0.9rem;
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
    font-size: 0.7rem;
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
    font-size: 0.7rem;
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

  .card-actions :global(.pin),
  .card-actions :global(.menu-trigger) {
    background: #1f2937;
    color: inherit;
    border: 0;
    border-radius: 6px;
    cursor: pointer;
    padding: 0.25rem 0.55rem;
    font-size: 0.85rem;
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
    position: absolute;
    right: 0;
    bottom: calc(100% + 0.35rem);
    display: flex;
    flex-direction: column;
    min-width: 11rem;
    background: #0e1116;
    border: 1px solid #30363d;
    border-radius: 8px;
    padding: 0.25rem;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.45);
    z-index: 4;
  }

  .menu-item {
    background: transparent;
    color: inherit;
    border: 0;
    text-align: left;
    padding: 0.4rem 0.6rem;
    border-radius: 6px;
    cursor: pointer;
    font-size: 0.85rem;
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
    font-size: 0.65rem;
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
  .org-error {
    margin: 0;
    padding: 0.35rem 0.5rem;
    border-radius: 6px;
    background: rgba(248, 113, 113, 0.15);
    border: 1px solid rgba(248, 113, 113, 0.45);
    color: #fee2e2;
    font-size: 0.7rem;
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
    font-size: 0.7rem;
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
