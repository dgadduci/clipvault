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
    clampedSelectedIndex,
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
    entryFullPreviewText,
    entryPreviewText,
    hasRenderableImage,
    isImageEntry,
  } from "./lib/clipboardAsset";
  import {
    matchesPreviewShortcut,
    previewShortcutAccessibleLabel,
    previewShortcutLabel,
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
  import { hydrateCodeLanguageForEntry } from "./lib/codeLanguageHydration";
  import {
    applyCodeLanguageToRecentAndHits,
    shouldShowCodeLanguageBadge,
  } from "./lib/codeLanguageProjections";
  import { canonicalLabel as canonicalCodeLanguageLabel } from "./lib/codeLanguageDetector";
  import { sourceAppIconCommand, diagnosticsCommand } from "./lib/tauri";
  import {
    entryTagsCommand,
    organizationSnapshotCommand,
  } from "./lib/tauri";
  import {
    applyQuickPasteTagsResult,
    bumpQuickPasteTagsToken,
    currentQuickPasteTagsToken,
    markQuickPasteTagsError,
    markQuickPasteTagsPending,
    resetQuickPasteTagsToken,
    truncateQuickPasteTags,
    type QuickPasteTagsCache,
    type QuickPasteTagsHydration,
  } from "./lib/quickPasteTags";
  import type { Tag } from "./types";
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
   * Explicit surface state machine. `list` is the canonical
   * capture-list surface; `preview` is the read-only `ClipboardPreview`
   * overlay. The Escape contract from `quick-paste/spec.md` keys
   * off this flag so the keyboard handler cannot accidentally
   * hide the window when the user is dismissing the preview.
   *
   * The flag is derived from `previewEntryId !== null` to keep a
   * single source of truth, but is exposed as a separate reactive
   * value so the keyboard handler branches on a stable boolean
   * the unit tests can assert without DOM access.
   */
  $: surface = previewEntryId !== null ? "preview" : "list";
  /**
   * One-shot guard the preview-close path sets so a single
   * `Escape` keystroke cannot both close the preview AND hide the
   * Quick Paste window. The `<ClipboardPreview>` overlay stops
   * propagation of its own `keydown` event as the primary fix;
   * this flag is the defence in depth that survives a future
   * regression that forgets the `stopPropagation()` call. The
   * flag is consumed by `onWindowKeydown` and reset on the next
   * animation frame so it never leaks past a single keystroke.
   */
  let suppressNextWindowEscape = false;
  /**
   * Platform the Quick Paste window was opened on. Resolved once
   * from the backend diagnostics so the visible `Cmd/Ctrl+K` hint
   * and the keyboard matcher stay in lockstep without a second
   * global listener.
   */
  let shortcutPlatform: SearchShortcutPlatform = "other";

  // Fixed dimensions of the result item. The row geometry the
  // `quick-paste-desktop-polish` change pins:
  //
  // ```text
  // item
  // ├── title-row              ← type, title, tags, pin, source-app
  // ├── capture-content        ← preview (exactly two lines)
  // └── footer/meta            ← code-language, elapsed, menu
  // ```
  //
  // The footer/meta is optional per the spec ("si corresponde") but
  // the Quick Paste palette renders the elapsed time and the menu
  // trigger, so the row reserves a third track. Every entry — text,
  // image, loading, error, selected, hover, focus — shares the same
  // outer rectangle. Any change to the value MUST update the
  // matching CSS rule on `.qp-row` AND the tests under
  // `tests/quickPasteDesktopPolishRegressions.test.ts` and
  // `tests/quickPasteCompact.test.ts`.
  //
  // The total height is computed explicitly from the track sizes,
  // padding, gaps and borders (see `QP_ROW_HEIGHT_PX` below) so a
  // regression that drifts any single constant surfaces before the
  // user sees a row that no longer fits inside the documented
  // 720×520 window. The previous round pinned `ROW_HEIGHT_PX = 80`
  // but declared the tracks via `2lh`, which resolved against
  // `CAPTURE_LINE_HEIGHT_REM × 16px ≈ 30.4px`; combined with the
  // title-row, footer, padding, gap and borders the inner sum was
  // 85.6px — 5.6px larger than the 80px outer rectangle. The row
  // visibly overflowed and the second reserved line was effectively
  // clipped by the row's `overflow: hidden`. The current geometry
  // replaces `2lh` with the explicit `QP_CAPTURE_CONTENT_HEIGHT_PX`
  // track and recomputes `ROW_HEIGHT_PX` from the parts so the
  // inner sum always fits the outer rectangle by construction.
  const TITLE_ROW_HEIGHT_PX = 24;
  /**
   * Per-line height of `capture-content`. The value matches
   * `--cv-preview` (0.72rem) so the truncation contract is shared
   * with the desktop rail. Combined with `CAPTURE_LINE_HEIGHT_PX`
   * below it gives the `2 × line-height` reservation the
   * capture-content track needs to guarantee a short preview
   * paints on the first line while keeping the second line
   * reserved.
   */
  const CAPTURE_LINE_HEIGHT_REM = 0.95;
  /**
   * Pixel value of `CAPTURE_LINE_HEIGHT_REM` evaluated against the
   * document's default font-size (16px). The constant is the
   * single source of truth for the capture-content track height so
   * the grid template and the `min/max-height` clamps on
   * `.qp-row-line-body` resolve to the same value by construction
   * (no more `2lh` against a line-height that the rest of the
   * stylesheet can drift past).
   */
  const CAPTURE_LINE_HEIGHT_PX = Math.round(CAPTURE_LINE_HEIGHT_REM * 16);
  /**
   * Height the capture-content track reserves for the preview.
   * Equals `2 × CAPTURE_LINE_HEIGHT_PX` and replaces the previous
   * `2lh` declaration in both the grid template and the body
   * `min-height` / `max-height` clamps so a long preview truncates
   * inside the row instead of growing the row.
   */
  const CAPTURE_CONTENT_HEIGHT_PX = CAPTURE_LINE_HEIGHT_PX * 2;
  /**
   * Fixed height of the footer/meta track that hosts the code
   * language badge, the elapsed-time label and the menu trigger.
   * The footer stays outside `capture-content` so the time and the
   * menu can never count as a content line — the second visual
   * line of `capture-content` belongs exclusively to the preview.
   */
  const FOOTER_HEIGHT_PX = 18;
  /**
   * Top + bottom padding the row reserves. The CSS declaration
   * `padding: 0.3rem 0.55rem` applies 0.3rem vertically; the
   * constant rounds the value up to integer pixels so the test
   * suite can assert the exact sum.
   */
  const ROW_PADDING_VERTICAL_PX = Math.round(0.3 * 2 * 16);
  /**
   * Sum of the two grid gaps (0.05rem × 2 = 0.1rem ≈ 1.6px). The
   * constant rounds the value up so a regression that drifts the
   * gap surfaces before the user sees the second line clipped by
   * the row's `overflow: hidden`.
   */
  const ROW_GAP_TOTAL_PX = Math.round(0.05 * 2 * 16);
  /**
   * Sum of the top + bottom 1px borders the row carries.
   */
  const ROW_BORDER_PX = 2;
    /**
     * Final, fully-derived row height. The constant is the single
     * arithmetic expression the geometry tests assert against so
     * future contributors can move a track, a padding or a gap
     * without having to redo the math by hand.
     *
     * ```text
     * ROW_HEIGHT_PX =
     *     TITLE_ROW_HEIGHT_PX
     *   + CAPTURE_CONTENT_HEIGHT_PX
     *   + FOOTER_HEIGHT_PX
     *   + ROW_PADDING_VERTICAL_PX
     *   + ROW_GAP_TOTAL_PX
     *   + ROW_BORDER_PX
     * =   24 + 30 + 18 + 10 + 2 + 2
     * =   86
     * ```
     */
    const ROW_HEIGHT_PX =
      TITLE_ROW_HEIGHT_PX +
      CAPTURE_CONTENT_HEIGHT_PX +
      FOOTER_HEIGHT_PX +
      ROW_PADDING_VERTICAL_PX +
      ROW_GAP_TOTAL_PX +
      ROW_BORDER_PX;

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
      void hydrateCodeLanguages();
      void hydrateTagsForVisibleEntries(recent);
    } catch (err) {
      searchError = err instanceof Error ? err.message : String(err);
      loading = false;
    }
  }

  // ---------------------------------------------------------------
  // Tag hydration for the Quick Paste rows.
  //
  // The Quick Paste window is a separate webview so it cannot
  // reuse the `entryOrganization` map `App.svelte` already keeps
  // for the desktop rail. The rows need to surface the same tag
  // chips on the title line so the user can recognise a capture
  // at a glance; the helpers in `lib/quickPasteTags.ts` own the
  // cache transitions and the stale-response guard so a refresh
  // that lands while a previous round is still in flight cannot
  // attach a tag set to a row that is no longer in scope.
  //
  // The hydration contract:
  //   - `entryTagsCache` and `entryTagsHydration` are the only
  //     reactive writers the rows consult. They MUST be reassigned
  //     (= new Map(...)) after every change so Svelte's reactivity
  //     picks the change up — mutating them in place is invisible.
  //   - The snapshot the bridge round-trip reads comes from
  //     `organizationSnapshotCommand`. The command is metadata-only:
  //     the response carries no clipboard content, hashes, asset
  //     references or paths.
  //   - The per-entry token table (`quickPasteTags.ts`) is the
  //     single switch the stale-response guard reads. A response
  //     whose `token` no longer matches the live counter is dropped
  //     silently so a late commit never attaches tags to an entry
  //     that left the visible scope.
  //   - A row's hydration state never lowers the row height: the
  //     chips row collapses to `0` when the entry has no tags or
  //     when the bridge round is in flight, and the `+N` indicator
  //     truncates a long tag set on the same line. A failure state
  //     silently omits chips without showing errors as visible
  //     content.
  // ---------------------------------------------------------------

  let entryTagsCache: QuickPasteTagsCache = new Map();
  let entryTagsHydration: QuickPasteTagsHydration = new Map();
  let knownTags: Tag[] = [];
  let knownTagsHydrated = false;
  let knownTagsHydrationToken = 0;

  async function ensureKnownTagsLoaded(): Promise<Tag[]> {
    const token = ++knownTagsHydrationToken;
    if (knownTagsHydrated) return knownTags;
    try {
      const snapshot = await organizationSnapshotCommand();
      if (token !== knownTagsHydrationToken) {
        return knownTags;
      }
      knownTags = snapshot.tags;
      knownTagsHydrated = true;
      return knownTags;
    } catch {
      if (token === knownTagsHydrationToken) {
        knownTagsHydrated = false;
      }
      return [];
    }
  }

  async function hydrateTagsForEntry(entryId: number): Promise<void> {
    // The stale-response guard bumps the per-entry token so the
    // response below is the only one that can write to the cache.
    // The companion `currentQuickPasteTagsToken` check rejects a
    // late resolution that lands after the entry left the visible
    // scope (a refresh, a search, a fresh recents feed).
    const token = bumpQuickPasteTagsToken(entryId);
    entryTagsHydration = markQuickPasteTagsPending(entryTagsHydration, entryId);
    // The pre-load in `hydrateTagsForVisibleEntries` already
    // populated `knownTags` before this call started, so the local
    // `ensureKnownTagsLoaded()` here is defence in depth for direct
    // callers (a future refresh path, etc.) — the happy path hits
    // the cached `knownTagsHydrated === true` branch and resolves
    // synchronously.
    await ensureKnownTagsLoaded();
    try {
      const tagIds = await entryTagsCommand({ entryId });
      if (currentQuickPasteTagsToken(entryId) !== token) return;
      // The known-tags snapshot might have refreshed between the
      // token bump and the response. Re-derive against the latest
      // known tags so a freshly created tag the user added is
      // honoured by the row.
      const applied = applyQuickPasteTagsResult(
        entryTagsCache,
        entryTagsHydration,
        entryId,
        tagIds,
        knownTags,
      );
      entryTagsCache = applied.nextCache;
      entryTagsHydration = applied.nextHydration;
    } catch (error) {
      if (currentQuickPasteTagsToken(entryId) !== token) return;
      entryTagsHydration = markQuickPasteTagsError(
        entryTagsHydration,
        entryId,
      );
      console.warn(
        "quick-paste tags hydration failed:",
        error instanceof Error ? error.message : String(error),
      );
    }
  }

  /**
   * Walk the supplied entries and hydrate their tag chips in
   * parallel. The helper is the only place the Quick Paste window
   * schedules a tag round-trip; it NEVER fetches more than once per
   * entry id and silently skips an entry that already reached the
   * `"loaded"` state. A stale response cannot leak across rows
   * because every round-trip captures its own token before issuing
   * the bridge call.
   *
   * The snapshot pre-load is the documented fix for the
   * `quick-paste-desktop-polish` regression that left rows without
   * tags even when the backend returned ids the user just persisted:
   * without the pre-load, each parallel `hydrateTagsForEntry` race
   * for the snapshot fetch and only the last one to complete writes
   * a populated lookup into `knownTags`; the earlier entries land
   * with an empty cache even though the bridge payload carried the
   * tag ids. Pre-loading the snapshot once, then firing the parallel
   * entry round-trips, guarantees every apply call sees the
   * populated `knownTags` map.
   */
  async function hydrateTagsForVisibleEntries(
    entries: readonly EntryRecord[],
  ): Promise<void> {
    const targets = entries.filter(
      (entry) => entryTagsHydration.get(entry.id) !== "loaded",
    );
    if (targets.length === 0) return;
    // Pre-load the organization snapshot ONCE before the parallel
    // entry round-trips so every `applyQuickPasteTagsResult` call
    // resolves tag ids against the same populated lookup.
    await ensureKnownTagsLoaded();
    await Promise.all(targets.map((entry) => hydrateTagsForEntry(entry.id)));
  }

  /**
   * Reconcile the cache against the visible scope: an entry that
   * left the result list MUST release its slot so a future
   * re-addition never inherits the previous row's tag set. The
   * helper mirrors the `reconcileEntryOrganizationToVisible`
   * invariant `lib/entryOrganization.ts` exposes for the rail —
   * a single stale entry cannot survive a scope change.
   */
  function pruneTagsToVisibleEntries(
    visibleIds: ReadonlySet<number>,
  ): void {
    let nextCache = entryTagsCache;
    let nextHydration = entryTagsHydration;
    let cacheDirty = false;
    let hydrationDirty = false;
    for (const id of entryTagsCache.keys()) {
      if (!visibleIds.has(id)) {
        resetQuickPasteTagsToken(id);
        if (!cacheDirty) {
          nextCache = new Map(entryTagsCache);
          cacheDirty = true;
        }
        nextCache.delete(id);
      }
    }
    for (const id of entryTagsHydration.keys()) {
      if (!visibleIds.has(id)) {
        if (!hydrationDirty) {
          nextHydration = new Map(entryTagsHydration);
          hydrationDirty = true;
        }
        nextHydration.delete(id);
      }
    }
    if (cacheDirty) entryTagsCache = nextCache;
    if (hydrationDirty) entryTagsHydration = nextHydration;
  }

  $: {
    // Touch the cache and hydration maps at the call site so
    // Svelte's compiler marks them as dependencies of this
    // reactive block. Without these explicit reads the compiler
    // would only track `resultIds`, and the prune would never
    // re-run after the hydration populates the cache.
    const cache = entryTagsCache;
    const hydration = entryTagsHydration;
    const visibleIds = new Set(resultIds);
    pruneTagsToVisibleEntries(visibleIds);
    // Reference the captured locals so the compiler cannot elide
    // the dependency reads.
    void cache;
    void hydration;
  }

  /**
   * Tags the row template renders for the supplied entry id. The
   * helper is the single switch the markup consults: it returns
   * the truncated `Tag[]` projection when the entry has loaded
   * chips, an empty array when the entry has no tags or when the
   * bridge round is still in flight, and `null` only when the
   * bridge explicitly failed so the renderer can keep the row's
   * height stable without showing errors as visible content.
   *
   * The two reactive maps (`cache` and `hydration`) are explicit
   * parameters — NOT closed-over references — so the Svelte
   * compiler can detect them as dependencies of the call site
   * `{@const tagsProjection = tagsForEntry(id, entryTagsCache,
   * entryTagsHydration)}`. Without the explicit reference, the
   * `{@const}` would NOT re-evaluate when the maps change (the
   * compiler only tracks dependencies at the syntactic call
   * site, not through the function body), and the chips would
   * stay empty even after the hydration round populated the
   * cache. The previous round shipped the markup and the
   * hydration but the chips silently failed to appear because of
   * this exact reactivity gap.
   */
  function tagsForEntry(
    entryId: number,
    cache: QuickPasteTagsCache,
    hydration: QuickPasteTagsHydration,
  ): {
    chips: { tag: Tag; key: string }[];
    overflow: number;
  } | null {
    const hydrationState = hydration.get(entryId);
    if (hydrationState === "error") {
      return null;
    }
    const tags = cache.get(entryId) ?? [];
    const { visible, overflow } = truncateQuickPasteTags(tags, 2);
    return {
      chips: visible.map((tag) => ({ tag, key: `tag-${tag.id}` })),
      overflow,
    };
  }

  /**
   * Walk the freshly loaded `recent` feed and persist any pending
   * `code_language` classification the conservative detector
   * accepts. The helper coalesces requests per entry id, skips
   * already-classified rows and never forwards clipboard content.
   *
   * Every persisted classification patches `recent` and `hits`
   * through the pure helper
   * `applyCodeLanguageToRecentAndHits` so the row renders the
   * canonical language badge and the preview overlay mirrors the
   * same metadata. The patch is an immutable copy of the
   * affected row only, so titles, tags, favourites,
   * source-app, thumbnails and image metadata stay intact.
   */
  let codeLanguageHydrationToken = 0;
  async function hydrateCodeLanguages(): Promise<void> {
    const token = ++codeLanguageHydrationToken;
    for (const entry of recent) {
      if (token !== codeLanguageHydrationToken) return;
      try {
        const outcome = await hydrateCodeLanguageForEntry(entry);
        if (outcome.persisted && outcome.language !== null) {
          const patched = applyCodeLanguageToRecentAndHits(
            recent,
            hits,
            entry.id,
            outcome.language,
          );
          if (patched.applied) {
            recent = patched.nextRecent;
            hits = patched.nextHits;
          }
        }
      } catch (error) {
        console.warn(
          "code-language hydration failed:",
          error instanceof Error ? error.message : String(error),
        );
      }
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
      void hydrateTagsForVisibleEntries(
        response.hits.map((hit) => hit.record),
      );
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
    // The row preview MUST keep the captured whitespace byte-for-byte
    // (LF / CRLF / tabs / indentation / blank lines / significant
    // spaces) so the CSS clamp recipe can paint the document on up
    // to two visual lines (`display: -webkit-box` +
    // `-webkit-line-clamp: 2` + `white-space: pre-wrap`).
    //
    // The previous `entryPreviewText(entry, 80)` call replaced runs
    // of whitespace with a single space, trimmed the string and
    // truncated it to ~80 characters before the renderer saw it —
    // so even when the CSS reservation was correct (title-row +
    // capture-content + footer/meta) the visible content was a
    // single flattened line. The previous search-mode snippet
    // fallback (`hit.snippet`) had the same problem for any
    // highlighted match the `SearchService` had already collapsed.
    //
    // `entryFullPreviewText` is the named helper the shared
    // `<ClipboardPreview>` overlay already pins for the same
    // whitespace contract; the row preview reuses it so the row
    // and the overlay render the exact same characters. The search
    // snippet remains available for the preview overlay (which
    // renders the highlighted branch) but for the row surface we
    // prefer the canonical `EntryRecord` content so the visible
    // truncation is deterministic and the search ranking is never
    // coupled to the row geometry.
    void currentMode;
    void searchHits;
    return entryFullPreviewText(entry);
  }

  function moveSelection(delta: number): void {
    // Arrow keys CLAMP at the ends of the list. Wrapping with the
    // modulo operator would silently teleport the selection from
    // the last row back to the first (and vice versa) the moment the
    // user pressed Down on the last entry, which felt like a glitch
    // during the manual QA pass. Home / End are the documented
    // shortcuts for the wrap-around jump; ArrowUp / ArrowDown stay
    // on the boundary. The pure `clampedSelectedIndex` helper keeps
    // the math unit-testable without a DOM.
    const next = clampedSelectedIndex(resultIds, selectedIndex, delta);
    if (next < 0 || next === selectedIndex) {
      // No-op at the boundary or on an empty list: do NOT re-issue
      // scrollIntoView so the browser keeps the row steady; the
      // visible selection stays highlighted on the same row.
      return;
    }
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
   *
   * The list snapshot (`query` / `selectedEntryId` / `selectedIndex`)
   * is captured before the overlay mounts so the Escape-driven
   * `preview` → `list` transition can restore the same context
   * even if a refresh / search round lands while the overlay is
   * open. The snapshot is intentionally read-only — `closePreview`
   * never writes the captured values back into the live variables,
   * the live state is already authoritative.
   */
  function openPreviewFor(entryId: number): void {
    const entry = findEntry(mode, recent, hits, entryId);
    if (!entry) return;
    previewEntryId = entryId;
    openMenuEntryId = null;
  }

  /**
   * Restore focus after the preview overlay closes so the user lands
   * back on the search input (the same surface the activated window
   * starts on) without losing the typed query or the selected row.
   * The helper is a best-effort no-op when the result list has not
   * mounted yet; the `selectedIndex` branch is reserved for a future
   * surface that exposes a per-row focus target.
   */
  function restoreFocusAfterPreview(): void {
    if (searchInputEl) {
      searchInputEl.focus();
    }
  }

  function closePreview(): void {
    previewEntryId = null;
    // Arm the one-shot guard so the bubbled `Escape` cannot reach
    // `handleEscape()` after the overlay closed. `ClipboardPreview`
    // already stops propagation as the primary fix; the flag is a
    // belt-and-braces fallback that survives a future regression.
    suppressNextWindowEscape = true;
    // Restore focus on the next microtask so the overlay has time
    // to unmount and the search input is the next focusable element
    // the keyboard flow expects. `tick()` would also work, but
    // `queueMicrotask` is cheaper and the focus call is idempotent.
    queueMicrotask(() => {
      restoreFocusAfterPreview();
      // Reset the guard a frame later so a follow-up `Escape`
      // pressed while the list is open still hides the window —
      // the guard consumes exactly the keystroke that closed the
      // preview, no more.
      requestAnimationFrame(() => {
        suppressNextWindowEscape = false;
      });
    });
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
    // through to the legacy Quick Paste handler below. The branch
    // consults the explicit `surface` state machine the
    // `quick-paste-desktop-polish` change pins so the keyboard
    // contract cannot drift from the visible surface.
    if (surface === "preview" && event.key === "Escape") {
      event.preventDefault();
      closePreview();
      return;
    }
    // Defence in depth: the overlay's `onOverlayKeydown` already
    // calls `event.stopPropagation()` so this branch is the
    // backstop for any future component that forgets to stop the
    // bubble. The guard consumes the single `Escape` keystroke
    // that just closed the preview and never lets it reach
    // `handleEscape()`, so the Quick Paste window stays open.
    if (event.key === "Escape" && suppressNextWindowEscape) {
      event.preventDefault();
      suppressNextWindowEscape = false;
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
   * Per-input keydown handler for the search field. The
   * `<svelte:window>` listener handles ArrowUp / ArrowDown when the
   * focus lives outside the field, but the user typically types
   * inside the search input and expects the keyboard shortcuts to
   * navigate the list while the caret stays inside the field.
   * Without this handler the browser default would only move the
   * caret to the end of the typed query on ArrowDown / ArrowUp;
   * the keyboard selection would NOT change.
   *
   * The handler is intentionally narrow: it ONLY routes the four
   * documented navigation keys (ArrowUp / ArrowDown / Home / End)
   * so a typing surface keeps every other keystroke (alphanumerics,
   * backspace, modifiers, IME composition) intact. The `Enter`
   * branch stays in the window listener so the search input can
   * still receive an explicit Enter (which is the documented
   * shortcut for "confirm the highlighted entry").
   *
   * `stopPropagation` is called for the four handled keys so the
   * window-level listener cannot fire on the same event — otherwise
   * the keyboard selection would advance by two rows on a single
   * ArrowDown press because both branches would call
   * `moveSelection(1)`. The window listener is still the canonical
   * handler for the same keys when the focus sits outside the
   * input (e.g. on the list or on the document body).
   */
  function onSearchInputKeydown(event: KeyboardEvent): void {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      event.stopPropagation();
      moveSelection(1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      event.stopPropagation();
      moveSelection(-1);
    } else if (event.key === "Home") {
      event.preventDefault();
      event.stopPropagation();
      jumpToFirst();
    } else if (event.key === "End") {
      event.preventDefault();
      event.stopPropagation();
      jumpToLast();
    }
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
        on:keydown={onSearchInputKeydown}
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
          {@const tagsProjection = tagsForEntry(id, entryTagsCache, entryTagsHydration)}
          <li
            class="qp-row"
            class:qp-row-active={index === selectedIndex}
            style="--qp-row-height: {ROW_HEIGHT_PX}px; --qp-title-row-height: {TITLE_ROW_HEIGHT_PX}px; --qp-capture-line-height: {CAPTURE_LINE_HEIGHT_PX}px; --qp-capture-content-height: {CAPTURE_CONTENT_HEIGHT_PX}px; --qp-footer-height: {FOOTER_HEIGHT_PX}px;"
            data-entry-id={id}
            data-testid="quick-paste-row"
            data-selected={index === selectedIndex ? "true" : "false"}
            data-content-type={contentType}
            data-thumb-state={isImage ? thumbState : "none"}
            data-row-height={ROW_HEIGHT_PX}
            data-row-title-height={TITLE_ROW_HEIGHT_PX}
            data-row-content-lines={2}
            data-row-content-height={CAPTURE_CONTENT_HEIGHT_PX}
            data-row-footer-height={FOOTER_HEIGHT_PX}
            data-pinned={isPinned ? "true" : "false"}
            role="option"
            aria-selected={index === selectedIndex}
            aria-label={title}
            title={title}
            on:click={(event) => handleRowClick(id, event)}
            bind:this={rowEls[index]}
          >
            <div class="qp-row-line qp-row-line-meta" data-testid="quick-paste-title-row">
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
              {#if tagsProjection && (tagsProjection.chips.length > 0 || tagsProjection.overflow > 0)}
                <span
                  class="qp-tags"
                  data-testid="quick-paste-tags"
                  data-tags-state={entryTagsHydration.get(id) ?? "loaded"}
                  aria-label="Tags de la entrada"
                >
                  {#each tagsProjection.chips as chip (chip.key)}
                    <span
                      class="qp-tag-chip"
                      data-testid="quick-paste-tag-chip"
                      data-tag-id={chip.tag.id}
                    >
                      {chip.tag.display_name}
                    </span>
                  {/each}
                  {#if tagsProjection.overflow > 0}
                    <span
                      class="qp-tag-chip qp-tag-chip-more"
                      data-testid="quick-paste-tag-more"
                      aria-label={`${tagsProjection.overflow} tags adicionales`}
                    >
                      +{tagsProjection.overflow}
                    </span>
                  {/if}
                </span>
              {/if}
              {#if index === selectedIndex}
                <span
                  class="qp-preview-hint"
                  data-testid="quick-paste-preview-hint"
                  data-preview-platform={shortcutPlatform}
                  title={previewShortcutAccessibleLabel(shortcutPlatform)}
                  aria-label={previewShortcutAccessibleLabel(shortcutPlatform)}
                  aria-keyshortcuts={shortcutPlatform === "macos"
                    ? "Meta+Enter"
                    : "Control+Enter"}
                >
                  <span class="qp-preview-hint-label">Preview</span>
                  <span
                    class="qp-preview-hint-keys"
                    aria-hidden="true"
                  >
                    {previewShortcutLabel(shortcutPlatform)}
                  </span>
                </span>
              {/if}
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
            <div class="qp-row-line qp-row-line-body" data-testid="quick-paste-capture-content">
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
            </div>
            <!--
              Footer/meta track that hosts the optional code-language
              badge, the elapsed-time label and the menu trigger. The
              footer lives OUTSIDE `capture-content` so the second
              visual line of the capture-content area always belongs
              exclusively to the preview — the time and the menu can
              never count as a content line, satisfying the
              `quick-paste-desktop-polish` requirement that the two
              visual lines of capture-content are reserved for the
              captured content alone.
            -->
            <div
              class="qp-row-line qp-row-line-footer"
              data-testid="quick-paste-row-footer"
            >
              {#if entry && shouldShowCodeLanguageBadge(entry)}
                <span
                  class="qp-code-language"
                  data-testid="quick-paste-code-language"
                  data-code-language={entry.code_language ?? ""}
                  title={`Lenguaje: ${canonicalCodeLanguageLabel(entry.code_language)}`}
                >
                  Código · {canonicalCodeLanguageLabel(entry.code_language)}
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
    /* The fixed-height row: every state (default, hover, selected,
     * focus, loading, error) keeps the same rectangle so the
     * neighbour rows never shift when a single entry's state
     * changes. The row is split into the documented three-region
     * geometry the `quick-paste-desktop-polish` change pins:
     *
     *   - `title-row` reserves a fixed `TITLE_ROW_HEIGHT_PX`
     *     line for the type icon, the title, the tag chips, the
     *     preview-shortcut hint and the pin / source-app controls;
     *   - `capture-content` reserves exactly two visual lines
     *     (`CAPTURE_CONTENT_HEIGHT_PX`) so a long preview
     *     truncates with an ellipsis instead of growing the row
     *     and a short preview still occupies the same footprint;
     *   - `footer/meta` reserves a fixed `FOOTER_HEIGHT_PX` track
     *     for the optional code-language badge, the elapsed-time
     *     label and the menu trigger so the second visual line of
     *     `capture-content` is reserved exclusively for the
     *     captured content (the time and the menu never count as
     *     content lines).
     *
     * The grid template is computed from the CSS custom properties
     * so a future refactor can move the constants without rewriting
     * every row selector. The capture-content track uses an
     * explicit pixel value (NOT `2lh`) so the grid reservation and
     * the body's `min-height` / `max-height` clamps resolve to the
     * same number by construction — `2lh` against a `line-height`
     * the rest of the stylesheet can drift past silently pushed
     * the inner sum past the outer rectangle (85.6px > 80px) and
     * the second reserved line was clipped by `overflow: hidden`.
     */
    box-sizing: border-box;
    height: var(--qp-row-height, 86px);
    min-height: var(--qp-row-height, 86px);
    max-height: var(--qp-row-height, 86px);
    display: grid;
    grid-template-rows:
      var(--qp-title-row-height, 24px)
      var(--qp-capture-content-height, 32px)
      var(--qp-footer-height, 18px);
    gap: 0.05rem;
    padding: 0.3rem 0.55rem;
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
    /* The metadata line carries the type icon, the title, the
     * tag chips (when the entry has any), the preview-shortcut
     * hint (only on the selected row), the pin button and the
     * source-app icon. Each fixed column reuses the documented
     * card footprints (`1.5rem` for the type, `1.65rem` for the
     * pin / source-app icons) so the Quick Paste row and the
     * desktop rail render the same effective icon size.
     *
     * The layout is a real CSS grid (not the inherited flex from
     * `.qp-row-line`) so the six-column template reserves the
     * exact column widths the spec pins. The `auto` columns for
     * tags and hint collapse to `0` when their children are
     * absent so the title still flexes in the `1fr` column and
     * the row never reserves room for an empty container.
     *
     * The six columns resolve as:
     *   type | title | tags | hint | pin | source-app
     * with two of them (tags, hint) collapsing to their content
     * width and reading as `0` when the row has nothing to show. */
    display: grid;
    grid-template-columns: 1.5rem minmax(0, 1fr) auto auto 1.65rem 1.65rem;
    align-items: center;
    column-gap: 0.45rem;
    min-height: 0;
    max-height: var(--qp-title-row-height, 24px);
    overflow: hidden;
  }

  .qp-tags {
    /* Compact tag chip row the `quick-paste-desktop-polish` change
     * introduces. The chips sit on the metadata line, immediately
     * after the title and before the preview-shortcut hint, so the
     * reading order stays type → title → tags → hint → pin → source-app.
     * The row collapses to `0` when the entry has no chips (the
     * markup wraps the chips in an `{#if}` guard) so a row with no
     * tags never reserves horizontal space.
     *
     * Each chip is `max-width: 7rem` and uses `text-overflow:
     * ellipsis` so a long display name never widens the row; the
     * `+N` chip collapses to the documented `--cv-tag` footprint
     * so a 12-tag entry still fits on the documented two-line
     * geometry. The whole row is `flex: 0 1 auto` so it shrinks
     * before the title does. */
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
    min-width: 0;
    flex: 0 1 auto;
    max-width: 12rem;
  }

  .qp-tag-chip {
    /* Compact chip that mirrors the desktop rail's
     * `HistoryCard.svelte` `.tag-chip` style without duplicating
     * the CSS file. The `--cv-tag` token keeps the typography in
     * lock-step with the rest of the metadata strip. The chip
     * overflows with an ellipsis instead of wrapping so the row
     * height stays stable. The chip is `pointer-events: none` so
     * a click on a chip never bubbles up and accidentally selects
     * the row.
     *
     * Visibility: the manual QA pass showed that the original
     * 0.65rem font + 12%-opacity blue palette was hard to read on
     * the dark surface. The chip uses a higher-contrast pair
     * (`#bfdbfe` on `rgba(147,197,253,0.22)`) and a slightly
     * larger footprint so the chip stays visible after hydration
     * without forcing a third line on the row. The chip still
     * fits inside the documented `title-row` 24px height because
     * `line-height: 1.1` × `0.72rem` font + 0.1rem vertical
     * padding = ~16px which is comfortably under the 24px cap. */
    flex: 0 1 auto;
    max-width: 7rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    background: rgba(147, 197, 253, 0.22);
    color: #bfdbfe;
    border: 1px solid rgba(147, 197, 253, 0.45);
    border-radius: 999px;
    padding: 0.1rem 0.5rem;
    font-size: var(--cv-tag, 0.72rem);
    line-height: 1.1;
    font-weight: 600;
    pointer-events: none;
    user-select: none;
    -webkit-user-select: none;
  }

  .qp-tag-chip-more {
    /* The `+N` indicator. Sits at the end of the chip row with a
     * slightly dimmer palette so the user can tell it apart from
     * the canonical tag chips. The accessible name carries the
     * overflow count so a screen reader announces the
     * information without depending on the literal `+N` glyph. */
    background: rgba(255, 255, 255, 0.10);
    color: #e2e8f0;
    border-color: rgba(148, 163, 184, 0.45);
    max-width: none;
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
    /* The capture-content line carries the preview (and the
     * thumbnail, for image entries) exclusively. The body line
     * MUST reserve exactly two visual lines so a row with a short
     * preview still occupies the same footprint as a row with a
     * long preview. The grid track is `CAPTURE_CONTENT_HEIGHT_PX`
     * (declared on the parent `.qp-row`); here we pin the
     * `min-height` / `max-height` to the same explicit pixel value
     * so the body's reservation and the grid track cannot drift
     * apart. The previous `2lh` declaration against
     * `var(--qp-capture-line-height, 0.95rem)` resolved to
     * `30.4px`, which combined with the title-row, footer,
     * padding, gap and borders pushed the inner sum to `85.6px`
     * — `5.6px` past the outer `80px` rectangle. The row's
     * `overflow: hidden` then clipped the second reserved line so
     * the visible footprint collapsed to a single line. The
     * explicit pixel values (`CAPTURE_CONTENT_HEIGHT_PX = 32`) and
     * the recomputed outer `ROW_HEIGHT_PX = 88` keep the inner sum
     * strictly inside the outer rectangle by construction.
     *
     * The body line is laid out as a flex row: the thumbnail
     * reserves a fixed 40x40px square for image entries and the
     * preview consumes the remaining space. Tags, code-language,
     * elapsed time and the menu trigger are NOT children of this
     * line — they live in the `footer/meta` track so the second
     * visual line of `capture-content` belongs exclusively to the
     * captured content (the time and the menu never count as a
     * content line, per the `quick-paste-desktop-polish` change).
     */
    display: flex;
    align-items: center;
    gap: 0.45rem;
    min-width: 0;
    min-height: var(--qp-capture-content-height, 32px);
    max-height: var(--qp-capture-content-height, 32px);
    line-height: var(--qp-capture-line-height, 15px);
    overflow: hidden;
  }

  .qp-preview {
    flex: 1 1 auto;
    min-width: 0;
    /* The row preview uses the documented `--cv-muted` token so the
     * compact row reads at the same weight the desktop metadata
     * strips consume. The preview MUST paint the captured block on
     * up to two visual lines so a short preview keeps the second
     * line reserved and a long preview truncates inside the row
     * without growing it. The clamp recipe is the cross-browser
     * combination `display: -webkit-box` +
     * `-webkit-box-orient: vertical` + `-webkit-line-clamp: 2`
     * (plus the modern `line-clamp: 2` equivalent so the same rule
     * applies once the property lands without the `-webkit-`
     * prefix).
     *
     * `white-space: pre-wrap` preserves the LF / CRLF / tabs /
     * indentation / blank lines / significant spaces the source
     * application produced so a multi-line capture paints exactly
     * as many lines as its content warrants and only the third
     * line is clipped. The previous `white-space: normal`
     * declaration collapsed the whitespace runs the row preview
     * fed it (LF → space, tabs → space, indentation → space,
     * empty lines gone) and the previous `entryPreviewText(entry,
     * 80)` call had already collapsed + truncated the string
     * upstream — so the CSS reservation could never show a real
     * second line. The helper now feeds the canonical
     * `entryFullPreviewText` string through the `pre-wrap` recipe
     * so the document renders byte-for-byte. The `line-height`
     * stays bound to `--qp-capture-line-height` so the clamp
     * truncates exactly on the second reserved line.
     *
     * `word-break: break-word` keeps long single tokens (URLs, hex
     * blobs) inside the row without widening the column;
     * `overflow: hidden` clips the third line. */
    font-size: var(--cv-muted, 0.78rem);
    line-height: var(--qp-capture-line-height, 15px);
    color: #cbd5f5;
    white-space: pre-wrap;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    word-break: break-word;
    text-overflow: ellipsis;
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

  .qp-row-line-footer {
    /* Footer/meta track that hosts the optional code-language
     * badge, the elapsed-time label and the menu trigger. The
     * footer lives OUTSIDE `capture-content` so the second visual
     * line of the capture-content area always belongs exclusively
     * to the preview — the time and the menu can never count as
     * a content line. The grid reserves a fixed-height track
     * (FOOTER_HEIGHT_PX) so the row geometry stays stable across
     * every entry shape (text, image, code with a badge).
     *
     * The footer grid distributes the available width with a
     * flexible spacer, then auto-width slots for the badge (when
     * present), the elapsed time and the menu trigger. The badge
     * column collapses to its natural width and is only present
     * when the entry carries a canonical code classification, so
     * non-code rows do not reserve any width for it. The menu
     * trigger takes a fixed 18px column so the footer never
     * shifts when the menu opens or closes. */
    display: grid;
    grid-template-columns: 1fr auto auto 18px;
    align-items: center;
    column-gap: 0.4rem;
    min-width: 0;
    min-height: 0;
    max-height: var(--qp-footer-height, 18px);
    overflow: hidden;
    font-size: var(--cv-tag, 0.65rem);
    line-height: 1.2;
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
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .qp-code-language {
    /* Compact language badge that mirrors the desktop rail's
     * `card-code-language` style. The badge reuses the same
     * `--cv-tag` token the rest of the metadata strip consumes so
     * the row reads at the same weight the desktop card
     * surfaces. The canonical `data-code-language` attribute is
     * what the bridge, the highlight.js grammar and the search
     * index consume; the visible copy is decorative. The badge
     * lives in the footer/meta track (NOT in `capture-content`)
     * so the second visual line of the content area is reserved
     * exclusively for the preview. */
    align-self: center;
    flex: 0 0 auto;
    padding: 0.05rem 0.4rem;
    border-radius: 4px;
    background: rgba(94, 234, 212, 0.18);
    color: #5eead4;
    font-size: var(--cv-tag, 0.65rem);
    font-weight: 500;
    line-height: 1.2;
    letter-spacing: 0.02em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 9rem;
  }

  .qp-row-active .qp-code-language {
    background: rgba(255, 255, 255, 0.22);
    color: #ecfeff;
  }

  .qp-row-active .qp-elapsed {
    color: rgba(255, 255, 255, 0.85);
  }

  .qp-menu-trigger {
    /* The ellipsis trigger lives in the footer/meta track, after
     * the elapsed-time column. The 18px footprint matches the
     * other control icons so the footer never shifts when the
     * menu opens or closes. The button is a `button` element
     * with the documented aria-haspopup contract. */
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

  /*
   * Compact preview-shortcut hint that surfaces only for the
   * currently selected row. The pill sits in the meta line AFTER
   * the title (between the title and the pin button) so the
   * visual reading order is type/icon → title → hint → pin →
   * source-app, matching the documented UX flow. The pill is
   * non-interactive (`pointer-events: none`) so a click on the
   * hint never accidentally stops the row's selection / copy
   * flow, and uses the same platform-aware matcher the
   * `Cmd/Ctrl+Enter` shortcut consults so the visible glyph and
   * the keyboard accelerator cannot drift apart. The row's
   * documented 72 px height stays untouched because every change
   * happens inside the meta line's intrinsic height, and the
   * `auto` column the hint lives in collapses to `0` whenever the
   * row is not selected so the type / pin / source-app icons
   * keep their fixed columns.
   */
  .qp-preview-hint {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.05rem 0.4rem;
    border-radius: 999px;
    background: rgba(96, 165, 250, 0.18);
    border: 1px solid rgba(96, 165, 250, 0.45);
    color: #cbd5f5;
    font-size: var(--cv-tag, 0.65rem);
    line-height: 1.1;
    pointer-events: none;
    user-select: none;
    -webkit-user-select: none;
    flex: 0 0 auto;
    max-width: 9rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .qp-preview-hint-label {
    font-weight: 600;
    color: #93c5fd;
  }
  .qp-preview-hint-keys {
    font-weight: 700;
    color: #f0f4f8;
    font-variant-numeric: tabular-nums;
  }
  .qp-row-active .qp-preview-hint {
    background: rgba(255, 255, 255, 0.22);
    border-color: rgba(255, 255, 255, 0.4);
    color: #ffffff;
  }
  .qp-row-active .qp-preview-hint-label {
    color: #ecfeff;
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
