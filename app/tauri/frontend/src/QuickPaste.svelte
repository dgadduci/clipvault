<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import type {
    CopyResponse,
    EntryRecord,
    PasteResponse,
    PlatformGuidance,
    SearchHit,
    SearchResponse,
  } from "./types";
  import {
    copyEntryCommand,
    pasteEntryCommand,
    recentEntriesCommand,
    searchEntriesCommand,
    setFavoriteCommand,
  } from "./lib/tauri";
  import { runSearch } from "./lib/search";
  import {
    QUICK_PASTE_OPENED_EVENT,
    QUICK_PASTE_WINDOW_LABEL,
    hideQuickPasteWindow,
  } from "./lib/quickPasteBridge";
  import { performCopyFlow, performPasteFlow } from "./lib/quickPasteController";
  import {
    capabilitiesOf,
    preserveSelectionAfterReorder,
    quickPasteConfirmAction,
    quickPasteMenuActions,
    quickPasteOrderedIds,
    scrollSelectedRowIntoView,
    selectedIndexForClick,
    selectedIndexForEntryId,
    type CopyMode,
  } from "./lib/quickPasteActions";
  import { listen } from "@tauri-apps/api/event";
  import { contentTypeLabel } from "./lib/contentType";
  import {
    contentTypeIconId,
    contentTypeIconLabel,
  } from "./lib/contentTypeIcons";
  import { sourceAppAccessibleLabel } from "./lib/sourceAppFallback";
  import { formatElapsedTime } from "./lib/elapsedTime";
  import {
    createClipboardAssetResolver,
    entryPreviewText,
    hasRenderableImage,
    isImageEntry,
  } from "./lib/clipboardAsset";
  import type { IconResolver } from "./lib/iconResolver";
  import PlatformGuidanceModal from "./PlatformGuidanceModal.svelte";

  type Mode = "idle" | "recent" | "search";
  type ThumbnailState = "loading" | "loaded" | "error";

  let query = "";
  let mode: Mode = "idle";
  let recent: EntryRecord[] = [];
  let hits: SearchHit[] = [];
  let searching = false;
  let searchError: string | null = null;
  let selectedIndex = 0;
  /**
   * Stable id of the selected entry. The list owns `selectedIndex`
   * for rendering and the keyboard clamp, but the id is the single
   * source of truth for "which entry is currently selected" so a
   * search, a favourite toggle or a refresh can keep the selection
   * deterministic even when the candidate list reorders, drops rows
   * or grows. `null` means the selection will be re-derived when the
   * next result list lands.
   */
  let selectedEntryId: number | null = null;
  let loading = true;
  let emptyMessage = "";
  let guidance: PlatformGuidance | null = null;
  let pasteError: string | null = null;
  /**
   * Inline error surfaced after a failed pin/unpin round-trip. The
   * value is rendered through the same status band the paste flow
   * uses so the row state stays in lockstep with the rest of the
   * panel — no separate toast, no second modal, no log noise.
   */
  let pinError: string | null = null;
  let quickPasteController: { cancel: () => void } | null = null;
  /**
   * Id of the row whose `...` menu is currently open. Only one menu
   * can be open at a time; clicking the trigger on a different row
   * replaces the previous menu without leaving the previous listener
   * hanging. `null` means no menu is open.
   */
  let openMenuEntryId: number | null = null;
  /**
   * Entry ids currently driving a paste or pin round-trip. The set
   * is the only switch the menu / keyboard helpers consult so two
   * concurrent activations on the same row are coalesced and a
   * second click on the trigger or the menu item while the round
   * trip is in flight becomes a no-op.
   */
  let pasteInFlight: Set<number> = new Set();
  let pinInFlight: Set<number> = new Set();

  // Fixed dimensions of the result item. The contract documented in
  // `quick-paste-compact-ui/spec.md` pins 72 logical pixels so every
  // entry — text, image, loading, error, selected, hover, focus —
  // shares the same outer rectangle. Any change to the value MUST
  // update the matching CSS rule on `.qp-row` AND the tests under
  // `tests/quickPasteCompact.test.ts`.
  const ROW_HEIGHT_PX = 72;

  // List of entry ids actually rendered (depends on mode). We keep a
  // separate `resultIds` so keyboard navigation can clamp the
  // selection without reaching into the search-response shape. The
  // order helper moves favourites to the top while preserving the
  // ranking the recents / search feed already produced for each
  // group, so the pin toggle and the keyboard navigation stay
  // consistent.
  $: resultIds = quickPasteOrderedIds(mode, recent, hits);
  $: visibleEmpty = computeEmpty(mode, recent, hits, searching, searchError);
  $: emptyMessage = computeEmptyMessage(
    mode,
    recent,
    hits,
    searching,
    searchError,
    query,
  );

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
    if (isSearching) return "Buscando…";
    if (currentMode === "search") {
      if (searchHits.length === 0) {
        return `Sin coincidencias para “${currentQuery}”.`;
      }
      return "";
    }
    if (recents.length === 0) {
      return "Sin historial aún. Copia algo y vuelve a intentarlo.";
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
    // Re-anchor the index on the stable entry id whenever the
    // candidate set changes. If the previously selected id is no
    // longer visible the helper falls back to the previous index
    // (clamped) so a search, favourite toggle or refresh never
    // silently moves the keyboard focus to a stale row.
    selectedIndex = selectedIndexForEntryId(
      resultIds,
      selectedEntryId,
      selectedIndex,
    );
    const currentId = resultIds[selectedIndex];
    selectedEntryId = currentId ?? null;
  }

  // ---------------------------------------------------------------
  // Image thumbnails in the quick-paste list.
  //
  // The list reuses the same validated asset bridge and blob-URL
  // lifecycle as `HistoryCard`. The contract documented in
  // `quick-paste-compact-ui/spec.md` requires:
  //
  //   - the thumbnail occupies a fixed-size square that NEVER changes
  //     shape between `loading`, `loaded`, and `error`;
  //   - while the bridge round-trip is in flight the placeholder
  //     shows the same dimensions as the eventual thumbnail;
  //   - a stale response from a previous entry never overwrites the
  //     current entry's state.
  //
  // The token guard mirrors `HistoryCard.thumbnailToken` so a late
  // resolution from entry A cannot clobber entry B's `loaded` state
  // when the user navigates quickly through the rail.
  // ---------------------------------------------------------------

  const assetResolver: IconResolver = createClipboardAssetResolver();
  let thumbnails: Record<number, string> = {};
  /**
   * Per-entry thumbnail state. Mirrors `HistoryCard`'s three-state
   * machine so the compact UI can branch on `"loading"` without
   * inspecting `thumbnailUrl === null` (which is also the terminal
   * state for a textual entry). The initial value is computed per
   * entry because a coherent image row must never flash the error
   * fallback during the very first paint.
   */
  let thumbnailStates: Record<number, ThumbnailState> = {};
  /**
   * Per-entry token used to discard stale bridge round-trips. The
   * shared counter bumps on every `loadThumbnail` invocation; a
   * resolution that lands after a newer round has been scheduled
   * for a different entry is dropped on the floor so the row
   * never shows another entry's thumbnail.
   */
  let thumbnailToken = 0;
  const thumbnailTokens = new Map<number, number>();

  async function loadThumbnail(entry: EntryRecord): Promise<void> {
    if (!hasRenderableImage(entry) || !entry.asset_ref) return;
    if (thumbnails[entry.id]) return;
    const token = ++thumbnailToken;
    thumbnailTokens.set(entry.id, token);
    if (thumbnailStates[entry.id] !== "loading") {
      thumbnailStates = { ...thumbnailStates, [entry.id]: "loading" };
    }
    const resolution = await assetResolver.resolve(entry.asset_ref);
    if (thumbnailTokens.get(entry.id) !== token) {
      return;
    }
    if (resolution.ok && resolution.url) {
      thumbnails = { ...thumbnails, [entry.id]: resolution.url };
      thumbnailStates = { ...thumbnailStates, [entry.id]: "loaded" };
    } else {
      thumbnailStates = { ...thumbnailStates, [entry.id]: "error" };
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

  function dropThumbnail(id: number, assetRef?: string): void {
    const url = thumbnails[id];
    if (assetRef) {
      assetResolver.releaseFor(assetRef);
    }
    thumbnailTokens.delete(id);
    if (!url) {
      thumbnailStates = { ...thumbnailStates, [id]: "error" };
      return;
    }
    const { [id]: _removed, ...rest } = thumbnails;
    thumbnails = rest;
    thumbnailStates = { ...thumbnailStates, [id]: "error" };
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

  function renderTitle(
    currentMode: Mode,
    recents: EntryRecord[],
    searchHits: SearchHit[],
    id: number,
  ): string {
    const entry = findEntry(currentMode, recents, searchHits, id);
    if (!entry) return "";
    const explicit = entry.title?.trim();
    if (explicit) return explicit;
    return contentTypeLabel(entry.content_type);
  }

  function renderPreview(
    currentMode: Mode,
    recents: EntryRecord[],
    searchHits: SearchHit[],
    id: number,
  ): string {
    const entry = findEntry(currentMode, recents, searchHits, id);
    if (!entry) return "";
    if (isImageEntry(entry)) {
      return entryPreviewText(entry);
    }
    if (currentMode === "search") {
      const hit = searchHits.find((h) => h.entry_id === id);
      if (hit && hit.snippet) return hit.snippet;
    }
    return entryPreviewText(entry, 80);
  }

  function moveSelection(delta: number): void {
    const total = resultIds.length;
    if (total === 0) return;
    const next = (selectedIndex + delta + total) % total;
    selectedIndex = next;
    selectedEntryId = resultIds[next] ?? null;
    scrollSelectedIntoView();
  }

  function jumpToFirst(): void {
    if (resultIds.length === 0) return;
    selectedIndex = 0;
    selectedEntryId = resultIds[0] ?? null;
    scrollSelectedIntoView();
  }

  function jumpToLast(): void {
    if (resultIds.length === 0) return;
    selectedIndex = resultIds.length - 1;
    selectedEntryId = resultIds[resultIds.length - 1] ?? null;
    scrollSelectedIntoView();
  }

  /**
   * Drive the click-selection contract: clicking its non-interactive
   * surface MUST turn a row into the selected result. The helper is
   * kept so unit tests can exercise the selection math without the
   * Svelte renderer; the row's `on:click` handler routes through
   * [`handleRowClick`] so selection and confirmation share one
   * function and the keyboard shortcut that follows lands on the
   * same row.
   */
  function selectEntryOnClick(clickedEntryId: number): void {
    if (resultIds.length === 0) return;
    selectedIndex = selectedIndexForClick(
      resultIds,
      clickedEntryId,
      selectedIndex,
    );
    selectedEntryId = resultIds[selectedIndex] ?? clickedEntryId;
    scrollSelectedIntoView();
  }

  /**
   * Build a snapshot of the row elements in their current rendered
   * order. The `scrollSelectedRowIntoView` helper accepts the rows in
   * render order so a unit test can pass a stub array. The snapshot
   * is rebuilt on demand from `rowRefs`, which Svelte keeps in sync
   * with the `{#each resultIds}` block through the per-row
   * `bind:this` handlers.
   */
  function snapshotRowElements(): (Element | null)[] {
    if (!resultsListEl) return [];
    const snapshot: (Element | null)[] = [];
    for (const id of resultIds) {
      const element = rowRefs.get(id) ?? null;
      snapshot.push(element);
    }
    return snapshot;
  }

  /**
   * Scroll the selected row into view without moving the desktop
   * scroll surface. The helper delegates to the pure
   * `scrollSelectedRowIntoView` so the contract is exercised by the
   * `quickPasteActions` unit tests and the Svelte layer only wires
   * the DOM around it.
   */
  function scrollSelectedIntoView(): void {
    if (!resultsListEl) return;
    scrollSelectedRowIntoView(
      { length: resultIds.length },
      snapshotRowElements(),
      selectedIndex,
    );
  }

  /**
   * Single confirmation controller used by Enter, Shift+Enter and a
   * row click. The helper is the only switch that maps a stable
   * entry id to the typed copy action, so the keyboard shortcut and
   * the mouse click cannot drift apart: they both flow through
   * `runCopyForEntry` (which keeps the `pasteInFlight` in-flight
   * guard and hides Quick Paste on success). A no-op result from
   * the capability helper (image-only Shift+Enter, no selection) is
   * silently dropped so the caller does not have to branch.
   *
   * The `shiftKey` flag lets the caller pick the plain-text
   * representation (Shift+Enter / Shift+Click) without duplicating
   * the capability rules the row's `quickPasteConfirmAction` helper
   * already owns.
   */
  async function confirmEntry(
    entryId: number,
    options: { shiftKey?: boolean } = {},
  ): Promise<void> {
    const entry = findEntry(mode, recent, hits, entryId);
    if (!entry) return;
    const capabilities = capabilitiesOf(entry);
    const action = quickPasteConfirmAction(
      capabilities,
      options.shiftKey ?? false,
    );
    if (!action || action.kind === "none") {
      return;
    }
    await runCopyForEntry(entryId, action.mode);
  }

  async function handleEnter(event?: KeyboardEvent): Promise<void> {
    if (selectedIndex < 0 || selectedIndex >= resultIds.length) {
      // No selection: do NOT invoke any command. The contract is
      // "Enter without selection must not paste / copy".
      return;
    }
    const entryId = resultIds[selectedIndex];
    if (entryId == null) return;
    await confirmEntry(entryId, { shiftKey: event?.shiftKey ?? false });
  }

  /**
   * Drive the row click contract: clicking the non-interactive
   * surface of a row MUST select the row AND invoke the exact same
   * copy-only confirmation as Enter. The selection step re-anchors
   * the index on the stable entry id (a click on row 3 while row 1
   * is highlighted must not silently leave the keyboard focus on
   * row 1) so the next keyboard shortcut lands on the same row.
   *
   * The handler is intentionally fire-and-forget: the in-flight
   * guard inside `runCopyForEntry` coalesces the second copy that
   * would otherwise be issued by the same click, and any failure
   * surfaces through the same `pasteError` band Enter uses.
   *
   * Pin and menu controls inside the row use `on:click|stopPropagation`
   * so this handler never runs for their activations.
   */
  function handleRowClick(entryId: number, event?: MouseEvent): void {
    selectEntryOnClick(entryId);
    void confirmEntry(entryId, { shiftKey: event?.shiftKey ?? false });
  }

  /**
   * Drive the copy-only flow for a single entry id. The helper is
   * reused by Enter / Shift+Enter / `...` → "Copiar" (when the menu
   * grows it) so the controller and the typed outcome stay in one
   * place. The function hides Quick Paste after a successful write
   * and surfaces the typed guidance on failure without mutating the
   * history row.
   */
  async function runCopyForEntry(entryId: number, mode: CopyMode): Promise<void> {
    if (pasteInFlight.has(entryId)) {
      return;
    }
    const next = new Set(pasteInFlight);
    next.add(entryId);
    pasteInFlight = next;
    guidance = null;
    pasteError = null;
    pinError = null;
    try {
      const outcome = await performCopyFlow({
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
        copyFn: () => copyEntryCommand({ id: entryId, mode }),
      });
      if (outcome.kind === "copied") {
        // Window stays hidden by contract; nothing to do here.
        return;
      }
      // Failure path: window has been re-shown by the controller.
      guidance = readCopyGuidance(outcome.response);
      pasteError = readCopyError(outcome.response);
    } catch (error) {
      pasteError = error instanceof Error ? error.message : String(error);
    } finally {
      const reduced = new Set(pasteInFlight);
      reduced.delete(entryId);
      pasteInFlight = reduced;
    }
  }

  function readCopyGuidance(
    response: CopyResponse | { error: string },
  ): PlatformGuidance | null {
    if ("guidance" in response && response.guidance) {
      return response.guidance;
    }
    return null;
  }

  function readCopyError(response: CopyResponse | { error: string }): string {
    if ("error" in response) {
      return response.error;
    }
    return response.message ?? `Copy ${response.kind}`;
  }

  /**
   * Drive the direct paste flow for a single entry id. The menu
   * actions keep the legacy paste lifecycle: they hide Quick Paste
   * first so the previously focused application can receive the
   * synthetic paste, and they surface the typed guidance on
   * failure. The flow is intentionally NOT used by Enter or
   * Shift+Enter — those go through `runCopyForEntry` so the
   * clipboard is written without a synthetic paste.
   */
  async function runMenuPasteForEntry(
    entryId: number,
    mode: "plain" | "rich" | null,
  ): Promise<void> {
    if (pasteInFlight.has(entryId)) {
      return;
    }
    const next = new Set(pasteInFlight);
    next.add(entryId);
    pasteInFlight = next;
    guidance = null;
    pasteError = null;
    pinError = null;
    openMenuEntryId = null;
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
        pasteFn: () => pasteEntryCommand({ id: entryId, mode }),
      });
      if (outcome.kind === "pasted") {
        return;
      }
      guidance = readGuidance(outcome.response);
      pasteError =
        "error" in outcome.response
          ? outcome.response.error
          : readMessage(outcome.response);
    } catch (error) {
      pasteError = error instanceof Error ? error.message : String(error);
    } finally {
      const reduced = new Set(pasteInFlight);
      reduced.delete(entryId);
      pasteInFlight = reduced;
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
    if (openMenuEntryId !== null) {
      closeMenu();
      return;
    }
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

  /**
   * Toggle the favourite flag for the supplied entry id. The helper
   * reuses the existing `clipvault_set_favorite` command so the
   * history row stays the single source of truth and the desktop
   * rail cannot drift from the Quick Paste pin state. The toggle
   * never mutates payload, tags, collections, timestamps or assets:
   * the management service only flips `is_pinned`.
   *
   * On success the local `recent` / `hits` lists are patched with
   * the response's refreshed summary so the order helper (which
   * puts favourites first) can re-rank the list without a full
   * reload. The response carries an `EntrySummary`; we keep the
   * existing record's payload, content_type and metadata
   * untouched and only flip the `is_pinned` field.
   */
  async function togglePin(entryId: number): Promise<void> {
    if (pinInFlight.has(entryId)) {
      return;
    }
    const current =
      mode === "search"
        ? hits.find((hit) => hit.entry_id === entryId)?.record ?? null
        : recent.find((entry) => entry.id === entryId) ?? null;
    if (!current) return;
    const next = new Set(pinInFlight);
    next.add(entryId);
    pinInFlight = next;
    pinError = null;
    const previousIndex = selectedIndex;
    try {
      const response = await setFavoriteCommand({
        id: entryId,
        pinned: !current.is_pinned,
      });
      if (response.kind === "updated") {
        const summary = response.entry;
        const patched: EntryRecord = {
          ...current,
          is_pinned: summary.is_pinned,
          updated_at: summary.updated_at,
        };
        if (mode === "search") {
          hits = hits.map((hit) =>
            hit.entry_id === entryId ? { ...hit, record: patched } : hit,
          );
        } else {
          recent = recent.map((entry) =>
            entry.id === entryId ? patched : entry,
          );
        }
        // Preserve the user's selection across the reorder so the
        // keyboard focus stays deterministic after a pin toggle.
        const reordered = preserveSelectionAfterReorder(
          resultIds,
          previousIndex,
        );
        selectedIndex = reordered.selectedIndex;
      } else {
        pinError = `La entrada #${entryId} ya no está disponible.`;
      }
    } catch (error) {
      pinError = error instanceof Error ? error.message : String(error);
    } finally {
      const reduced = new Set(pinInFlight);
      reduced.delete(entryId);
      pinInFlight = reduced;
    }
  }

  function closeMenu(): void {
    openMenuEntryId = null;
  }

  function toggleMenuFor(entryId: number): void {
    openMenuEntryId = openMenuEntryId === entryId ? null : entryId;
  }

  /**
   * The Quick Paste menu exposes the legacy direct paste actions
   * for every entry. The helper closes the menu and dispatches the
   * paste through `runMenuPasteForEntry` so the legacy paste
   * lifecycle (hide-before-paste, active-target, guidance, close)
   * stays intact and the keyboard copy-only flow is unaffected.
   */
  function runMenuAction(entryId: number, mode: "plain" | "rich" | null): void {
    void runMenuPasteForEntry(entryId, mode);
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

  /**
   * Reference to the search input. The compact UI must autofocus the
   * input every time the window opens so the user can start typing
   * immediately after `Cmd/Ctrl+Shift+V`. The reference is captured
   * with `bind:this` and the focus call happens in
   * `onQuickPasteOpened` (after the `tick()` microtask so the input
   * is guaranteed to be mounted and visible).
   */
  let searchInputEl: HTMLInputElement | null = null;
  let unlistenOpened: (() => void) | null = null;
  /**
   * Reference to the result list container. The autoscroll helper
   * uses the element only as a "list exists" sentinel; the actual
   * `scrollIntoView` call always targets the selected row, so the
   * desktop / window scroll surface cannot move.
   */
  let resultsListEl: HTMLUListElement | null = null;
  /**
   * Per-row element references keyed by entry id. The row uses
   * `bind:this` to capture the element and the reactive block
   * below mirrors it into `rowRefs` so the autoscroll helper can
   * target the selected row without a `querySelector` round-trip.
   */
  const rowRefs = new Map<number, HTMLLIElement>();
  /**
   * Parallel array used by `bind:this` to capture the rendered `<li>`
   * element at the row's render index. Length tracks `resultIds`
   * and the `rebuildRowRefs` reactive block mirrors the array into
   * the keyed `rowRefs` map so the autoscroll helper can look up the
   * element by entry id without a `querySelector` round-trip.
   */
  let rowEls: (HTMLLIElement | null)[] = [];
  $: {
    const next = new Array<HTMLLIElement | null>(resultIds.length).fill(null);
    for (let i = 0; i < rowEls.length && i < resultIds.length; i += 1) {
      next[i] = rowEls[i];
    }
    rowEls = next;
  }
  $: {
    // Mirror `rowEls` into the id-keyed map so the autoscroll helper
    // can resolve the selected row without re-running the each-block.
    const next = new Map<number, HTMLLIElement>();
    resultIds.forEach((id, index) => {
      const element = rowEls[index];
      if (element) {
        next.set(id, element);
      }
    });
    rowRefs.clear();
    for (const [id, element] of next) {
      rowRefs.set(id, element);
    }
  }

  function focusSearchInput(): void {
    if (!searchInputEl) return;
    searchInputEl.focus();
    // Place the caret at the end of the existing value so the user
    // can keep typing without having to click into the field.
    const valueLength = searchInputEl.value.length;
    try {
      searchInputEl.setSelectionRange(valueLength, valueLength);
    } catch {
      // `setSelectionRange` is not supported on every input type on
      // every host; ignoring the failure is the safe no-op the
      // spec relies on.
    }
  }

  async function onQuickPasteOpened(): Promise<void> {
    // Reset the visible state so the user sees the recent list when
    // the hotkey fires. The bridge only emits a null payload, so we
    // intentionally do not look at the event details.
    selectedIndex = 0;
    query = "";
    void loadRecent();
    // The autofocus contract from the compact-UI spec: focus the
    // search field every time the window opens so the user can type
    // immediately. We wait for the next microtask so the focus call
    // lands on a mounted, visible input.
    await tick();
    focusSearchInput();
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

  onMount(() => {
    void loadRecent();
    unlistenOpened = safeListenOpened(() => {
      void onQuickPasteOpened();
    });
    // The opened signal is the canonical "user just opened the
    // window" cue. The initial mount also calls `onQuickPasteOpened`
    // (via the listener) so the autofocus is the contract of the
    // very first activation as well — no separate code path needed.
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
    thumbnailStates = {};
  });
</script>

<svelte:window on:keydown={onWindowKeydown} />

<main data-testid="quick-paste-root">
  <input
    type="search"
    class="qp-search"
    placeholder="Buscar en el historial del portapapeles"
    value={query}
    on:input={onInput}
    aria-label="Buscar en el historial del portapapeles"
    data-testid="quick-paste-input"
    bind:this={searchInputEl}
  />

  <div class="qp-results-region" data-testid="quick-paste-results-region">
    {#if loading}
      <p
        class="qp-status"
        data-testid="quick-paste-loading"
        role="status"
        aria-live="polite"
      >
        Cargando historial…
      </p>
    {:else if searchError}
      <p
        class="qp-status qp-status-error"
        role="alert"
        data-testid="quick-paste-error"
      >
        {searchError}
      </p>
    {:else if visibleEmpty}
      <p
        class="qp-status"
        data-testid="quick-paste-empty"
        role="status"
        aria-live="polite"
      >
        {emptyMessage}
      </p>
    {:else if pasteError}
      <p
        class="qp-status qp-status-error"
        role="alert"
        data-testid="quick-paste-paste-error"
      >
        {pasteError}
      </p>
    {:else}
      <ul
        class="qp-entries"
        data-testid="quick-paste-results"
        data-row-height={ROW_HEIGHT_PX}
        bind:this={resultsListEl}
      >
        {#each resultIds as id, index (id)}
          {@const entry = findEntry(mode, recent, hits, id)}
          {@const contentType = resolveContentType(mode, recent, hits, id)}
          {@const title = renderTitle(mode, recent, hits, id)}
          {@const preview = renderPreview(mode, recent, hits, id)}
          {@const isImage = entry ? isImageEntry(entry) : false}
          {@const thumbState = thumbnailStates[id] ?? (isImage ? "loading" : "error")}
          {@const sourceAppLabel = entry
            ? sourceAppAccessibleLabel(entry)
            : "Aplicación fuente desconocida"}
          {@const typeIconId = contentTypeIconId(contentType)}
          {@const typeLabel = contentTypeIconLabel(contentType)}
          {@const isPinned = entry ? entry.is_pinned : false}
          {@const menuActions = entry
            ? quickPasteMenuActions(entry, title, {
                pasteBusy: pasteInFlight.has(id) || pinInFlight.has(id),
              })
            : []}
          {@const menuOpen = openMenuEntryId === id}
          <li
            class="qp-row"
            class:qp-row-active={index === selectedIndex}
            style="--qp-row-height: {ROW_HEIGHT_PX}px;"
            data-entry-id={id}
            data-testid="quick-paste-row"
            data-selected={index === selectedIndex ? "true" : "false"}
            data-content-type={contentType}
            data-thumb-state={isImage ? thumbState : "none"}
            data-row-height={ROW_HEIGHT_PX}
            data-pinned={isPinned ? "true" : "false"}
            role="option"
            aria-selected={index === selectedIndex}
            aria-label={title}
            title={title}
            on:click={(event) => handleRowClick(id, event)}
            bind:this={rowEls[index]}
          >
            <div class="qp-row-line qp-row-line-meta">
              <span
                class="qp-type"
                data-testid="quick-paste-type"
                data-content-type={contentType}
                aria-hidden="true"
                title={`Tipo: ${typeLabel}`}
              >
                <svg
                  aria-hidden="true"
                  focusable="false"
                  width="14"
                  height="14"
                >
                  <use href="#{typeIconId}" />
                </svg>
                <span class="qp-visually-hidden">
                  Tipo: {typeLabel}
                </span>
              </span>
              <span
                class="qp-title"
                data-testid="quick-paste-title"
                data-row-title={title}
              >
                {title}
              </span>
              <button
                type="button"
                class="qp-pin"
                data-testid="quick-paste-pin"
                data-pinned={isPinned ? "true" : "false"}
                data-busy={pinInFlight.has(id) ? "true" : "false"}
                aria-pressed={isPinned}
                aria-label={isPinned
                  ? `Quitar favorito de ${title}`
                  : `Marcar ${title} como favorito`}
                title={isPinned ? "Quitar favorito" : "Marcar como favorito"}
                on:click|stopPropagation={() => void togglePin(id)}
              >
                <svg
                  aria-hidden="true"
                  focusable="false"
                  width="14"
                  height="14"
                  viewBox="0 0 24 24"
                  fill={isPinned ? "currentColor" : "none"}
                  stroke="currentColor"
                  stroke-width="1.6"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  data-testid={isPinned
                    ? "quick-paste-pin-filled"
                    : "quick-paste-pin-outline"}
                >
                  <path d="M12 17v5" />
                  <path d="M9 10.76V5a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v5.76l3.13 1.88a1 1 0 0 1 .44 1.34l-2.34 4.42a1 1 0 0 1-.87.5h-4.72a1 1 0 0 1-.87-.5L7.43 13.98a1 1 0 0 1 .44-1.34L11 10.76Z" />
                </svg>
                <span class="qp-visually-hidden">
                  {isPinned ? "Quitar favorito" : "Marcar como favorito"}
                </span>
              </button>
              <span
                class="qp-source-app"
                data-testid="quick-paste-source-app"
                title={sourceAppLabel}
                aria-label={sourceAppLabel}
              >
                <svg
                  aria-hidden="true"
                  focusable="false"
                  width="14"
                  height="14"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.6"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  data-testid="quick-paste-source-app-fallback"
                >
                  <rect x="4" y="4" width="16" height="16" rx="3" />
                  <path d="M9 9h6v6H9z" />
                </svg>
                <span class="qp-visually-hidden">{sourceAppLabel}</span>
              </span>
            </div>
            <div class="qp-row-line qp-row-line-body">
              {#if isImage}
                <span
                  class="qp-thumb"
                  data-testid="quick-paste-thumbnail"
                  data-thumb-state={thumbState}
                  aria-hidden="true"
                >
                  {#if thumbnails[id] && thumbState === "loaded"}
                    <img
                      class="qp-thumb-img"
                      src={thumbnails[id]}
                      alt=""
                       on:error={() => dropThumbnail(id, entry?.asset_ref ?? undefined)}
                    />
                  {:else if thumbState === "loading"}
                    <span
                      class="qp-thumb-placeholder"
                      data-testid="quick-paste-thumbnail-loading"
                      aria-label="Cargando imagen"
                    >
                      <svg
                        aria-hidden="true"
                        focusable="false"
                        width="16"
                        height="16"
                      >
                        <use href="#{typeIconId}" />
                      </svg>
                    </span>
                  {:else}
                    <span
                      class="qp-thumb-placeholder qp-thumb-placeholder-error"
                      data-testid="quick-paste-thumbnail-error"
                      aria-label="Imagen no disponible"
                    >
                      <svg
                        aria-hidden="true"
                        focusable="false"
                        width="16"
                        height="16"
                      >
                        <use href="#{typeIconId}" />
                      </svg>
                    </span>
                  {/if}
                </span>
                <span
                  class="qp-preview qp-preview-image"
                  data-testid="quick-paste-preview"
                  data-content-type="image"
                >
                  {preview}
                </span>
              {:else}
                <span
                  class="qp-preview"
                  data-testid="quick-paste-preview"
                  data-content-type={contentType}
                >
                  {preview}
                </span>
              {/if}
              <span
                class="qp-elapsed"
                data-testid="quick-paste-elapsed"
                aria-label={entry
                  ? formatElapsedTime(entry.created_at, new Date()).accessible
                  : ""}
              >
                {entry
                  ? formatElapsedTime(entry.created_at, new Date()).visual
                  : ""}
              </span>
              {#if entry}
                <button
                  type="button"
                  class="qp-menu-trigger"
                  data-testid="quick-paste-menu-trigger"
                  aria-haspopup="menu"
                  aria-expanded={menuOpen}
                  aria-label={`Más acciones para ${title}`}
                  title="Más acciones"
                  on:click|stopPropagation={() => toggleMenuFor(id)}
                >
                  <svg
                    aria-hidden="true"
                    focusable="false"
                    width="14"
                    height="14"
                    viewBox="0 0 24 24"
                    fill="currentColor"
                  >
                    <circle cx="12" cy="5" r="1.6" />
                    <circle cx="12" cy="12" r="1.6" />
                    <circle cx="12" cy="19" r="1.6" />
                  </svg>
                  <span class="qp-visually-hidden">Más acciones</span>
                </button>
              {/if}
            </div>
            {#if menuOpen && entry}
              <ul
                class="qp-menu"
                role="menu"
                data-testid="quick-paste-menu"
                data-entry-id={id}
              >
                {#each menuActions as action (action.testId)}
                  <li role="none">
                    <button
                      type="button"
                      role="menuitem"
                      class="qp-menu-item"
                      data-testid={action.testId}
                      data-action-kind={action.kind}
                      disabled={action.disabled}
                      aria-label={action.ariaLabel}
                      title={action.tooltip}
                      on:click|stopPropagation={() =>
                        runMenuAction(id, action.mode)}
                    >
                      {action.label}
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    {#if pinError}
      <p
        class="qp-status qp-status-error"
        role="alert"
        data-testid="quick-paste-pin-error"
      >
        {pinError}
      </p>
    {/if}
  </div>
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
    /* The compact UI fills the 720x520 transient window. Every
     * padding/inset is hand-tuned so the 72px rows fit without
     * horizontal scroll and the search input stays the visual
     * header. */
    box-sizing: border-box;
    width: 100%;
    height: 100vh;
    padding: 0.5rem 0.75rem 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    overflow: hidden;
    user-select: none;
    -webkit-user-select: none;
  }

  .qp-search {
    /* The search field is the primary header of the palette. The
     * typography follows the compact-UI spec: 15-16px, slightly
     * larger than the row title so the user lands on the right
     * surface as soon as the window opens. */
    flex: 0 0 auto;
    width: 100%;
    padding: 0.45rem 0.7rem;
    background: #161b22;
    color: inherit;
    border: 1px solid #30363d;
    border-radius: 6px;
    font-family: inherit;
    font-size: 0.95rem;
    line-height: 1.2;
    box-sizing: border-box;
  }

  .qp-search:focus {
    outline: none;
    border-color: #2563eb;
    box-shadow: 0 0 0 1px rgba(37, 99, 235, 0.5);
  }

  .qp-results-region {
    flex: 1 1 auto;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .qp-status {
    margin: 0;
    padding: 0.4rem 0.5rem;
    font-size: 0.78rem;
    line-height: 1.3;
    color: #94a3b8;
    font-style: italic;
    /* Pin the empty/loading/error band to a stable height that
     * matches a single row so the window never resizes when the
     * list state flips between results, empty and error. */
    min-height: 72px;
    display: flex;
    align-items: center;
  }

  .qp-status-error {
    color: #f87171;
    font-style: normal;
  }

  .qp-entries {
    /* The list owns the only vertical scroll surface in the
     * window. Every other container (search, status) is bounded,
     * so the rail cannot grow the window or introduce horizontal
     * scroll. */
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    overflow-x: hidden;
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .qp-row {
    /* The fixed-height row: 72px is the value pinned by the
     * compact-UI spec. Every state (default, hover, selected,
     * focus, loading, error) keeps the same rectangle so the
     * neighbour rows never shift when a single entry's state
     * changes. */
    box-sizing: border-box;
    height: var(--qp-row-height, 72px);
    min-height: var(--qp-row-height, 72px);
    max-height: var(--qp-row-height, 72px);
    display: grid;
    grid-template-rows: 1fr 1fr;
    gap: 0.1rem;
    padding: 0.35rem 0.55rem;
    border-radius: 6px;
    border: 1px solid #1f2937;
    background: rgba(15, 23, 42, 0.55);
    color: inherit;
    overflow: hidden;
    cursor: default;
  }

  .qp-row:hover {
    background: rgba(37, 99, 235, 0.12);
  }

  .qp-row-active,
  .qp-row-active:hover {
    background: #1d4ed8;
    color: #ffffff;
    border-color: #2563eb;
  }

  .qp-row-line {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    min-width: 0;
  }

  .qp-row-line-meta {
    /* The metadata line carries the type icon, the title, the pin
     * button and the source-app icon. Each column has a fixed
     * footprint so the title truncates with an ellipsis instead of
     * pushing the source-app icon out of the visible area. */
    grid-template-columns: 18px 1fr 18px 18px;
  }

  .qp-type {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    flex: 0 0 18px;
    border-radius: 4px;
    background: rgba(147, 197, 253, 0.12);
    color: #93c5fd;
  }

  .qp-row-active .qp-type {
    background: rgba(255, 255, 255, 0.18);
    color: #ffffff;
  }

  .qp-title {
    flex: 1 1 auto;
    min-width: 0;
    font-size: 0.84rem;
    line-height: 1.2;
    font-weight: 600;
    color: inherit;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .qp-source-app {
    flex: 0 0 18px;
    width: 18px;
    height: 18px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    background: rgba(148, 163, 184, 0.12);
    color: #94a3b8;
  }

  .qp-row-active .qp-source-app {
    background: rgba(255, 255, 255, 0.18);
    color: #ffffff;
  }

  .qp-pin {
    /* The pin button lives in the metadata line, between the title
     * and the source-app icon. The 18px footprint matches the
     * neighbouring icons so the title truncates instead of
     * pushing the column out of the visible area. The button is
     * a `button` element so keyboard activation and screen-reader
     * announcements follow the documented accessible contract. */
    flex: 0 0 18px;
    width: 18px;
    height: 18px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    background: rgba(148, 163, 184, 0.12);
    color: #94a3b8;
    border: 0;
    padding: 0;
    cursor: pointer;
    transition: color 80ms ease;
  }

  .qp-pin:hover {
    color: #fde68a;
  }

  .qp-pin:focus-visible {
    outline: 1px solid #2563eb;
    outline-offset: 1px;
  }

  .qp-pin[data-pinned="true"] {
    color: #facc15;
    background: rgba(250, 204, 21, 0.18);
  }

  .qp-pin[data-busy="true"] {
    opacity: 0.6;
    cursor: progress;
  }

  .qp-row-active .qp-pin {
    color: #ffffff;
    background: rgba(255, 255, 255, 0.18);
  }

  .qp-row-active .qp-pin[data-pinned="true"] {
    color: #fef3c7;
    background: rgba(255, 255, 255, 0.28);
  }

  .qp-row-line-body {
    /* The body line carries the preview/thumbnail, the elapsed
     * time and the menu trigger. The thumbnail reserves a fixed
     * 40x40px square; the preview truncates with an ellipsis. The
     * elapsed time stays a fixed-width column so the preview
     * never has to compete with it. The menu trigger takes a
     * fixed 18px column so the body line never shifts when the
     * menu opens or closes. */
    grid-template-columns: minmax(0, 1fr) auto 18px;
  }

  .qp-preview {
    flex: 1 1 auto;
    min-width: 0;
    font-size: 0.78rem;
    line-height: 1.25;
    color: #cbd5f5;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .qp-row-active .qp-preview {
    color: rgba(255, 255, 255, 0.92);
  }

  .qp-preview-image {
    /* The image preview text sits next to the thumbnail; the
     * ellipsis truncates long image labels the same way text
     * entries do. */
    color: #94a3b8;
  }

  .qp-row-active .qp-preview-image {
    color: rgba(255, 255, 255, 0.78);
  }

  .qp-elapsed {
    flex: 0 0 auto;
    font-size: 0.7rem;
    line-height: 1.2;
    color: #94a3b8;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .qp-row-active .qp-elapsed {
    color: rgba(255, 255, 255, 0.85);
  }

  .qp-menu-trigger {
    /* The ellipsis trigger lives in the body line, after the
     * elapsed-time column. The 18px footprint matches the other
     * control icons so the body line stays a stable grid. The
     * button is a `button` element with the documented
     * aria-haspopup contract. */
    flex: 0 0 18px;
    width: 18px;
    height: 18px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    background: rgba(148, 163, 184, 0.12);
    color: #94a3b8;
    border: 0;
    padding: 0;
    cursor: pointer;
    transition: color 80ms ease;
  }

  .qp-menu-trigger:hover {
    color: #cbd5f5;
  }

  .qp-menu-trigger:focus-visible {
    outline: 1px solid #2563eb;
    outline-offset: 1px;
  }

  .qp-menu-trigger[aria-expanded="true"] {
    background: rgba(37, 99, 235, 0.45);
    color: #ffffff;
  }

  .qp-row-active .qp-menu-trigger {
    background: rgba(255, 255, 255, 0.18);
    color: #ffffff;
  }

  .qp-menu {
    /* The menu is positioned absolutely inside the row so the
     * fixed-height contract stays intact: a single-open menu does
     * not push other rows down. */
    position: absolute;
    right: 0.55rem;
    top: 100%;
    z-index: 2;
    margin: 0;
    padding: 0.25rem;
    list-style: none;
    min-width: 12rem;
    background: #1f2937;
    color: #f0f4f8;
    border: 1px solid #30363d;
    border-radius: 6px;
    box-shadow: 0 8px 18px rgba(0, 0, 0, 0.45);
  }

  .qp-row {
    position: relative;
  }

  .qp-menu-item {
    display: block;
    width: 100%;
    padding: 0.35rem 0.55rem;
    border-radius: 4px;
    border: 0;
    background: transparent;
    color: inherit;
    text-align: left;
    font: inherit;
    cursor: pointer;
  }

  .qp-menu-item:hover:not(:disabled) {
    background: rgba(37, 99, 235, 0.35);
  }

  .qp-menu-item:focus-visible {
    outline: 1px solid #2563eb;
    outline-offset: 1px;
  }

  .qp-menu-item:disabled {
    color: rgba(148, 163, 184, 0.55);
    cursor: not-allowed;
  }

  .qp-thumb {
    flex: 0 0 40px;
    width: 40px;
    height: 40px;
    min-width: 40px;
    min-height: 40px;
    border-radius: 4px;
    background: rgba(148, 163, 184, 0.12);
    overflow: hidden;
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .qp-thumb-img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .qp-thumb-placeholder {
    width: 100%;
    height: 100%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: #94a3b8;
  }

  .qp-thumb-placeholder-error {
    color: #f87171;
  }

  .qp-visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    padding: 0;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
    border: 0;
  }
</style>
