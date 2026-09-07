<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import type {
    CopyResponse,
    EntryRecord,
    PlatformGuidance,
    SearchHit,
    SearchResponse,
  } from "./types";
  import { visualTokenCss } from "./lib/visualTokens";
  import {
    copyEntryCommand,
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
  import { performCopyFlow } from "./lib/quickPasteController";
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
    APP_FALLBACK_ICON_SVG,
    CONTENT_TYPE_ICON_SPRITE,
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
  import {
    matchesPreviewShortcut,
  } from "./lib/clipboardPreview";
  import {
    createIconResolver,
    type IconResolver,
  } from "./lib/iconResolver";
  import {
    quickPasteSearchShortcutAccessibleLabel,
    quickPasteSearchShortcutLabel,
    searchShortcutPlatform,
    type SearchShortcutPlatform,
  } from "./lib/searchShortcut";
  import { sourceAppIconCommand, diagnosticsCommand } from "./lib/tauri";
  import PlatformGuidanceModal from "./PlatformGuidanceModal.svelte";
  import ClipboardPreview from "./ClipboardPreview.svelte";

  /**
   * Single source of truth for the visual tokens. The string is the
   * exact CSS custom-property block `lib/visualTokens.ts` emits and
   * the `<svelte:head>` block below injects at the document root.
   * Quick Paste runs inside its own Tauri webview so the `:root`
   * block `App.svelte` injects on the desktop window is NOT visible
   * here — the Quick Paste stylesheet MUST mount the same block to
   * keep the documented `var(--cv-*, fallback)` references in
   * lock-step with the desktop rail. Without this re-declaration the
   * palette would render with the literal fallback colours and the
   * typography would drift to the browser default family.
   */
  const VISUAL_TOKEN_ROOT_CSS = visualTokenCss();
  // Mark the constant as used at the TypeScript level so svelte-check
  // does not flag it — the actual consumer is the `<svelte:head>`
  // block below.
  void VISUAL_TOKEN_ROOT_CSS;

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
   * `position: fixed` rectangle the menu popover renders at. The
   * menu is portalised out of the row so the documented
   * `overflow: hidden` on the row / list / main cannot clip it;
   * `recomputeMenuPosition` writes the style string every time the
   * user opens the menu or scrolls the list so the popover always
   * stays anchored to the triggering `...` button and inside the
   * documented `720 × 520` viewport.
   */
  let menuPositionStyle = "";
  /**
   * Per-row menu trigger element references keyed by entry id. The
   * rows mount a hidden `qp-menu-anchor` button only while the menu
   * is open; the menu popover reads the button's bounding rect to
   * compute its `position: fixed` rectangle. The map mirrors the
   * existing `rowRefs` pattern so the helper can resolve the
   * anchor by id without a `querySelector` round-trip.
   */
  const menuAnchorEls: Record<number, HTMLElement> = {};
  /**
   * Reference to the open menu popover element. The popover lives
   * outside the row (portalised into the list container) so the
   * outside-click listener can inspect it directly without
   * walking the DOM. `null` when no menu is open.
   */
  let menuEl: HTMLUListElement | null = null;
  /**
   * Entry ids currently driving a paste or pin round-trip. The set
   * is the only switch the menu / keyboard helpers consult so two
   * concurrent activations on the same row are coalesced and a
   * second click on the trigger or the menu item while the round
   * trip is in flight becomes a no-op.
   */
  let pasteInFlight: Set<number> = new Set();
  let pinInFlight: Set<number> = new Set();
  /**
   * Stable id of the entry whose `Previsualizar` overlay is currently
   * open. Only one preview can be open at a time; the overlay is a
   * strictly read-only surface inside the same fixed window — it
   * never copies, pastes, mutates history or alters the previously
   * active application target. `null` means no preview is open.
   */
  let previewEntryId: number | null = null;
  /**
   * Platform the Quick Paste window was opened on. Resolved once
   * from the backend diagnostics so the visible `Cmd/Ctrl+K` hint
   * and the keyboard matcher stay in lockstep without a second
   * global listener.
   */
  let shortcutPlatform: SearchShortcutPlatform = "other";

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

  // ---------------------------------------------------------------
  // Source-application icons in the quick-paste list.
  //
  // The list reuses the same validated icon bridge and blob-URL
  // lifecycle as `HistoryCard.source-app-icon` (see
  // `openspec/changes/archive/2026-09-04-history-card-layout/`).
  // The contract documented in
  // `openspec/changes/quick-paste-preview-ui/specs/quick-paste/spec.md`
  // requires:
  //
  //   - the icon area has a stable square footprint across
  //     `loading`, `loaded`, and `error` so the row never reflows;
  //   - the `loaded` state renders the real persisted icon;
  //   - the `loading` state renders a distinguishable placeholder
  //     (NOT an empty square);
  //   - the `error` / `metadata absent` state renders a generic
  //     application glyph and never displays the raw bundle
  //     identifier as visible content;
  //   - stale icon responses from a previous entry cannot clobber
  //     the current entry's state.
  // ---------------------------------------------------------------

  const tauriSourceAppIconLoader: import("./lib/iconResolver").IconLoader = {
    async loadIconBytes(ref: string): Promise<number[] | Uint8Array | null> {
      try {
        return await sourceAppIconCommand({ ref });
      } catch {
        return null;
      }
    },
  };

  const appIconResolver: IconResolver = createIconResolver(
    tauriSourceAppIconLoader,
  );
  let appIconUrls: Record<number, string> = {};
  /**
   * Per-entry source-app icon state. Mirrors the `thumbnailStates`
   * invariant: `loading` is the canonical state for any row whose
   * metadata carries a coherent `source_app_icon_ref` while the
   * bridge round-trip is still pending; `loaded` swaps in the
   * resolved blob URL; `error` collapses loader rejections and
   * metadata-absent rows onto the same accessible fallback.
   *
   * The branch above intentionally never asks the renderer to inspect
   * `appIconUrls === null` (which is also the terminal state for an
   * entry without a persisted icon), so the row never flashes a
   * missing icon.
   */
  type AppIconState = "loading" | "loaded" | "error";
  let appIconStates: Record<number, AppIconState> = {};
  /**
   * Per-entry token used to discard stale icon round-trips. Mirrors
   * the `thumbnailToken` invariant above: a response that lands
   * after a newer round has been scheduled for a different entry is
   * dropped so the row never shows another entry's icon.
   */
  let appIconToken = 0;
  const appIconTokens = new Map<number, number>();

  async function loadAppIcon(entry: EntryRecord): Promise<void> {
    const ref = entry.source_app_icon_ref;
    if (!ref) return;
    if (appIconUrls[entry.id]) return;
    if (appIconStates[entry.id] !== "loading") {
      appIconStates = { ...appIconStates, [entry.id]: "loading" };
    }
    const token = ++appIconToken;
    appIconTokens.set(entry.id, token);
    const resolution = await appIconResolver.resolve(ref);
    if (appIconTokens.get(entry.id) !== token) {
      return;
    }
    if (resolution.ok && resolution.url) {
      appIconUrls = { ...appIconUrls, [entry.id]: resolution.url };
      appIconStates = { ...appIconStates, [entry.id]: "loaded" };
    } else {
      appIconStates = { ...appIconStates, [entry.id]: "error" };
    }
  }

  function syncAppIcons(
    currentMode: Mode,
    recents: EntryRecord[],
    searchHits: SearchHit[],
  ): void {
    const entries =
      currentMode === "search" ? searchHits.map((hit) => hit.record) : recents;
    for (const entry of entries) {
      void loadAppIcon(entry);
    }
  }

  $: syncAppIcons(mode, recent, hits);

  function dropAppIcon(id: number, ref?: string | null): void {
    appIconTokens.delete(id);
    if (ref) {
      appIconResolver.releaseFor(ref);
    }
    if (appIconUrls[id]) {
      const { [id]: _removed, ...rest } = appIconUrls;
      appIconUrls = rest;
    }
    appIconStates = { ...appIconStates, [id]: "error" };
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
   * guard). A no-op result from the capability helper (image-only
   * Shift+Enter, no selection) is silently dropped so the caller
   * does not have to branch.
   *
   * `hideAfterSuccess` defaults to `true` so the keyboard contract
   * keeps the historical behaviour (Enter / Shift+Enter hide the
   * palette after a successful write). The row-click contract
   * documented in the `quick-paste-preview-ui` change passes
   * `hideAfterSuccess: false` so a click leaves the palette open
   * and the user can dispatch `Cmd/Ctrl+V` against the just-copied
   * representation. The helper is the single switch so the two
   * surfaces cannot drift apart.
   *
   * The `shiftKey` flag lets the caller pick the plain-text
   * representation (Shift+Enter / Shift+Click) without duplicating
   * the capability rules the row's `quickPasteConfirmAction` helper
   * already owns.
   */
  async function confirmEntry(
    entryId: number,
    options: { shiftKey?: boolean; hideAfterSuccess?: boolean } = {},
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
    await runCopyForEntry(entryId, action.mode, { hideAfterSuccess: options.hideAfterSuccess ?? true });
  }

  async function handleEnter(event?: KeyboardEvent): Promise<void> {
    if (selectedIndex < 0 || selectedIndex >= resultIds.length) {
      // No selection: do NOT invoke any command. The contract is
      // "Enter without selection must not paste / copy".
      return;
    }
    const entryId = resultIds[selectedIndex];
    if (entryId == null) return;
    await confirmEntry(entryId, { shiftKey: event?.shiftKey ?? false, hideAfterSuccess: true });
  }

  /**
   * Drive the row click contract: clicking the non-interactive
   * surface of a row MUST select the row AND invoke the exact same
   * copy-only confirmation as Enter. The selection step re-anchors
   * the index on the stable entry id (a click on row 3 while row 1
   * is highlighted must not silently leave the keyboard focus on
   * row 1) so the next keyboard shortcut lands on the same row.
   *
   * The `quick-paste-preview-ui` change supersedes the previous
   * click contract: the click keeps Quick Paste visible after a
   * successful copy so the user can dispatch `Cmd/Ctrl+V` from the
   * previously focused application without re-opening the palette.
   * The `pasteInFlight` guard inside `runCopyForEntry` coalesces
   * the second copy that would otherwise be issued by the same
   * click, and any failure surfaces through the same `pasteError`
   * band Enter uses.
   *
   * Pin and menu controls inside the row use
   * `on:click|stopPropagation` so this handler never runs for their
   * activations.
   */
  function handleRowClick(entryId: number, event?: MouseEvent): void {
    selectEntryOnClick(entryId);
    void confirmEntry(entryId, { shiftKey: event?.shiftKey ?? false, hideAfterSuccess: false });
  }

  /**
   * Drive the copy-only flow for a single entry id. The helper is
   * reused by Enter / Shift+Enter / row-click so the controller and
   * the typed outcome stay in one place. The function hides Quick
   * Paste after a successful write (or re-shows it when the caller
   * passes `hideAfterSuccess: false`) and surfaces the typed
   * guidance on failure without mutating the history row.
   */
  async function runCopyForEntry(
    entryId: number,
    mode: CopyMode,
    options: { hideAfterSuccess?: boolean } = {},
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
        hideAfterSuccess: options.hideAfterSuccess ?? true,
      });
      if (outcome.kind === "copied") {
        // The keyboard contract hides the window on success
        // (`windowStaysHidden === true`); the click contract keeps
        // the window visible (`windowStaysHidden === false`) so the
        // user can dispatch `Cmd/Ctrl+V` from the previously
        // focused application. The controller itself owns the
        // show/hide lifecycle, so the row only has to check the
        // outcome shape — no second branch on `options`.
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
   * Drive the copy-only flow for a single menu entry id. The menu
   * action reuses the same `performCopyFlow` controller the
   * keyboard / row-click paths consult, only with
   * `hideAfterSuccess: false` so the Quick Paste window stays visible
   * after a successful copy and the user can dispatch `Cmd/Ctrl+V`
   * from the previously focused application. The menu actions MUST
   * NOT invoke `pasteEntryCommand` or trigger a synthetic paste
   * controller — that contract is the whole point of the
   * `quick-paste-preview-ui` change.
   *
   * The popover closes before the copy round-trip starts so the
   * user sees the menu dismiss immediately even if the backend takes
   * a few milliseconds to write the representation. The
   * `pasteInFlight` guard coalesces concurrent activations on the
   * same row (a double click on the menu trigger or on the menu
   * item itself becomes a no-op while the round-trip is in flight).
   */
  async function runMenuCopyForEntry(
    entryId: number,
    mode: CopyMode,
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
      await runCopyForEntry(entryId, mode, { hideAfterSuccess: false });
    } finally {
      const reduced = new Set(pasteInFlight);
      reduced.delete(entryId);
      pasteInFlight = reduced;
    }
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
    menuPositionStyle = "";
  }

  function toggleMenuFor(entryId: number): void {
    if (openMenuEntryId === entryId) {
      openMenuEntryId = null;
      menuPositionStyle = "";
      return;
    }
    openMenuEntryId = entryId;
    // Compute the menu's viewport position the same tick the row
    // renders. `tick()` is awaited from the click handler so the
    // anchor element is always mounted before the helper reads
    // its bounding box. The menu itself lives outside the row
    // (portalised into the list container) so the row's
    // `overflow: hidden` cannot clip the popover.
    queueMicrotask(() => {
      recomputeMenuPosition();
    });
  }

  /**
   * Re-anchor the open menu popover to the trigger element of the
   * currently selected entry. The helper measures the trigger
   * with `getBoundingClientRect` and computes a `position: fixed`
   * rectangle that:
   *
   * - stays within the `720 × 520` Quick Paste viewport so it
   *   never escapes the fixed window even when the trigger is the
   *   last visible row;
   * - anchors to the bottom-left of the trigger (`top` set to the
   *   trigger's bottom edge, `right` aligned to its left edge)
   *   so the popover reads as a continuation of the row, but
   *   flips to the top of the trigger when there is no room
   *   below;
   * - collapses to the documented 12rem width and bumps back to
   *   the bottom edge when the computed `top` would render the
   *   popover above the list region.
   *
   * The trigger element is rendered inline through a hidden
   * `qp-menu-anchor` button the row mounts when the menu opens;
   * the helper tolerates a missing trigger (e.g. during a list
   * recompute before the next render) by collapsing the menu
   * back to `top: 0; left: 0` so the visible surface never hangs
   * off-screen.
   */
  function recomputeMenuPosition(): void {
    if (openMenuEntryId === null) {
      menuPositionStyle = "";
      return;
    }
    const anchor = menuAnchorEls[openMenuEntryId];
    if (!anchor || typeof anchor.getBoundingClientRect !== "function") {
      menuPositionStyle = "";
      return;
    }
    const rect = anchor.getBoundingClientRect();
    // The popover reserves a fixed 12rem (~192px) column; the
    // pre-computed width mirrors the CSS rule so a future change
    // to `.qp-menu` cannot drift past the helper.
    const POPOVER_WIDTH = 192;
    const VIEWPORT_WIDTH = 720;
    const VIEWPORT_HEIGHT = 520;
    // Left edge aligned to the trigger's right minus its declared
    // width; clamped so the popover never escapes the window.
    const desiredLeft = Math.max(
      8,
      rect.right - POPOVER_WIDTH,
    );
    const left = Math.min(desiredLeft, VIEWPORT_WIDTH - POPOVER_WIDTH - 8);
    // The popover drops below the trigger when there is at least
    // ~160px of room; otherwise it flips above so the last row
    // always anchors the actions into the viewport.
    const POPOVER_MIN_HEIGHT = 160;
    const belowTop = rect.bottom + 4;
    const aboveTop = rect.top - 4;
    const flipsAbove =
      belowTop + POPOVER_MIN_HEIGHT > VIEWPORT_HEIGHT &&
      aboveTop - POPOVER_MIN_HEIGHT >= 8;
    const top = flipsAbove ? Math.max(8, aboveTop - POPOVER_MIN_HEIGHT) : belowTop;
    menuPositionStyle = `top: ${top}px; left: ${left}px;`;
  }

  /**
   * Menu keyboard affordances. The helper only owns the in-menu
   * navigation (`Escape` to close); the `...` trigger itself is a
   * `button` so the platform activation (`Enter`/`Space`) opens
   * the menu through the same `click` handler as a mouse. The
   * `Cmd/Ctrl+K` shortcut the window installs never reaches the
   * menu because it lives on the `<svelte:window>` listener and
   * only fires when the focus is on the list / body.
   */
  function onMenuKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      closeMenu();
    }
  }

  /**
   * Open-window `pointerdown` handler that closes the menu when a
   * pointer lands outside the menu surface. The listener lives on
   * `<svelte:window>` so it survives the row remounts the list
   * runs while the user is searching; the attached guard makes
   * sure the menu only closes when the pointer is actually
   * outside the menu (a click inside it bubbles to the menu item
   * handlers and to the row's own `on:click|stopPropagation`
   * controls).
   */
  function onWindowPointerDown(event: PointerEvent): void {
    if (openMenuEntryId === null) return;
    const target = event.target;
    if (target instanceof Node && menuEl && menuEl.contains(target)) {
      return;
    }
    if (target instanceof Node && menuAnchorEls[openMenuEntryId]) {
      const anchor = menuAnchorEls[openMenuEntryId];
      if (anchor && target instanceof Node && anchor.contains(target)) {
        return;
      }
    }
    closeMenu();
  }

  /**
   * The Quick Paste menu exposes the copy-only actions for every
   * entry. The helper dispatches the copy through
   * `runMenuCopyForEntry` so the menu shares the same
   * `performCopyFlow` controller Enter / Shift+Enter / row click
   * use, only with `hideAfterSuccess: false`. The window stays
   * visible while the representation reaches the clipboard and the
   * previously focused application can paste it with `Cmd/Ctrl+V`.
   */
  function runMenuAction(entryId: number, mode: CopyMode): void {
    void runMenuCopyForEntry(entryId, mode);
  }

  /**
   * Open the preview overlay for the supplied entry id. The helper is
   * the single switch the menu `Previsualizar` action and the
   * `Cmd/Ctrl+Enter` keyboard shortcut consult so the two surfaces
   * cannot drift apart. The overlay is strictly read-only — the
   * helper never writes to the clipboard, never invokes the paste
   * command and never mutates the entry.
   */
  function openPreviewFor(entryId: number): void {
    const entry = findEntry(mode, recent, hits, entryId);
    if (!entry) return;
    previewEntryId = entryId;
    openMenuEntryId = null;
  }

  function closePreview(): void {
    previewEntryId = null;
  }

  function previewEntry(): EntryRecord | null {
    if (previewEntryId === null) return null;
    return findEntry(mode, recent, hits, previewEntryId);
  }

  function onWindowKeydown(event: KeyboardEvent): void {
    if (guidance) return;
    // The preview overlay is the only modal layered on top of Quick
    // Paste. `Escape` MUST close the preview first so the user can
    // dismiss it without losing the window; a second `Escape` falls
    // through to the legacy Quick Paste handler below.
    if (previewEntryId !== null && event.key === "Escape") {
      event.preventDefault();
      closePreview();
      return;
    }
    // `Cmd/Ctrl+K` focuses the existing search input and selects the
    // current query so the user can overwrite it without losing the
    // platform-correct shortcut. The shortcut is scoped to the
    // active Quick Paste window — no second global listener is
    // installed.
    if (matchesQuickPasteSearchShortcut(event, shortcutPlatform)) {
      event.preventDefault();
      focusSearchInput(true);
      return;
    }
    // `Cmd/Ctrl+Enter` opens the preview overlay for the selected
    // entry. The shortcut is read-only and never writes to the
    // clipboard; the search input remains a typing surface, so the
    // activation only fires when the focus is on the list, the body
    // or the search field itself. The `desktop-card-preview` change
    // promotes the matcher to a shared helper consumed by Quick
    // Paste and the Desktop rail so the two surfaces cannot drift.
    if (matchesQuickPastePreviewShortcut(event, shortcutPlatform)) {
      const target = event.target as HTMLElement | null;
      if (
        target instanceof HTMLTextAreaElement ||
        (target instanceof HTMLElement && target.isContentEditable)
      ) {
        return;
      }
      if (selectedIndex >= 0 && selectedIndex < resultIds.length) {
        event.preventDefault();
        const id = resultIds[selectedIndex];
        if (id !== undefined) {
          openPreviewFor(id);
        }
      }
      return;
    }
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

  /**
   * Whether the keyboard event matches the platform-specific search
   * shortcut the Quick Paste window installs. Mirrors the
   * `searchShortcut.ts` helper so the listener stays a thin matcher
   * without duplicating the modifier table.
   */
  function matchesQuickPasteSearchShortcut(
    event: KeyboardEvent,
    platform: SearchShortcutPlatform,
  ): boolean {
    if (event.altKey || event.shiftKey) return false;
    const key = (event.key ?? "").toLowerCase();
    if (key !== "k") return false;
    return platform === "macos"
      ? Boolean(event.metaKey) && !event.ctrlKey
      : Boolean(event.ctrlKey) && !event.metaKey;
  }

  /**
   * Whether the keyboard event matches the preview shortcut
   * (`Cmd+Enter` on macOS, `Ctrl+Enter` elsewhere). The helper
   * delegates to `matchesPreviewShortcut` from `lib/clipboardPreview.ts`
   * so the Quick Paste window and the Desktop rail consume one
   * shared matcher; the wrapper exists so the legacy
   * `quick-paste-preview-ui` regression suite keeps reading the
   * inline `function matchesQuickPastePreviewShortcut` declaration.
   */
  function matchesQuickPastePreviewShortcut(
    event: KeyboardEvent,
    platform: SearchShortcutPlatform,
  ): boolean {
    return matchesPreviewShortcut(event, platform);
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
   * Handle returned by `installWindowFocusClose` so the unmount path
   * can detach the single window-focus listener. The listener is
   * registered exactly once on `onMount`; remounts reuse the same
   * idempotent flag so two `<svelte:window>` listeners can never
   * stack and race the hide-on-blur branch.
   */
  let unlistenFocusClose: (() => void) | null = null;
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

  function focusSearchInput(selectAll = false): void {
    if (!searchInputEl) return;
    searchInputEl.focus();
    if (selectAll) {
      // The `Cmd/Ctrl+K` shortcut expects the existing query to be
      // selected so the user can overwrite it immediately. Fall back
      // to caret-at-end when `setSelectionRange` is unavailable so
      // the field stays usable on every supported host.
      const length = searchInputEl.value.length;
      try {
        searchInputEl.setSelectionRange(0, length);
        return;
      } catch {
        // ignore and fall through to the caret-at-end behaviour.
      }
    }
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

  /**
   * Subscribe to the Quick Paste window's OS-level focus changes
   * through `getCurrentWindow().onFocusChanged`. The Tauri runtime
   * fires the callback with `payload: boolean` whenever the window
   * gains or loses OS-level focus — gaining focus (`focused = true`)
   * is a no-op for Quick Paste and losing focus (`focused = false`)
   * must hide the palette. This API is the canonical Tauri 2 way to
   * observe window focus and replaces the previous
   * raw blur event handler that produced an unreliable
   * hide in the binary build.
   *
   * Why `onFocusChanged` and not the raw blur event:
   *
   * - `getCurrentWindow()` resolves the *current* webview's window
   *   automatically; the helper does not need a `target` qualifier
   *   because it is rooted in the webview that imports it. The
   *   previous implementation passed `{ target: QUICK_PASTE_WINDOW_LABEL }`
   *   to `listen` and the listener did not fire on the binary build.
   * - The callback exposes a single typed `focused: boolean` payload
   *   so the helper can branch on `focused === false` deterministically;
   *   the previous raw event had no payload and forced every consumer
   *   to treat every emit as a hide.
   * - The helper installs exactly one listener per mount and returns
   *   the unlisten handle so `onDestroy` always tears it down. A
   *   remount returns the existing handle, making the registration
   *   idempotent — the `unlistenFocusClose` slot the parent component
   *   owns can never stack.
   * - The helper guards the Tauri runtime with the same
   *   `window.__TAURI_INTERNALS__` check `safeListenOpened` uses so a
   *   non-Tauri test runtime never crashes; the helper then no-ops.
   *
   * Internal interactions (search input focus, row focus, menu
   * open/close, preview open/close, pin toggle) DO NOT trip the
   * listener — those events are DOM-level focus mutations that the
   * OS-level window never observes.
   */
  function safeListenWindowFocus(handler: (focused: boolean) => void): () => void {
    if (!window.__TAURI_INTERNALS__) return () => undefined;
    let unlisten: (() => void) | null = null;
    let disposed = false;
    void import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => getCurrentWindow())
      .then((windowHandle) =>
        windowHandle.onFocusChanged(({ payload: focused }) => {
          if (disposed) {
            // Defensive: a focus event that lands after the unmount
            // path detached the listener must never reach the hide
            // branch (otherwise `hideQuickPasteWindow` could fire
            // against a stale window reference and re-open the
            // palette as a side effect of the focus event itself).
            return;
          }
          try {
            handler(focused);
          } catch (error) {
            console.error("quick-paste focus handler threw", error);
          }
        }),
      )
      .then((stop) => {
        if (disposed) {
          // The unmount path ran while we were awaiting the focus
          // subscription; detach immediately so the listener never
          // reaches the handler.
          try {
            stop();
          } catch {
            // Best-effort: the runtime may have already torn down the
            // subscription.
          }
          return;
        }
        unlisten = stop;
      })
      .catch((error) => {
        console.error("failed to listen for quick-paste window focus", error);
      });
    return () => {
      disposed = true;
      if (unlisten) {
        try {
          unlisten();
        } catch {
          // Best-effort: the runtime may have already torn down the
          // subscription.
        }
        unlisten = null;
      }
    };
  }

  /**
   * Visible label the search surface renders next to the field so
   * the user can see the platform-correct shortcut without reading
   * the docs. The Quick Paste window matches `Cmd/Ctrl+K` (the same
   * modifier table as the desktop rail's `Cmd/Ctrl+F`) so the badge
   * derives from `quickPasteSearchShortcutLabel` instead of the
   * `F`-key helper the main window uses.
   */
  $: shortcutLabelText = quickPasteSearchShortcutLabel(shortcutPlatform);
  $: shortcutAccessibleLabel = quickPasteSearchShortcutAccessibleLabel(
    shortcutPlatform,
  );

  /**
   * Load the platform the Quick Paste window was opened on. The
   * helper is best-effort: the listener only listens when the
   * window is active, so a missing capability collapses to
   * `"other"` (Linux / unknown) instead of surfacing an error to
   * the user. The label and the matcher both read the same value,
   * which is the only state the shortcut layer cares about.
   */
  async function loadShortcutPlatform(): Promise<void> {
    try {
      const diagnostics = await diagnosticsCommand();
      shortcutPlatform = searchShortcutPlatform(diagnostics.platform_os);
    } catch {
      shortcutPlatform = "other";
    }
  }

  onMount(() => {
    void loadShortcutPlatform();
    void loadRecent();
    unlistenOpened = safeListenOpened(() => {
      void onQuickPasteOpened();
    });
    // Window focus listener: when the OS-level window loses focus
    // to another application, hide the Quick Paste window. The
    // listener is installed through
    // `getCurrentWindow().onFocusChanged` (Tauri 2's documented
    // focus API) and only fires on OS-level focus handoffs, so an
    // internal interaction (search input, menu, preview, pin)
    // never reaches the hide branch. The listener is idempotent —
    // remounts return the existing unlisten handle so two focus
    // listeners can never stack.
    //
    // The handler also guards against the focus event that fires
    // *because* we just hid the window: `hide()` calls into the
    // Tauri shell, which can briefly clear focus, fire the focus
    // event again, and re-enter the handler. The `isHiding` flag
    // collapses that round-trip into a single hide so we never
    // schedule a second hide while the first is in flight.
    let isHiding = false;
    unlistenFocusClose = safeListenWindowFocus((focused) => {
      if (focused) return;
      if (isHiding) return;
      isHiding = true;
      void hideQuickPasteWindow()
        .catch(() => {
          // Best-effort: hiding is idempotent. Failure is non-fatal.
        })
        .finally(() => {
          // Release the guard on the next microtask so a future
          // external focus event can hide the palette again.
          queueMicrotask(() => {
            isHiding = false;
          });
        });
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
    if (unlistenFocusClose) {
      unlistenFocusClose();
      unlistenFocusClose = null;
    }
    // Revoke every thumbnail and source-app icon blob URL the list
    // minted so a long-lived window does not leak memory.
    assetResolver.release();
    appIconResolver.release();
    thumbnails = {};
    thumbnailStates = {};
    appIconUrls = {};
    appIconStates = {};
    previewEntryId = null;
  });
</script>

<svelte:head>
  {@html `<style data-clipvault-visual-tokens>:root{${VISUAL_TOKEN_ROOT_CSS}}</style>`}
</svelte:head>

<svelte:window on:keydown={onWindowKeydown} on:pointerdown={onWindowPointerDown} />

<main data-testid="quick-paste-root">
  <div class="qp-search-row">
    <div class="qp-search-shell" data-testid="quick-paste-search-shell">
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
      <span
        class="qp-search-hint"
        data-testid="quick-paste-search-hint"
        data-shortcut-platform={shortcutPlatform}
        aria-label={shortcutAccessibleLabel}
        title={shortcutAccessibleLabel}
      >
        {shortcutLabelText}
      </span>
    </div>
  </div>

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
                  width="20"
                  height="20"
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
                  width="18"
                  height="18"
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
                data-app-icon-state={entry && entry.source_app_icon_ref
                  ? (appIconStates[id] ?? "loading")
                  : "absent"}
                title={sourceAppLabel}
                aria-label={sourceAppLabel}
              >
                {#if appIconUrls[id] && (appIconStates[id] ?? "loading") === "loaded"}
                  <img
                    class="qp-source-app-img"
                    src={appIconUrls[id]}
                    alt=""
                    aria-hidden="true"
                    data-testid="quick-paste-source-app-icon"
                    on:error={() =>
                      dropAppIcon(id, entry?.source_app_icon_ref ?? null)}
                  />
                {:else if entry && entry.source_app_icon_ref && (appIconStates[id] ?? "loading") === "loading"}
                  <span
                    class="qp-source-app-placeholder"
                    data-testid="quick-paste-source-app-loading"
                    aria-label="Cargando icono"
                  >
                    <svg
                      aria-hidden="true"
                      focusable="false"
                      width="18"
                      height="18"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="1.6"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                    >
                      <rect x="4" y="4" width="16" height="16" rx="3" />
                      <path d="M9 9h6v6H9z" />
                    </svg>
                  </span>
                {:else}
                  <span
                    class="qp-source-app-fallback"
                    data-testid="quick-paste-source-app-fallback"
                    aria-hidden="true"
                  >
                    {@html APP_FALLBACK_ICON_SVG}
                  </span>
                {/if}
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
              <button
                type="button"
                class="qp-menu-anchor"
                data-testid="quick-paste-menu-anchor"
                data-entry-id={id}
                bind:this={menuAnchorEls[id]}
                aria-hidden="true"
                tabindex="-1"
                on:click|stopPropagation={() => undefined}
              ></button>
            {/if}
          </li>
        {/each}
      </ul>
      {#if openMenuEntryId !== null}
        {@const anchorEntry = findEntry(mode, recent, hits, openMenuEntryId)}
        {#if anchorEntry}
          {@const anchorActions = quickPasteMenuActions(
            anchorEntry,
            renderTitle(mode, recent, hits, openMenuEntryId),
            {
              copyBusy:
                pasteInFlight.has(openMenuEntryId) ||
                pinInFlight.has(openMenuEntryId),
            },
          )}
          <ul
            class="qp-menu"
            role="menu"
            data-testid="quick-paste-menu"
            data-entry-id={openMenuEntryId}
            style={menuPositionStyle}
            bind:this={menuEl}
            on:keydown={onMenuKeydown}
          >
            {#each anchorActions as action (action.testId)}
              <li role="none">
                <button
                  type="button"
                  role="menuitem"
                  class="qp-menu-item"
                  class:qp-menu-item-preview={action.kind === "preview"}
                  data-testid={action.testId}
                  data-action-kind={action.kind}
                  disabled={action.disabled}
                  aria-label={action.ariaLabel}
                  title={action.tooltip}
                  on:click|stopPropagation={() =>
                    action.kind === "preview"
                      ? openPreviewFor(openMenuEntryId!)
                      : runMenuAction(openMenuEntryId!, action.mode)}
                >
                  {action.label}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      {/if}
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

{#if previewEntryId !== null}
  {@const previewRecord = previewEntry()}
  {#if previewRecord}
    <ClipboardPreview
      entry={previewRecord}
      testIdPrefix="quick-paste-preview"
      accessibleLabel={`Previsualización de ${renderTitle(mode, recent, hits, previewEntryId)}`}
      onClose={closePreview}
    />
  {/if}
{/if}

<!--
  Mount the content-type icon sprite once at the document root so
  every `<use href="#cv-icon-…">` Quick Paste renders resolves to a
  shape. The same sprite is mounted by `HistoryCard.svelte` so the
  Quick Paste palette and the desktop rail share the exact same
  glyph registry without a parallel implementation. The block is
  rendered after the visible tree to keep the rendered output
  deterministic.
-->
{@html CONTENT_TYPE_ICON_SPRITE}

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
  /*
   * Quick Paste runs inside its own Tauri webview (`quick-paste.html`)
   * so the `:root { --cv-* }` block `App.svelte` injects on the main
   * desktop window is NOT visible here. The component above mounts
   * the same string `lib/visualTokens.ts` emits through
   * `<svelte:head>` so every `var(--cv-*, fallback)` reference in
   * this stylesheet resolves to the same value the desktop rail
   * consumes. Two parallel `--cv-*` blocks would be a regression —
   * the regression suite pins the shared helper as the only writer.
   */

  :global(html, body) {
    margin: 0;
    padding: 0;
    background: var(--cv-bg-surface, #0e1116);
    color: var(--cv-fg, #f0f4f8);
    /* Reuse the documented `--cv-font-family` token (defined in
     * `visualTokens.ts`) so the Quick Paste palette matches the
     * desktop rail's typography token-for-token. A defensive
     * fallback is kept so the webview stays readable even when
     * the parent shell forgot to mount the token block. */
    font-family: var(--cv-font-family, -apple-system, BlinkMacSystemFont,
      "Segoe UI", system-ui, sans-serif);
    font-size: var(--cv-body, 0.9rem);
  }

  main {
    /* The compact UI fills the 720x520 transient window. Every
     * padding/inset is hand-tuned so the 72px rows fit without
     * horizontal scroll and the search input stays the visual
     * header. The rounded shell border lives on the visible body so
     * the geometry stays anchored to the documented `720 × 520`
     * rectangle while the corners read consistently with the rest
     * of the cards. The font tokens mirror the visual system so the
     * card surface reads identically to the desktop rail. The
     * `var(--cv-*, fallback)` lookups read the tokens the
     * `<svelte:head>` block above emits; the literal fallback
     * keeps the palette readable even when the global block has
     * not been emitted. */
    box-sizing: border-box;
    width: 100%;
    height: 100vh;
    padding: 0.75rem 0.85rem 0.85rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    overflow: hidden;
    user-select: none;
    -webkit-user-select: none;
    background: var(--cv-bg-surface, #0e1116);
    border-radius: 16px;
    border: 1px solid var(--cv-border-strong, #1f2937);
    font-family: var(--cv-font-family, inherit);
    color: var(--cv-fg, #f0f4f8);
  }

  .qp-search-row {
    /* The search row groups the field with the platform-aware
     * shortcut hint. The grid reserves a flexible column for the
     * shell so the inner badge never has to compete with the field
     * for horizontal space. The hint collapses to `Ctrl K` on
     * Linux and `⌘K` on macOS via `quickPasteSearchShortcutLabel`. */
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 0.45rem;
    align-items: center;
    flex: 0 0 auto;
  }

  .qp-search-shell {
    /* Relative wrapper anchoring the platform shortcut badge the
     * same way `DesktopToolbar.svelte` does: the input fills the
     * wrapper, the badge sits absolutely inside it with
     * `pointer-events: none` so clicks and the `Cmd/Ctrl+K`
     * shortcut both land on the input. The right padding on the
     * input keeps the typed query from rendering under the badge. */
    position: relative;
    display: flex;
    align-items: center;
    flex: 1 1 auto;
    min-width: 0;
  }

  .qp-search {
    /* The search field is the primary header of the palette. The
     * typography follows the compact-UI spec: 15-16px, slightly
     * larger than the row title so the user lands on the right
     * surface as soon as the window opens. The right padding
     * reserves the column the `.qp-search-hint` badge lives in. */
    width: 100%;
    padding: 0.45rem 4rem 0.45rem 0.7rem;
    background: #161b22;
    color: inherit;
    border: 1px solid #30363d;
    border-radius: 8px;
    font-family: inherit;
    /* The search field keeps the documented `--cv-body` scale so the
     * Quick Paste header reads at the same weight the desktop
     * toolbar uses. A future theme change updates the toolbar
     * (`--cv-body`) and the field in lock-step. */
    font-size: var(--cv-body, 0.9rem);
    line-height: 1.2;
    box-sizing: border-box;
  }

  .qp-search:focus {
    outline: none;
    border-color: #2563eb;
    box-shadow: 0 0 0 1px rgba(37, 99, 235, 0.5);
  }

  .qp-search-hint {
    /* The platform-aware shortcut hint. Anchored to the right
     * side of the shell so the field can grow without
     * colliding with the badge. The badge never intercepts a
     * click — `pointer-events: none` keeps the underlying input
     * always-reachable so the `Cmd/Ctrl+K` shortcut and the
     * search helper both hit the input directly. The font size
     * uses the documented `--cv-preview` token so the badge
     * reads at the same weight the captured-content previews do. */
    position: absolute;
    right: 0.5rem;
    top: 50%;
    transform: translateY(-50%);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 22px;
    padding: 0 0.5rem;
    border-radius: 6px;
    background: rgba(148, 163, 184, 0.14);
    border: 1px solid rgba(148, 163, 184, 0.25);
    color: #cbd5f5;
    font-size: var(--cv-preview, 0.72rem);
    font-weight: 600;
    line-height: 1;
    pointer-events: none;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
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
    /* Reuse the documented `--cv-muted` token so the empty /
     * loading / error band reads at the same weight the desktop
     * toast / status text consumes. */
    font-size: var(--cv-muted, 0.78rem);
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
     * button and the source-app icon. Each column reuses the
     * documented card footprints (`1.5rem` for the type, `1.65rem`
     * for the pin / source-app icons) so the Quick Paste row and
     * the desktop rail render the same effective icon size. The
     * title takes the remaining width and truncates with an
     * ellipsis instead of pushing the source-app icon out of the
     * visible area. */
    grid-template-columns: 1.5rem 1fr 1.65rem 1.65rem;
  }

  .qp-type {
    /* The content-type icon area reuses the documented
     * `HistoryCard.svelte` footprint: a 1.5rem square container
     * with a 20×20 SVG inside. The shared size is the
     * non-negotiable contract the `quick-paste-preview-ui` spec
     * pins so the Quick Paste list and the desktop rail render
     * the same effective icon. */
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.5rem;
    height: 1.5rem;
    flex: 0 0 1.5rem;
    border-radius: 6px;
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
    /* The row title reuses `--cv-control` (the documented token
     * `HistoryCard.svelte` consumes for its own title) so the
     * compact row reads at the same weight the desktop card
     * titles do. */
    font-size: var(--cv-control, 0.85rem);
    line-height: 1.2;
    font-weight: 600;
    color: inherit;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .qp-source-app {
    /* The source-application icon area reuses the documented
     * `HistoryCard.svelte` footprint: a 1.65rem square container,
     * the same `border-radius` and the same `border` colour so the
     * Quick Paste list and the desktop rail render the same
     * effective icon size. The shared resolver, the stable
     * `loading` / `loaded` / `error` states and the `object-fit:
     * contain` content contract mirror HistoryCard verbatim. */
    flex: 0 0 1.65rem;
    width: 1.65rem;
    height: 1.65rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.06);
    color: #94a3b8;
    overflow: hidden;
  }

  .qp-source-app-img {
    /* The resolved source-app icon. `object-fit: contain` keeps
     * the icon inside the 18px square without distorting a square
     * icon or cropping a tall icon — same contract the desktop
     * rail enforces for `HistoryCard`. */
    width: 100%;
    height: 100%;
    object-fit: contain;
    display: block;
  }

  .qp-source-app-placeholder {
    /* A distinguishable placeholder while the icon bridge is in
     * flight. NOT an empty square — the icon area must keep the
     * same footprint across `loading`, `loaded` and `error` so the
     * row never reflows while the user types. */
    width: 100%;
    height: 100%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: rgba(148, 163, 184, 0.55);
  }

  .qp-source-app-fallback {
    /* Generic application glyph the row renders when the metadata
     * bridge cannot deliver bytes. The row never surfaces the raw
     * bundle identifier; the accessible label carries that detail
     * for screen readers only. */
    width: 100%;
    height: 100%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: #94a3b8;
  }

  .qp-source-app-fallback :global(svg) {
    width: 100%;
    height: 100%;
  }

  .qp-row-active .qp-source-app {
    background: rgba(255, 255, 255, 0.18);
    color: #ffffff;
  }

  .qp-pin {
    /* The pin button lives in the metadata line, between the title
     * and the source-app icon. The 1.65rem footprint matches the
     * `HistoryCard.svelte` pin button (`.card-actions :global(.pin)`)
     * and the source-app icon column so the title truncates
     * instead of pushing any of the controls out of the visible
     * area. The button is a `button` element so keyboard
     * activation and screen-reader announcements follow the
     * documented accessible contract. */
    flex: 0 0 1.65rem;
    width: 1.65rem;
    height: 1.65rem;
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
    /* The row preview uses the documented `--cv-muted` token so the
     * compact row reads at the same weight the desktop metadata
     * strips consume. */
    font-size: var(--cv-muted, 0.78rem);
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
    /* The elapsed time uses the documented `--cv-tag` token so the
     * metadata strip reads at the same weight the desktop tag
     * chips consume. */
    font-size: var(--cv-tag, 0.65rem);
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
    /* Portalised popover. The menu lives OUTSIDE the row so the
     * fixed-height contract stays intact and the row's
     * `overflow: hidden` cannot clip the popover. The exact
     * rectangle is computed in JS (`menuPositionStyle`) and
     * anchored to the trigger button (`.qp-menu-anchor`) with a
     * collision-aware fallback that flips the popover above the
     * trigger when the bottom of the list would clip it. */
    position: fixed;
    z-index: 50;
    margin: 0;
    padding: 0.25rem;
    list-style: none;
    width: 12rem;
    background: #1f2937;
    color: #f0f4f8;
    border: 1px solid #30363d;
    border-radius: 6px;
    box-shadow: 0 8px 18px rgba(0, 0, 0, 0.45);
    max-height: calc(100vh - 1rem);
    overflow-y: auto;
  }

  /* The hidden anchor the menu's position is computed against.
   * Lives inline in the row only while the menu is open; its
   * `bind:this` reference becomes the trigger element the helper
   * reads through `getBoundingClientRect`. */
  .qp-menu-anchor {
    position: absolute;
    right: 0.4rem;
    bottom: 0.4rem;
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    margin: 0;
    background: transparent;
    cursor: default;
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

  .qp-menu-item-preview {
    /* The `Previsualizar` menu entry stays visually distinct from
     * the direct paste actions so the user can tell the read-only
     * affordance apart from the actions that mutate the clipboard. */
    color: #cbd5f5;
    font-style: italic;
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
