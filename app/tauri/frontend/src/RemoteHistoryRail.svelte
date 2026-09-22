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
  } from "./lib/tauri";
  import type {
    PeerHistoryBrowseResponse,
    PeerHistoryRow,
    PeerSnapshot,
  } from "./types";
  import RemotePreviewCard from "./RemotePreviewCard.svelte";

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
  type PageState = {
    rows: PeerHistoryRow[];
    cursor: string;
    snapshotId: string;
    loading: boolean;
    error: string | null;
    /** True when the host reported no more pages. */
    exhausted: boolean;
  };
  const pageCache = new Map<string, PageState>();

  /**
   * Active page state the rail renders. The reactive variable
   * is always a fresh shallow copy so Svelte picks up the
   * mutations the async responses cause.
   */
  let rows: PeerHistoryRow[] = [];
  let cursor: string = "";
  let snapshotId: string = "";
  let loading = false;
  let error: string | null = null;
  let exhausted = false;
  let activePeerId: string | null = peerId;

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
    error = null;
    if (peerId === null) {
      forgetRail();
    } else if (railShouldShowUnavailable) {
      rows = [];
      cursor = "";
      snapshotId = "";
      loading = false;
      error = null;
      exhausted = false;
      void refreshPeerState();
    } else {
      const cached = pageCache.get(peerId);
      if (cached) {
        rows = cached.rows;
        cursor = cached.cursor;
        snapshotId = cached.snapshotId;
        loading = cached.loading;
        error = cached.error;
        exhausted = cached.exhausted;
      } else {
        rows = [];
        cursor = "";
        snapshotId = "";
        loading = false;
        error = null;
        exhausted = false;
        void refreshPeerState();
        void requestFirstPage();
      }
    }
  }

  /**
   * Best-effort sync hook that keeps the runtime trust /
   * active cache aligned with the latest snapshot. The runtime
   * uses the cache to decide whether to project the page
   * request; calling the hook every time the parent supplies a
   * fresh snapshot guarantees a stale `trust_state` flip can
   * never resurrect a revoked peer.
   */
  function refreshPeerState(): void {
    if (peerId === null) return;
    const entry = activeEntry;
    void peerHistoryRecordStateCommand({
      peer_id: peerId,
      trusted: entry?.trust_state === "trusted",
      active: entry?.is_present ?? false,
    });
  }

  /**
   * Drive a single page request through the typed bridge. The
   * helper captures the current `loadGeneration` so a response
   * for the previous peer / previous page is silently dropped.
   */
  async function requestPage(
    nextCursor: string | null,
    options: { append: boolean },
  ): Promise<void> {
    if (peerId === null) return;
    const generation = loadGeneration;
    const targetPeer = peerId;
    if (!options.append) {
      // Replace mode: drop the cursor + cache state so the
      // helper exposes the loading state immediately and the
      // user sees the placeholder disappear.
      cursor = nextCursor ?? "";
      rows = [];
      exhausted = false;
    }
    loading = true;
    error = null;
    persistCache();
    let response: PeerHistoryBrowseResponse;
    try {
      response = await peerHistoryBrowseCommand({
        peer_id: targetPeer,
        cursor: nextCursor ?? "",
      });
    } catch (err) {
      if (generation !== loadGeneration) return;
      loading = false;
      error = err instanceof Error ? err.message : String(err);
      persistCache();
      return;
    }
    if (generation !== loadGeneration) return;
    applyResponse(response, options);
  }

  function applyResponse(
    response: PeerHistoryBrowseResponse,
    options: { append: boolean },
  ): void {
    if (peerId === null) return;
    switch (response.kind) {
      case "ok": {
        const nextRows = options.append ? rows.concat(response.rows) : response.rows;
        rows = nextRows;
        cursor = response.next_cursor ?? "";
        snapshotId = response.snapshot_id;
        loading = false;
        error = null;
        exhausted = response.next_cursor.length === 0 || response.rows.length === 0;
        persistCache();
        break;
      }
      case "invalid_cursor": {
        // The cursor the renderer submitted is no longer valid;
        // the safest action is to clear the cursor and refetch
        // the first page so the user does not stay stuck on a
        // placeholder that no longer matches the host.
        loading = false;
        error = "invalid_cursor";
        cursor = "";
        exhausted = false;
        rows = [];
        persistCache();
        void requestFirstPage();
        break;
      }
      case "peer_unavailable": {
        loading = false;
        error = `peer_unavailable:${response.reason}`;
        rows = [];
        cursor = "";
        snapshotId = "";
        exhausted = true;
        persistCache();
        break;
      }
      case "persistence_unavailable": {
        loading = false;
        error = "persistence_unavailable";
        persistCache();
        break;
      }
    }
  }

  function requestFirstPage(): void {
    void requestPage(null, { append: false });
  }

  function requestNextPage(): void {
    if (peerId === null) return;
    if (cursor.length === 0) return;
    if (exhausted) return;
    void requestPage(cursor, { append: true });
  }

  function requestPreviousPage(): void {
    // The bridge exposes a newest-first cursor — there is no
    // "previous" direction in the metadata-only contract, so
    // the rail always re-issues the first-page request and the
    // user keeps the option to scroll down again.
    void requestPage(null, { append: false });
  }

  function persistCache(): void {
    if (peerId === null) return;
    pageCache.set(peerId, {
      rows,
      cursor,
      snapshotId,
      loading,
      error,
      exhausted,
    });
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
        disabled={exhausted || cursor.length === 0}
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
      {#each rows as row, index (row.remote_entry_id)}
        <div
          role="listitem"
          class="remote-history-rail-card-slot"
          data-testid="remote-history-rail-card-slot"
          data-remote-entry-id={row.remote_entry_id}
        >
          <RemotePreviewCard {row} rowTestId={`remote-history-rail-card-${index}`} />
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
