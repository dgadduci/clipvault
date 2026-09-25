<script lang="ts">
  /**
   * Metadata-only horizontal rail the desktop renders when the
   * user selects a paired, active peer from the `Equipos
   * vinculados` list. The component replaces the local history
   * / collection content the main panel would otherwise show;
   * selecting a local collection or the `Historial` row returns
   * the panel to its default surface.
   *
   * The rail owns one in-flight page per peer; switching peers
   * keeps the previous peer cached so a quick back-and-forth
   * never costs an extra round-trip. A response that arrives
   * after the user already switched peer / closed the rail is
   * dropped on the floor: every async call carries the peer id
   * the request started against and the rail discards any
   * response whose `peerId` no longer matches the active one.
   * A network failure surfaces a typed error copy and keeps the
   * previous peer available so the user can return to the local
   * rail or pick another peer.
   *
   * No hay botón `Volver` como navegación principal: volver
   * ocurre al seleccionar `Historial` o una colección local, que
   * el padre enruta a través de `selectCollectionFromSidebar`
   * (que limpia `activePeerId`). El callback `onClose` queda
   * sólo para limpiar caches cuando el peer activo deja de
   * estar disponible y el padre desmonta el rail.
   */
  import { onDestroy } from "svelte";
  import {
    peerHistoryBrowseCommand,
    peerHistoryRecordStateCommand,
    peerImportRecordStateCommand,
    peerImageBrowseCommand,
    peerImageRecordStateCommand,
    peerImageImportRecordStateCommand,
    peerImageThumbnailRecordStateCommand,
  } from "./lib/tauri";
  import type {
    PeerSnapshot,
  } from "./types";
  import RemotePreviewCard from "./RemotePreviewCard.svelte";
  import { remoteImageThumbnailCardKey } from "./lib/remoteImageThumbnailState";
  import {
    INITIAL_CURSORS,
    applyResponses as mergeResponses,
    pickNextCursors,
    promoteBuffers,
    type PageState,
    type RemoteRailRow,
  } from "./lib/remoteHistoryMerge";

  /**
   * Snapshot the parent supplies so the rail can keep the
   * `trust_state` / `is_present` cache the
   * `peerHistoryRecordStateCommand` consults in sync with the
   * linked list. The rail never opens a snapshot round-trip of
   * its own.
   */
  export let snapshot: PeerSnapshot | null = null;
  /**
   * Active peer id the parent routes through the main panel.
   * `null` collapses the rail to its empty state — the parent
   * renders the local rail instead.
   */
  export let peerId: string | null = null;
  /**
   * Callback the parent uses to forget the runtime cache entry
   * the peer-pairing commands consult so a subsequent
   * re-selection refetches the page. The rail never calls it as
   * a primary "back" affordance — that lives in the local
   * collection / `Historial` selection.
   */
  export let onClose: () => void = () => undefined;

  /**
   * Per-peer page cache so a back-and-forth between peers does
   * not always pay a round-trip. The cache is metadata-only
   * (rows + cursor + snapshot id) and never persists anywhere
   * outside the in-memory map. The structure is keyed by peer
   * id; the parent decides which peer the main panel shows.
   */
  const pageCache = new Map<string, PageState>();

  /**
   * Active page state the rail renders. The reactive variable
   * is always a fresh shallow copy so Svelte picks up the
   * mutations the async responses cause.
   */
  let rows: RemoteRailRow[] = [];
  let cursor: string = "";
  let imageCursor: string = "";
  let snapshotId: string = "";
  let loading = false;
  let error: string | null = null;
  let exhausted = false;
  let textBuffer: RemoteRailRow[] = [];
  let imageBuffer: RemoteRailRow[] = [];
  let thumbnailPeerStateReady = false;
  let peerStateSyncKey: string | null = null;
  let peerStateSyncPromise: Promise<void> | null = null;
  let peerStateSyncGeneration = 0;
  // Start detached from the prop so the initial reactive pass treats an
  // already-selected peer exactly like a later selection. Initialising this
  // from `peerId` skipped the only branch that loads the first remote page.
  let activePeerId: string | null = null;

  /**
   * Per-peer sequence number the rail uses to discard responses
   * for a peer the user already left. Every async call captures
   * the current value; the response handler only mutates the
   * visible state when the captured value still matches the
   * current one. A response that arrives after the user
   * switched peer / closed the rail is silently dropped.
   */
  let loadGeneration = 0;

  /**
   * Whether the active peer is currently `Active` per the
   * snapshot the parent supplied. The rail surfaces the no
   * `Activo` placeholder without opening a network call, exactly
   * as the spec scenario "Peer becomes unavailable" requires.
   */
  $: activeEntry =
    peerId !== null
      ? (snapshot?.entries ?? []).find((entry) => entry.peer_id === peerId) ?? null
      : null;
  $: activeIsTrusted = activeEntry?.trust_state === "trusted";
  $: activeIsPresent = activeEntry?.is_present ?? false;
  // Combine the legacy capability token and additive capabilities
  // from the peer snapshot. The renderer must not infer
  // `image_import` from the legacy `pairing` field because strict
  // legacy parsers require that field to remain exactly `pairing`.
  // The host / client core still revalidates the gate.
  $: activePeerCapability = activeEntry
    ? [activeEntry.capability, activeEntry.caps_extra]
        .filter((token) => token.length > 0)
        .join(",")
    : null;
  // The exact predicate the linked list uses to colour the dot;
  // mirroring it here guarantees the rail and the dot can never
  // disagree. A peer that is NOT `trusted && is_present` MUST NOT
  // open a network call, so the rail surfaces the unavailable
  // placeholder and stops there.
  $: railShouldShowUnavailable = peerId !== null && !(activeIsTrusted && activeIsPresent);

  /**
   * Whenever the parent flips the active peer id we restore the
   * cached page (if any) and trigger the first-page request when
   * the cache is empty / exhausted AND the peer is
   * `trusted && is_present`. A peer that fails the gate MUST NOT
   * trigger a network call — the unavailable placeholder the
   * template renders is the only feedback the user gets until
   * the trust / presence cache flips back to active.
   *
   * `peerId === null` means the parent tore the rail down
   * (e.g. the user picked `Historial` or a local collection in
   * the sidebar). The helper drops the runtime cache so a
   * subsequent re-selection starts clean.
   */
  $: if (peerId !== activePeerId) {
    activePeerId = peerId;
    loadGeneration += 1;
    thumbnailPeerStateReady = false;
    error = null;
    if (peerId === null) {
      forgetRail();
    } else if (railShouldShowUnavailable) {
      rows = [];
      cursor = "";
      imageCursor = "";
      snapshotId = "";
      textBuffer = [];
      imageBuffer = [];
      loading = false;
      error = null;
      exhausted = false;
      void refreshPeerState(peerId).catch(() => undefined);
    } else {
      const cached = pageCache.get(peerId);
      if (cached) {
        rows = cached.rows;
        cursor = cached.cursor;
        imageCursor = cached.imageCursor;
        textBuffer = cached.textBuffer;
        imageBuffer = cached.imageBuffer;
        snapshotId = cached.snapshotId;
        loading = cached.loading;
        error = cached.error;
        exhausted = cached.exhausted;
        void refreshPeerState(peerId).catch(() => undefined);
      } else {
        rows = [];
        cursor = "";
        imageCursor = "";
        snapshotId = "";
        textBuffer = [];
        imageBuffer = [];
        loading = false;
        error = null;
        exhausted = false;
        void loadInitialPage(peerId, loadGeneration);
      }
    }
  }

  /**
   * Best-effort sync hook that keeps the history and import
   * runtime trust / active caches aligned with the latest
   * snapshot. Both actions apply the same eligibility gate: a
   * remote row that was safe to browse must not be rejected as
   * an unknown peer when the user explicitly chooses Importar.
   */
  function refreshPeerState(targetPeerId: string): Promise<void> {
    const entry = (snapshot?.entries ?? []).find(
      (candidate) => candidate.peer_id === targetPeerId,
    );
    const peerState = {
      peer_id: targetPeerId,
      trusted: entry?.trust_state === "trusted",
      active: entry?.is_present ?? false,
    };
    const syncKey = `${targetPeerId}\u0000${peerState.trusted}\u0000${peerState.active}`;
    if (syncKey === peerStateSyncKey && peerStateSyncPromise !== null) {
      return peerStateSyncPromise;
    }
    peerStateSyncKey = syncKey;
    thumbnailPeerStateReady = false;
    const generation = ++peerStateSyncGeneration;
    peerStateSyncPromise = Promise.all([
      peerHistoryRecordStateCommand(peerState),
      peerImportRecordStateCommand(peerState),
      peerImageRecordStateCommand(peerState),
      peerImageImportRecordStateCommand(peerState),
      peerImageThumbnailRecordStateCommand(peerState),
    ])
      .then(() => {
        if (generation === peerStateSyncGeneration && peerId === targetPeerId) {
          thumbnailPeerStateReady = true;
        }
      })
      .catch((error: unknown) => {
        if (generation === peerStateSyncGeneration) {
          peerStateSyncKey = null;
          peerStateSyncPromise = null;
        }
        throw error;
      });
    return peerStateSyncPromise;
  }

  // Keep the thumbnail client's in-memory trust/presence gate aligned with
  // snapshot refreshes while the rail remains open. The signature dedupe in
  // refreshPeerState avoids issuing Tauri commands when only unrelated peer
  // metadata changed.
  $: if (peerId !== null) {
    const currentPeer = (snapshot?.entries ?? []).find(
      (candidate) => candidate.peer_id === peerId,
    );
    const observedState = `${currentPeer?.trust_state ?? "unknown"}\u0000${currentPeer?.is_present ?? false}`;
    void observedState;
    void refreshPeerState(peerId).catch(() => undefined);
  } else {
    peerStateSyncGeneration += 1;
    peerStateSyncKey = null;
    peerStateSyncPromise = null;
    thumbnailPeerStateReady = false;
  }

  /**
   * Populate the runtime's local trust/presence gate before the first browse.
   * The history and import cache writes are independent, but this function
   * awaits both before browse can run. The generation check retains the
   * stale-response protection when the user changes selection while the state
   * sync is in flight.
   */
  async function loadInitialPage(
    targetPeerId: string,
    generation: number,
  ): Promise<void> {
    try {
      await refreshPeerState(targetPeerId);
    } catch (err) {
      if (generation !== loadGeneration || peerId !== targetPeerId) return;
      loading = false;
      error = err instanceof Error ? err.message : String(err);
      persistCache();
      return;
    }
    if (
      generation !== loadGeneration ||
      peerId !== targetPeerId ||
      railShouldShowUnavailable
    ) {
      return;
    }
    // The initial load MUST hit both endpoints with the empty
    // cursor so the bridge decodes each into a "start from the
    // beginning" request. Passing `null` for either stream would
    // collapse the call to `Promise.resolve({ kind: "text-skip" })`
    // and leave the rail empty until the user pressed Siguiente.
    await requestPage(INITIAL_CURSORS, { append: false });
  }

  /**
   * Drive a single page request through the typed bridge. The
   * helper captures the current `loadGeneration` so a response
   * for the previous peer / previous page is silently dropped.
   *
   * `cursors.text` / `cursors.image` are the per-stream cursors
   * the rail forwards verbatim to each endpoint. Both streams
   * MUST receive their own cursor so a mixed-collections row set
   * (text exhausted, image still paging, …) never trips the
   * contract that the spec scenario "Pagination: streams are
   * independent" pins. A `null` cursor skips the matching
   * endpoint entirely so the runtime never re-requests the first
   * page from an exhausted stream while the other still has rows.
   */
  async function requestPage(
    cursors: { text: string | null; image: string | null },
    options: { append: boolean },
  ): Promise<void> {
    if (peerId === null) return;
    const generation = loadGeneration;
    const targetPeer = peerId;
    if (!options.append) {
      // Replace mode: drop the cursor + cache state so the
      // helper exposes the loading state immediately and the
      // user sees the placeholder disappear.
      cursor = cursors.text ?? "";
      imageCursor = cursors.image ?? "";
      rows = [];
      textBuffer = [];
      imageBuffer = [];
      exhausted = false;
    }
    loading = true;
    error = null;
    persistCache();
    const textCursorPayload = cursors.text;
    const imageCursorPayload = cursors.image;
    const textPromise =
      textCursorPayload === null
        ? Promise.resolve({ kind: "text-skip" as const })
        : peerHistoryBrowseCommand({
            peer_id: targetPeer,
            cursor: textCursorPayload,
          }).then(
            (response) => ({ kind: "text" as const, response }),
            (err) => ({ kind: "text-error" as const, err }),
          );
    const imagePromise =
      imageCursorPayload === null
        ? Promise.resolve({ kind: "image-skip" as const })
        : peerImageBrowseCommand({
            peer_id: targetPeer,
            cursor: imageCursorPayload,
          }).then(
            (response) => ({ kind: "image" as const, response }),
            (err) => ({ kind: "image-error" as const, err }),
          );
    const [textOutcome, imageOutcome] = await Promise.all([textPromise, imagePromise]);
    if (generation !== loadGeneration) return;
    consumeMergeResult(
      mergeResponses(textOutcome, imageOutcome, snapshotState(), options),
    );
  }

  function snapshotState(): PageState {
    return {
      rows,
      cursor,
      imageCursor,
      textBuffer,
      imageBuffer,
      snapshotId,
      error,
      loading,
      exhausted,
      requestInvalidCursor: false,
    };
  }

  function consumeMergeResult(next: PageState): void {
    rows = next.rows;
    cursor = next.cursor;
    imageCursor = next.imageCursor;
    textBuffer = next.textBuffer;
    imageBuffer = next.imageBuffer;
    snapshotId = next.snapshotId;
    error = next.error;
    loading = next.loading;
    exhausted = next.exhausted;
    persistCache();
    if (next.requestInvalidCursor) {
      // Surface the typed reason the merge helper computed and
      // ask for the first page so the rail refetches from
      // scratch. The retry MUST use the empty cursor so both
      // endpoints are dialled with "start from the beginning".
      void requestFirstPage();
    }
  }

  function requestFirstPage(): void {
    // The bridge collapses an empty cursor into "start from the
    // beginning", so a retry after an `invalid_cursor` outcome
    // re-issues the first-page request through BOTH endpoints.
    // Passing `null` for either stream would skip the matching
    // endpoint and leave the rail empty.
    void requestPage(INITIAL_CURSORS, { append: false });
  }

  function requestNextPage(): void {
    if (peerId === null) return;
    if (exhausted) return;
    // The text + image streams are paginated independently.
    // "Siguiente" first drains the per-stream buffers the previous
    // response left hidden (this is the spec scenario
    // "Per-stream buffers surface before the next host request"):
    // those rows are already known to the client, so promoting
    // them is synchronous and keeps the user advancing even when
    // both cursors are empty. Only when both buffers are empty do
    // we ask the host for the next page via `pickNextCursors`.
    // If both buffers AND both cursors are empty, the rail is
    // genuinely exhausted and the helper flips `exhausted = true`
    // so the button stays disabled.
    if (textBuffer.length > 0 || imageBuffer.length > 0) {
      consumeMergeResult(promoteBuffers(snapshotState()));
      return;
    }
    if (cursor.length === 0 && imageCursor.length === 0) {
      // No buffered rows AND no cursors: the helper already
      // pinned `exhausted = true` in `applyResponses`. The
      // explicit assignment keeps the invariant robust against
      // an out-of-band mutation that bypasses the helper.
      exhausted = true;
      persistCache();
      return;
    }
    const cursors = pickNextCursors(snapshotState());
    void requestPage(cursors, { append: true });
  }

  function requestPreviousPage(): void {
    // The bridge exposes a newest-first cursor — there is no
    // "previous" direction in the metadata-only contract, so
    // the rail always re-issues the first-page request and the
    // user keeps the option to scroll down again. The empty
    // cursor forces both endpoints to be dialled with
    // `start_from_beginning` semantics; passing `null` would
    // skip each endpoint and leave the rail blank.
    void requestPage(INITIAL_CURSORS, { append: false });
  }

  function persistCache(): void {
    if (peerId === null) return;
    pageCache.set(peerId, snapshotState());
  }

  /**
   * Drop the in-memory cache the moment the parent flips
   * `peerId` to `null`. The runtime keeps its own in-memory
   * trust / active cache; the rail cache is purely a UX
   * optimisation so a quick back-and-forth between two peers
   * does not pay a round-trip. The helper is invoked through the
   * reactive block above when the parent tears the rail down.
   */
  function forgetRail(): void {
    loadGeneration += 1;
    rows = [];
    cursor = "";
    snapshotId = "";
    textBuffer = [];
    imageBuffer = [];
    loading = false;
    error = null;
    exhausted = false;
    activePeerId = null;
    onClose();
  }

  onDestroy(() => {
    loadGeneration += 1;
  });

  /**
   * Forget the peer cache when the user destroys the rail (e.g.
   * deselects the collection row). The runtime keeps the
   * trust / active cache independently.
   */
  $: if (peerId === null) {
    pageCache.clear();
  }
</script>

<section
  class="remote-history-rail"
  data-testid="remote-history-rail"
  aria-label="Historial remoto"
>
  <header class="remote-history-rail-header">
    <h2 data-testid="remote-history-rail-title">
      {#if peerId !== null}
        {activeEntry?.display_name ?? peerId}
      {/if}
    </h2>
    <!--
      No hay botón `Volver`. Volver ocurre al seleccionar
      `Historial` o una colección local en el sidebar — el padre
      limpia `activePeerId` a través de
      `selectCollectionFromSidebar` y la rail local vuelve a
      renderizar. Mantener un botón explícito duplicaba el flujo
      y rompía el contrato que el design documenta.
    -->
  </header>

  {#if peerId === null}
    <p class="remote-history-rail-empty" data-testid="remote-history-rail-empty">
      Selecciona un equipo vinculado para ver su historial.
    </p>
  {:else if railShouldShowUnavailable}
    <div
      class="remote-history-rail-unavailable"
      data-testid="remote-history-rail-unavailable"
    >
      <p>
        {#if !activeIsTrusted}
          El equipo aún no está vinculado.
        {:else}
          El equipo no está disponible en este momento.
        {/if}
      </p>
    </div>
  {:else if loading && rows.length === 0}
    <p class="remote-history-rail-loading" data-testid="remote-history-rail-loading">
      Cargando historial…
    </p>
  {:else if error && rows.length === 0}
    <div
      class="remote-history-rail-error"
      role="alert"
      data-testid="remote-history-rail-error"
    >
      <p>No se pudo cargar el historial: {error}.</p>
      <button
        type="button"
        class="remote-history-rail-retry"
        data-testid="remote-history-rail-retry"
        on:click={requestFirstPage}
      >
        Reintentar
      </button>
    </div>
  {:else if rows.length === 0}
    <p class="remote-history-rail-empty" data-testid="remote-history-rail-empty">
      Este equipo no tiene capturas transferibles.
    </p>
  {:else}
    <div
      class="remote-history-rail-toolbar"
      data-testid="remote-history-rail-toolbar"
    >
      <button
        type="button"
        class="remote-history-rail-pager"
        data-testid="remote-history-rail-prev"
        on:click={requestPreviousPage}
        disabled={rows.length === 0}
      >
        Anterior
      </button>
      <button
        type="button"
        class="remote-history-rail-pager"
        data-testid="remote-history-rail-next"
        on:click={requestNextPage}
        disabled={exhausted}
      >
        Siguiente
      </button>
      <span
        class="remote-history-rail-snapshot"
        data-testid="remote-history-rail-snapshot"
        title="Huella estable del historial remoto"
      >
        {rows.length} captura{rows.length === 1 ? "" : "s"}
      </span>
    </div>
    {#if error}
      <p
        class="remote-history-rail-error-inline"
        role="alert"
        data-testid="remote-history-rail-error-inline"
      >
        Error: {error}
      </p>
    {/if}
    <div
      class="remote-history-rail-cards"
      data-testid="remote-history-rail-cards"
      role="list"
    >
      {#each rows as item, index (remoteImageThumbnailCardKey(peerId, item.row.remote_entry_id))}
        <div
          role="listitem"
          class="remote-history-rail-card-slot"
          data-testid="remote-history-rail-card-slot"
          data-remote-entry-id={item.row.remote_entry_id}
          data-row-kind={item.kind}
        >
          {#if item.kind === "text"}
            <RemotePreviewCard
              row={item.row}
              rowTestId={`remote-history-rail-card-${index}`}
              peerId={peerId}
              displayName={activeEntry?.display_name ?? null}
              peerCapability={activePeerCapability}
            />
          {:else}
            <RemotePreviewCard
              row={{
                remote_entry_id: item.row.remote_entry_id,
                title: item.row.title,
                content_type: item.row.content_type,
                created_at: item.row.created_at,
                preview: `${item.row.width}×${item.row.height} · ${Math.round(item.row.byte_size / 1024)} KB`,
              }}
              rowTestId={`remote-history-rail-card-${index}`}
              peerId={peerId}
              displayName={activeEntry?.display_name ?? null}
              isImageRow={true}
              peerCapability={activePeerCapability}
              peerStateReady={thumbnailPeerStateReady}
            />
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</section>

<style>
  .remote-history-rail {
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
    padding: 0.75rem 0;
  }
  .remote-history-rail-header {
    display: flex;
    align-items: center;
    gap: 0.65rem;
  }
  .remote-history-rail-header h2 {
    margin: 0;
    font-size: 1rem;
    flex: 1 1 auto;
  }
  .remote-history-rail-empty,
  .remote-history-rail-loading {
    margin: 0;
    color: var(--cv-fg-muted, #94a3b8);
    font-size: 0.85rem;
  }
  .remote-history-rail-unavailable,
  .remote-history-rail-error {
    border: 1px solid rgba(248, 113, 113, 0.45);
    background: rgba(248, 113, 113, 0.12);
    border-radius: 8px;
    padding: 0.65rem 0.8rem;
    color: #fee2e2;
    font-size: 0.85rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .remote-history-rail-error p,
  .remote-history-rail-unavailable p {
    margin: 0;
  }
  .remote-history-rail-retry {
    align-self: flex-start;
    background: transparent;
    border: 1px solid var(--cv-border, #30363d);
    border-radius: 6px;
    padding: 0.3rem 0.7rem;
    cursor: pointer;
    color: inherit;
    font: inherit;
  }
  .remote-history-rail-toolbar {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .remote-history-rail-pager {
    background: transparent;
    border: 1px solid var(--cv-border, #30363d);
    border-radius: 6px;
    padding: 0.3rem 0.7rem;
    cursor: pointer;
    color: inherit;
    font: inherit;
  }
  .remote-history-rail-pager:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .remote-history-rail-snapshot {
    margin-left: auto;
    font-size: 0.75rem;
    color: var(--cv-fg-muted, #94a3b8);
  }
  .remote-history-rail-error-inline {
    margin: 0;
    font-size: 0.8rem;
    color: #fee2e2;
  }
  .remote-history-rail-cards {
    display: flex;
    flex-direction: row;
    gap: 0.65rem;
    overflow-x: auto;
    overflow-y: hidden;
    padding-bottom: 0.5rem;
  }
  .remote-history-rail-card-slot {
    display: contents;
  }
</style>
