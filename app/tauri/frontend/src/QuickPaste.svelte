<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import type {
    EntryRecord,
    PasteResponse,
    PlatformGuidance,
    SearchHit,
    SearchResponse,
  } from "./types";
  import {
    pasteEntryCommand,
    recentEntriesCommand,
    searchEntriesCommand,
  } from "./lib/tauri";
  import { runSearch } from "./lib/search";
  import {
    QUICK_PASTE_OPENED_EVENT,
    QUICK_PASTE_WINDOW_LABEL,
    hideQuickPasteWindow,
  } from "./lib/quickPasteBridge";
  import { performPasteFlow } from "./lib/quickPasteController";
  import { listen } from "@tauri-apps/api/event";
  import { contentTypeLabel } from "./lib/contentType";
  import {
    createClipboardAssetResolver,
    entryPreviewText,
    hasRenderableImage,
    isImageEntry,
  } from "./lib/clipboardAsset";
  import type { IconResolver } from "./lib/iconResolver";
  import PlatformGuidanceModal from "./PlatformGuidanceModal.svelte";

  type Mode = "idle" | "recent" | "search";

  let query = "";
  let mode: Mode = "idle";
  let recent: EntryRecord[] = [];
  let hits: SearchHit[] = [];
  let searching = false;
  let searchError: string | null = null;
  let selectedIndex = 0;
  let loading = true;
  let emptyMessage = "";
  let guidance: PlatformGuidance | null = null;
  let pasteError: string | null = null;
  let quickPasteController: { cancel: () => void } | null = null;

  // List of entry ids actually rendered (depends on mode). We keep a
  // separate `resultIds` so keyboard navigation can clamp the
  // selection without reaching into the search-response shape.
  $: resultIds = computeResultIds(mode, recent, hits);
  $: visibleEmpty = computeEmpty(mode, recent, hits, searching, searchError);
  $: emptyMessage = computeEmptyMessage(
    mode,
    recent,
    hits,
    searching,
    searchError,
    query,
  );

  function computeResultIds(
    currentMode: Mode,
    recents: EntryRecord[],
    searchHits: SearchHit[],
  ): number[] {
    if (currentMode === "search") {
      return searchHits.map((hit) => hit.entry_id);
    }
    return recents.map((entry) => entry.id);
  }

  function computeEmpty(
    currentMode: Mode,
    recents: EntryRecord[],
    searchHits: SearchHit[],
    isSearching: boolean,
    error: string | null,
  ): boolean {
    if (error !== null) return false;
    if (isSearching) return false;
    if (currentMode === "search") {
      return searchHits.length === 0;
    }
    return recents.length === 0;
  }

  function computeEmptyMessage(
    currentMode: Mode,
    recents: EntryRecord[],
    searchHits: SearchHit[],
    isSearching: boolean,
    error: string | null,
    currentQuery: string,
  ): string {
    if (error !== null) return "";
    if (isSearching) return "Searching…";
    if (currentMode === "search") {
      if (searchHits.length === 0) {
        return `No matches for “${currentQuery}”.`;
      }
      return "";
    }
    if (recents.length === 0) {
      return "No clipboard history yet. Copy something and try again.";
    }
    return "";
  }

  async function loadRecent(): Promise<void> {
    try {
      recent = await recentEntriesCommand({ limit: 50 });
      if (mode === "idle" || mode === "recent") {
        mode = "recent";
      }
      loading = false;
      clampSelection();
    } catch (err) {
      searchError = err instanceof Error ? err.message : String(err);
      loading = false;
    }
  }

  async function runQuery(value: string): Promise<void> {
    query = value;
    if (value.trim().length === 0) {
      if (quickPasteController) {
        quickPasteController.cancel();
        quickPasteController = null;
      }
      searching = false;
      searchError = null;
      hits = [];
      mode = "recent";
      clampSelection();
      return;
    }
    mode = "search";
    if (quickPasteController) {
      quickPasteController.cancel();
      quickPasteController = null;
    }
    searching = true;
    searchError = null;
    const controller = runSearch({
      query: value,
      debounceMs: 80,
      invoke: async (q) => {
        const response: SearchResponse = await searchEntriesCommand({
          query: q,
          limit: 50,
        });
        return response;
      },
    });
    quickPasteController = controller;
    try {
      const response = await controller.result;
      if (quickPasteController !== controller) {
        return;
      }
      hits = response.hits;
      clampSelection();
    } catch (err) {
      if (quickPasteController === controller) {
        searchError = err instanceof Error ? err.message : String(err);
      }
    } finally {
      if (quickPasteController === controller) {
        searching = false;
        quickPasteController = null;
      }
    }
  }

  function clampSelection(): void {
    const total = resultIds.length;
    if (total === 0) {
      selectedIndex = 0;
      return;
    }
    if (selectedIndex >= total) {
      selectedIndex = total - 1;
    }
  }

  function renderSnippet(
    currentMode: Mode,
    recents: EntryRecord[],
    searchHits: SearchHit[],
    id: number,
  ): string {
    const entry = findEntry(currentMode, recents, searchHits, id);
    // An image row has no textual payload; showing the backend's empty
    // `content` sentinel would render a blank row, so the shared helper
    // returns a localised placeholder with the known dimensions instead.
    if (entry && isImageEntry(entry)) {
      return entryPreviewText(entry);
    }
    if (currentMode === "search") {
      const hit = searchHits.find((h) => h.entry_id === id);
      if (hit) return hit.snippet;
    } else if (entry) {
      return entryPreviewText(entry, 80);
    }
    return "";
  }

  /**
   * Resolve the full record behind a rendered row id, in either mode.
   * Quick-paste needs the record (not just the snippet) so it can
   * recognise an image entry and request its thumbnail.
   */
  function findEntry(
    currentMode: Mode,
    recents: EntryRecord[],
    searchHits: SearchHit[],
    id: number,
  ): EntryRecord | null {
    if (currentMode === "search") {
      return searchHits.find((hit) => hit.entry_id === id)?.record ?? null;
    }
    return recents.find((entry) => entry.id === id) ?? null;
  }

  // ---------------------------------------------------------------
  // Image thumbnails in the quick-paste list.
  //
  // The list reuses the same validated asset bridge and blob-URL
  // lifecycle as `HistoryCard`. Resolutions are stored per entry id so
  // the `{#each}` block stays synchronous; every URL is revoked in
  // `onDestroy`.
  // ---------------------------------------------------------------

  const assetResolver: IconResolver = createClipboardAssetResolver();
  let thumbnails: Record<number, string> = {};

  async function loadThumbnail(entry: EntryRecord): Promise<void> {
    if (!hasRenderableImage(entry) || !entry.asset_ref) return;
    if (thumbnails[entry.id]) return;
    const resolution = await assetResolver.resolve(entry.asset_ref);
    if (resolution.ok && resolution.url) {
      thumbnails = { ...thumbnails, [entry.id]: resolution.url };
    }
  }

  /**
   * Kick off the thumbnail loads for whatever the list currently
   * renders. Called reactively; `loadThumbnail` short-circuits on an
   * already-resolved id so a re-render never re-requests bytes.
   */
  function syncThumbnails(
    currentMode: Mode,
    recents: EntryRecord[],
    searchHits: SearchHit[],
  ): void {
    const entries =
      currentMode === "search" ? searchHits.map((hit) => hit.record) : recents;
    for (const entry of entries) {
      void loadThumbnail(entry);
    }
  }

  $: syncThumbnails(mode, recent, hits);

  function dropThumbnail(id: number): void {
    const url = thumbnails[id];
    if (!url) return;
    const { [id]: _removed, ...rest } = thumbnails;
    thumbnails = rest;
  }

  function resolveContentType(
    currentMode: Mode,
    recents: EntryRecord[],
    searchHits: SearchHit[],
    id: number,
  ): string {
    if (currentMode === "search") {
      const hit = searchHits.find((h) => h.entry_id === id);
      if (hit) return hit.record.content_type;
    } else {
      const entry = recents.find((e) => e.id === id);
      if (entry) return entry.content_type;
    }
    return "text";
  }

  function moveSelection(delta: number): void {
    const total = resultIds.length;
    if (total === 0) return;
    const next = (selectedIndex + delta + total) % total;
    selectedIndex = next;
  }

  function jumpToFirst(): void {
    if (resultIds.length === 0) return;
    selectedIndex = 0;
  }

  function jumpToLast(): void {
    if (resultIds.length === 0) return;
    selectedIndex = resultIds.length - 1;
  }

  async function handleEnter(): Promise<void> {
    if (selectedIndex < 0 || selectedIndex >= resultIds.length) {
      // No selection: do NOT invoke the paste command. The contract
      // is "Enter without selection must not paste".
      return;
    }
    const entryId = resultIds[selectedIndex];
    if (entryId == null) return;
    guidance = null;
    pasteError = null;
    try {
      const outcome = await performPasteFlow({
        bridge: {
          captureActiveApp: async () => ({
            available: false,
            name: null,
            identifier: null,
          }),
          show: async () => undefined,
          focus: async () => undefined,
          emitOpened: async () => undefined,
          hide: hideQuickPasteWindow,
        },
        pasteFn: () => pasteEntryCommand({ id: entryId }),
      });
      if (outcome.kind === "pasted") {
        // Window stays hidden by contract; nothing to do here.
        return;
      }
      // failure path: window has been re-shown by the controller.
      guidance = readGuidance(outcome.response);
      pasteError =
        "error" in outcome.response
          ? outcome.response.error
          : readMessage(outcome.response);
    } catch (error) {
      // Should not happen because performPasteFlow swallows the error,
      // but be defensive in case the helper itself throws.
      pasteError = error instanceof Error ? error.message : String(error);
    }
  }

  function readGuidance(
    response: PasteResponse | { error: string },
  ): PlatformGuidance | null {
    if ("guidance" in response && response.guidance) {
      return response.guidance;
    }
    return null;
  }

  function readMessage(response: PasteResponse): string {
    return response.message ?? `Paste ${response.kind}`;
  }

  async function handleEscape(): Promise<void> {
    if (quickPasteController) {
      quickPasteController.cancel();
      quickPasteController = null;
    }
    try {
      await hideQuickPasteWindow();
    } catch {
      // Best-effort: hiding is idempotent. Failure is non-fatal.
    }
  }

  function closeGuidance(): void {
    guidance = null;
    pasteError = null;
  }

  function onWindowKeydown(event: KeyboardEvent): void {
    if (guidance) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      moveSelection(1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      moveSelection(-1);
    } else if (event.key === "Home") {
      event.preventDefault();
      jumpToFirst();
    } else if (event.key === "End") {
      event.preventDefault();
      jumpToLast();
    } else if (event.key === "Enter") {
      // Only handle Enter when the focus is on the search input or
      // the list, so typing in another input does not trigger a
      // paste by accident.
      const target = event.target as HTMLElement | null;
      if (
        target &&
        target !== document.body &&
        target.tagName !== "INPUT" &&
        target.tagName !== "TEXTAREA"
      ) {
        return;
      }
      event.preventDefault();
      void handleEnter();
    } else if (event.key === "Escape") {
      event.preventDefault();
      void handleEscape();
    }
  }

  function onInput(event: Event): void {
    const value = (event.currentTarget as HTMLInputElement).value;
    void runQuery(value);
  }

  function onQuickPasteOpened(): void {
    // Reset the visible state so the user sees the recent list when
    // the hotkey fires. The bridge only emits a null payload, so we
    // intentionally do not look at the event details.
    selectedIndex = 0;
    query = "";
    void loadRecent();
  }

  // The Tauri event API is only available inside the Tauri runtime;
  // guard the subscription so non-Tauri callers (tests, Vite dev
  // preview) do not crash.
  function safeListenOpened(handler: () => void): () => void {
    if (!window.__TAURI_INTERNALS__) return () => undefined;
    let unlisten: (() => void) | null = null;
    void listen(QUICK_PASTE_OPENED_EVENT, () => {
      try {
        handler();
      } catch (error) {
        console.error("quick-paste opened handler threw", error);
      }
    }, { target: QUICK_PASTE_WINDOW_LABEL })
      .then((stop) => {
        unlisten = stop;
      })
      .catch((error) => {
        console.error("failed to listen for quick-paste-opened", error);
      });
    return () => {
      if (unlisten) unlisten();
    };
  }

  let unlistenOpened: (() => void) | null = null;

  onMount(() => {
    void loadRecent();
    unlistenOpened = safeListenOpened(onQuickPasteOpened);
  });

  onDestroy(() => {
    if (quickPasteController) {
      quickPasteController.cancel();
      quickPasteController = null;
    }
    if (unlistenOpened) {
      unlistenOpened();
      unlistenOpened = null;
    }
    // Revoke every thumbnail blob URL the list minted.
    assetResolver.release();
    thumbnails = {};
  });
</script>

<svelte:window on:keydown={onWindowKeydown} />

<main data-testid="quick-paste-root">
  <header>
    <h1>ClipVault Quick Paste</h1>
  </header>

  <input
    type="search"
    placeholder="Search the clipboard history"
    value={query}
    on:input={onInput}
    aria-label="Search the clipboard history"
    data-testid="quick-paste-input"
  />

  {#if loading}
    <p class="status" data-testid="quick-paste-loading">Loading history…</p>
  {:else if searchError}
    <p class="status error" role="alert" data-testid="quick-paste-error">
      {searchError}
    </p>
  {:else if visibleEmpty}
    <p class="status muted" data-testid="quick-paste-empty">
      {emptyMessage}
    </p>
  {:else if pasteError}
    <p class="status error" role="alert" data-testid="quick-paste-paste-error">
      {pasteError}
    </p>
  {:else}
    <ul class="entries" data-testid="quick-paste-results">
      {#each resultIds as id, index (id)}
        <li
          class:active={index === selectedIndex}
          data-entry-id={id}
          data-testid="quick-paste-row"
          data-selected={index === selectedIndex ? "true" : "false"}
        >
          {#if thumbnails[id]}
            <img
              class="row-thumbnail"
              src={thumbnails[id]}
              alt=""
              aria-hidden="true"
              data-testid="quick-paste-thumbnail"
              on:error={() => dropThumbnail(id)}
            />
          {/if}
          <code>{renderSnippet(mode, recent, hits, id)}</code>
          <span
            class="type-badge"
            data-testid="quick-paste-type"
            data-content-type={resolveContentType(mode, recent, hits, id)}
            aria-label="Tipo: {contentTypeLabel(resolveContentType(mode, recent, hits, id))}"
          >
            {contentTypeLabel(resolveContentType(mode, recent, hits, id))}
          </span>
          <span class="muted">#{id}</span>
        </li>
      {/each}
    </ul>
  {/if}
</main>

{#if guidance}
  <PlatformGuidanceModal
    {guidance}
    retryNotice={null}
    retryError={null}
    onClose={closeGuidance}
    onRetry={() => Promise.resolve()}
  />
{/if}

<style>
  :global(html, body) {
    margin: 0;
    padding: 0;
    background: #0e1116;
    color: #f0f4f8;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui,
      sans-serif;
  }

  main {
    padding: 1rem 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    height: 100vh;
    box-sizing: border-box;
  }

  header h1 {
    margin: 0;
    font-size: 1rem;
    color: #94a3b8;
    font-weight: 600;
  }

  input[type="search"] {
    width: 100%;
    padding: 0.5rem 0.75rem;
    background: #161b22;
    color: inherit;
    border: 1px solid #30363d;
    border-radius: 6px;
    font-family: inherit;
    font-size: 0.95rem;
    box-sizing: border-box;
  }

  input[type="search"]:focus {
    outline: none;
    border-color: #2563eb;
  }

  ul.entries {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    flex: 1 1 auto;
  }

  ul.entries li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.75rem;
    padding: 0.4rem 0.5rem;
    border-radius: 4px;
    border-top: 1px solid #1f2937;
  }

  /*
   * A bounded thumbnail so an image entry is recognisable at a glance
   * without changing the row height or the keyboard navigation model.
   */
  .row-thumbnail {
    flex: 0 0 auto;
    width: 2rem;
    height: 2rem;
    border-radius: 4px;
    object-fit: cover;
    background: rgba(255, 255, 255, 0.06);
  }

  ul.entries li.active {
    background: #1d4ed8;
    color: white;
  }

  ul.entries li.active .muted {
    color: rgba(255, 255, 255, 0.85);
  }

  .type-badge {
    display: inline-block;
    padding: 0.1rem 0.45rem;
    border-radius: 999px;
    font-size: 0.7rem;
    font-weight: 600;
    letter-spacing: 0.02em;
    background: #1f2937;
    color: #93c5fd;
    border: 1px solid #30363d;
    white-space: nowrap;
  }

  ul.entries li.active .type-badge {
    background: rgba(255, 255, 255, 0.15);
    color: white;
    border-color: rgba(255, 255, 255, 0.3);
  }

  .status {
    font-style: italic;
  }

  .status.error {
    color: #f87171;
    font-style: normal;
  }

  .muted {
    color: #94a3b8;
  }

  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }
</style>
