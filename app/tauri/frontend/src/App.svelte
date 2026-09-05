<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import type {
    ActiveApplicationResponse,
    Capabilities,
    Collection,
    Diagnostics,
    EntryRecord,
    OrganizationSnapshot,
    PasteResponse,
    PlatformGuidance,
    SearchResponse,
    Tag,
  } from "./types";
  import {
    activeApplicationCommand,
    clearUnorganizedHistoryCommand,
    collectionsCreateCommand,
    collectionsDeleteCommand,
    collectionsRenameCommand,
    deleteEntryCommand,
    diagnosticsCommand,
    entryCollectionsCommand,
    entryCollectionsSetCommand,
    entryRemoveFromCollectionCommand,
    entryTagsCommand,
    entryTagsSetCommand,
    organizationSnapshotCommand,
    platformCapabilitiesCommand,
    recentEntriesCommand,
    recentEntriesFilteredCommand,
    refreshCapabilitiesCommand,
    searchEntriesCommand,
    setFavoriteCommand,
    unorganizedClearableCountCommand,
  } from "./lib/tauri";
  import { retryGuidance } from "./lib/guidance";
  import { runSearch } from "./lib/search";
  import {
    applyDestructive,
    isClearConfirmationRequired,
    isDeleteConfirmationRequired,
    type ManagementOutcome,
  } from "./lib/management";
  import {
    createQuickSearchRegistrar,
    defaultQuickPasteBridge,
  } from "./lib/quickPasteBridge";
  import { createHistoryUpdatedRegistrar } from "./lib/historyUpdates";
  import { createOrganizationUpdatedRegistrar } from "./lib/organizationUpdates";
  import {
    applyEntryOrganizationResults,
    applyPinUpdate,
    markEntriesPending,
    reconcileEntryOrganizationToVisible,
    selectPendingEntries,
    type EntryOrganizationFetchResult,
  } from "./lib/entryOrganization";
  import {
    matchesSearchShortcut,
    searchShortcutAccessibleLabel,
    searchShortcutLabel,
    searchShortcutPlatform,
  } from "./lib/searchShortcut";
  import { combineMemberships, hasActiveDragSession } from "./lib/dragAndDrop";
  import { installPointerDragController } from "./lib/pointerDragAndDrop";
  import PlatformGuidanceModal from "./PlatformGuidanceModal.svelte";
  import HistoryCardRail from "./HistoryCardRail.svelte";
  import OrganizationSidebar from "./OrganizationSidebar.svelte";
  import DesktopToolbar from "./DesktopToolbar.svelte";
  import Modal from "./Modal.svelte";
  import DevelopmentModal from "./DevelopmentModal.svelte";
  import PrivacyModal from "./PrivacyModal.svelte";
  import RetentionModal from "./RetentionModal.svelte";
  import QuickPasteShortcutModal from "./QuickPasteShortcutModal.svelte";

  type ModalId =
    | null
    | "development"
    | "privacy"
    | "retention"
    | "quick_paste_shortcut";

  let diagnostics: Diagnostics | null = null;
  let capabilities: Capabilities | null = null;
  let activeApp: ActiveApplicationResponse | null = null;
  let entries: EntryRecord[] = [];
  let error: string | null = null;
  let loading = true;
  let guidance: PlatformGuidance | null = null;
  let retryNotice: string | null = null;
  let retryError: string | null = null;
  let searchQuery = "";
  let visibleEntries: EntryRecord[] = [];
  let searching = false;
  let searchError: string | null = null;
  let searchStatus: "idle" | "loading" | "ok" | "empty" | "no_matches" = "idle";
  let unorganizedClearableCount: number | null = null;
  let unorganizedClearableCountLoaded = false;
  let unorganizedClearableCountLoading = false;
  let quickSearchListenerStatus: "registering" | "ready" | "error" = "registering";
  let quickSearchError: string | null = null;
  let searchController: { cancel: () => void } | null = null;
  /**
   * The shell registers exactly one shortcut listener for
   * `Cmd+F` (macOS) / `Ctrl+F` (Linux). The dispatch is anchored on
   * the diagnostics platform so the same helper decides what binding
   * to render in the toolbar — the listener cannot drift from the
   * hint the user sees.
   */
  let shortcutPlatform: ReturnType<typeof searchShortcutPlatform> =
    "other";
  let pendingConfirmation:
    | { kind: "delete"; id: number; label: string }
    | { kind: "clear"; count: number }
    | null = null;
  let openModal: ModalId = null;
  let modalReturnFocus: HTMLElement | null = null;

  let organization: OrganizationSnapshot | null = null;
  let selectedCollectionId: number | null = null;
  let historyCollectionId: number | null = null;
  let entryOrganization: Map<number, { tags: Tag[]; collections: Collection[] }> =
    new Map();
  type EntryOrganizationHydration = "pending" | "loaded" | "error";
  let entryOrganizationHydration: Map<number, EntryOrganizationHydration> =
    new Map();
  let entryOrganizationHydrationToken = 0;
  let organizationError: string | null = null;
  /**
   * Set of `(entryId, collectionId)` pairs the drop flow is
   * currently persisting. The set is the single switch the helper
   * `isDropInFlight` reads from so the sidebar never issues a
   * duplicate `entry_collections_set` for the same drag. Once the
   * `await entryCollectionsSetCommand` resolves (or rejects) the pair
   * is removed again so a subsequent drop on the same target can
   * still succeed.
   */
  let dropInFlight: Set<string> = new Set();
  const SEARCH_DEBOUNCE_MS = 120;
  const RAIL_LIMIT = 60;

  $: activeCollection = organization?.collections.find(
    (c) => c.id === selectedCollectionId,
  ) ?? null;
  $: activeCollectionIsHistory =
    activeCollection !== null && activeCollection.kind === "system";
  $: isFiltering = searchQuery.trim().length > 0;
  $: if (diagnostics) {
    shortcutPlatform = searchShortcutPlatform(diagnostics.platform_os);
  }
  $: searchShortcutLabelText = searchShortcutLabel(shortcutPlatform);
  $: searchShortcutAccessibleText =
    searchShortcutAccessibleLabel(shortcutPlatform);

  async function refresh(): Promise<void> {
    loading = true;
    error = null;
    try {
      diagnostics = await diagnosticsCommand();
      capabilities = await platformCapabilitiesCommand();
      activeApp = await activeApplicationCommand();
      organization = await organizationSnapshotCommand();
      if (historyCollectionId === null) {
        historyCollectionId =
          organization.collections.find((c) => c.kind === "system")?.id ?? null;
      }
      await refreshEntries();
      await refreshUnorganizedClearableCount();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  async function loadEntries(): Promise<EntryRecord[]> {
    if (selectedCollectionId === null) {
      return recentEntriesCommand({ limit: RAIL_LIMIT });
    }
    return recentEntriesFilteredCommand({
      limit: RAIL_LIMIT,
      collectionId: selectedCollectionId,
      tagIds: [],
    });
  }

  async function hydrateEntryOrganization(
    entries: EntryRecord[],
    options: { force?: boolean } = {},
  ): Promise<void> {
    // The hydration round is anchored on a monotonic token so a
    // stale response can never overwrite a fresher commit (e.g. a
    // collection switch that lands while a previous round is still
    // in flight). The pure helpers in `lib/entryOrganization` are
    // intentionally tolerant of a subset call — a caller passing
    // only one entry keeps the existing `loaded` state of every
    // other card, which is the contract `toggleFavorite` relies on.
    const token = ++entryOrganizationHydrationToken;
    const reconciled = reconcileEntryOrganizationToVisible(
      entryOrganization,
      entryOrganizationHydration,
      entries,
    );
    entryOrganization = reconciled.nextOrganization;
    entryOrganizationHydration = reconciled.nextHydration;

    const pendingEntries = selectPendingEntries(entries, entryOrganizationHydration, {
      force: options.force,
    });
    if (pendingEntries.length === 0) {
      return;
    }
    entryOrganizationHydration = markEntriesPending(
      entryOrganizationHydration,
      pendingEntries,
    );

    const results: EntryOrganizationFetchResult[] = await Promise.all(
      pendingEntries.map(async (entry): Promise<EntryOrganizationFetchResult> => {
        try {
          const [tagIds, collectionIds] = await Promise.all([
            entryTagsCommand({ entryId: entry.id }),
            entryCollectionsCommand({ entryId: entry.id }),
          ]);
          return {
            id: entry.id,
            ok: true,
            tagIds,
            collectionIds,
          };
        } catch (err) {
          return {
            id: entry.id,
            ok: false,
            error: err instanceof Error ? err.message : String(err),
          };
        }
      }),
    );

    if (token !== entryOrganizationHydrationToken) {
      return;
    }

    if (!organization) {
      const failed = new Map(entryOrganizationHydration);
      for (const entry of pendingEntries) {
        failed.set(entry.id, "error");
      }
      entryOrganizationHydration = failed;
      organizationError =
        "organization snapshot unavailable; tag associations cannot be hydrated";
      return;
    }

    const applied = applyEntryOrganizationResults(
      entryOrganization,
      entryOrganizationHydration,
      results,
      {
        resolveTags: (tagIds) =>
          organization!.tags.filter((t) => tagIds.includes(t.id)),
        resolveCollections: (collectionIds) =>
          organization!.collections.filter((c) => collectionIds.includes(c.id)),
      },
    );
    entryOrganization = applied.nextOrganization;
    entryOrganizationHydration = applied.nextHydration;
    for (const result of results) {
      if (!result.ok) {
        organizationError = `Failed to hydrate tags for entry #${result.id}: ${result.error}`;
      }
    }
  }

  async function toggleFavorite(entry: EntryRecord): Promise<void> {
    try {
      const response = await setFavoriteCommand({
        id: entry.id,
        pinned: !entry.is_pinned,
      });
      if (response.kind === "updated") {
        // Pin/unpin only flips `is_pinned` on the affected row. It
        // does NOT change the entry's tags or collections, so the
        // per-entry cache MUST stay untouched — calling
        // `hydrateEntryOrganization([updated])` here used to wipe
        // every other card's hydration state and surface them as
        // "No se pudieron cargar los tags de esta entrada." until
        // the user changed collection or restarted the app.
        // Updating the canonical lists through `applyPinUpdate`
        // keeps `entryOrganization` and `entryOrganizationHydration`
        // intact for every other entry.
        const updated: EntryRecord = {
          ...entry,
          is_pinned: response.entry.is_pinned,
        };
        const patched = applyPinUpdate(entries, visibleEntries, updated, {
          isFiltering,
        });
        entries = patched.nextEntries;
        visibleEntries = patched.nextVisibleEntries;
        await refreshUnorganizedClearableCount();
      } else {
        organizationError = `Entry #${entry.id} is no longer available.`;
      }
    } catch (err) {
      organizationError = err instanceof Error ? err.message : String(err);
    }
  }

  function requestDelete(entry: EntryRecord): void {
    pendingConfirmation = {
      kind: "delete",
      id: entry.id,
      label: entry.content.replace(/\s+/g, " ").trim().slice(0, 60) || "(empty)",
    };
  }

  async function requestClearHistory(): Promise<void> {
    await refreshUnorganizedClearableCount();
    const count = unorganizedClearableCount ?? 0;
    pendingConfirmation = { kind: "clear", count };
  }

  function cancelConfirmation(): void {
    pendingConfirmation = null;
  }

  async function runDelete(id: number): Promise<void> {
    const outcome: ManagementOutcome<unknown> = await applyDestructive(
      (action) => deleteEntryCommand({ id: action.id, confirm: action.confirm }),
      isDeleteConfirmationRequired,
      { id, confirm: true },
    );
    pendingConfirmation = null;
    if (outcome.kind === "confirmation_required") {
      organizationError = "The backend requested confirmation. Please retry.";
      return;
    }
    try {
      await refreshEntries();
    } catch (err) {
      organizationError = err instanceof Error ? err.message : String(err);
    }
  }

  async function runClearHistory(): Promise<void> {
    // Trash semantics: only entries that satisfy the documented
    // unorganized predicate are removed. Favorites, secondary
    // collection rows and entries in multiple collections stay.
    const outcome = await applyDestructive(
      (action) =>
        clearUnorganizedHistoryCommand({ confirm: action.confirm }),
      isClearConfirmationRequired,
      { confirm: true },
    );
    pendingConfirmation = null;
    if (outcome.kind === "confirmation_required") {
      organizationError = "The backend requested confirmation. Please retry.";
      return;
    }
    try {
      await refreshEntries();
      await refreshUnorganizedClearableCount();
    } catch (err) {
      organizationError = err instanceof Error ? err.message : String(err);
    }
  }

  async function refreshEntries(): Promise<void> {
    entries = await loadEntries();
    if (isFiltering) {
      // A reload while the search filter is active must re-run the
      // search so the rail keeps reflecting the latest captures; a
      // stale list could otherwise hide rows the user expects to see.
      await performSearch(searchQuery, { silent: true });
    } else {
      visibleEntries = entries;
      await hydrateEntryOrganization(entries);
    }
  }

  async function refreshUnorganizedClearableCount(): Promise<void> {
    if (unorganizedClearableCountLoading) {
      return;
    }
    unorganizedClearableCountLoading = true;
    try {
      const count = await unorganizedClearableCountCommand();
      unorganizedClearableCount = count;
      unorganizedClearableCountLoaded = true;
    } catch (err) {
      unorganizedClearableCount = null;
      unorganizedClearableCountLoaded = false;
      organizationError =
        err instanceof Error ? err.message : String(err);
    } finally {
      unorganizedClearableCountLoading = false;
    }
  }

  async function refreshOrganization(): Promise<void> {
    try {
      organization = await organizationSnapshotCommand();
    } catch (err) {
      organizationError =
        err instanceof Error ? err.message : String(err);
    }
  }

  async function refreshEntryOrganization(entryId: number): Promise<void> {
    if (!organization) {
      const stateNext = new Map(entryOrganizationHydration);
      stateNext.set(entryId, "error");
      entryOrganizationHydration = stateNext;
      return;
    }
    const stateNext = new Map(entryOrganizationHydration);
    stateNext.set(entryId, "pending");
    entryOrganizationHydration = stateNext;
    try {
      const [tagIds, collectionIds] = await Promise.all([
        entryTagsCommand({ entryId }),
        entryCollectionsCommand({ entryId }),
      ]);
      const tags = organization.tags.filter((t) => tagIds.includes(t.id));
      const collections = organization.collections.filter((c) =>
        collectionIds.includes(c.id),
      );
      const next = new Map(entryOrganization);
      next.set(entryId, { tags, collections });
      entryOrganization = next;
      const loadedNext = new Map(entryOrganizationHydration);
      loadedNext.set(entryId, "loaded");
      entryOrganizationHydration = loadedNext;
    } catch (err) {
      organizationError =
        err instanceof Error ? err.message : String(err);
      const errorNext = new Map(entryOrganizationHydration);
      errorNext.set(entryId, "error");
      entryOrganizationHydration = errorNext;
    }
  }

  async function selectCollectionFromSidebar(
    event: CustomEvent<{ collectionId: number | null }>,
  ): Promise<void> {
    selectedCollectionId = event.detail.collectionId;
    // Changing collection MUST re-execute the active search so the
    // rail keeps showing results scoped to the new selection. The
    // search already debounces the input; we pass `silent` so the
    // UI does not flash a `loading` state mid-navigation.
    await refreshEntries();
  }

  async function handleCreateCollection(
    event: CustomEvent<{ name: string }>,
  ): Promise<void> {
    try {
      await collectionsCreateCommand({ name: event.detail.name });
      await refreshOrganization();
    } catch (err) {
      organizationError =
        err instanceof Error ? err.message : String(err);
    }
  }

  async function handleRenameCollection(
    event: CustomEvent<{ collectionId: number; name: string }>,
  ): Promise<void> {
    try {
      await collectionsRenameCommand({
        collectionId: event.detail.collectionId,
        name: event.detail.name,
      });
      await refreshOrganization();
    } catch (err) {
      organizationError =
        err instanceof Error ? err.message : String(err);
    }
  }

  async function handleDeleteCollection(
    event: CustomEvent<{ collectionId: number }>,
  ): Promise<void> {
    try {
      await collectionsDeleteCommand({
        collectionId: event.detail.collectionId,
      });
      if (selectedCollectionId === event.detail.collectionId) {
        selectedCollectionId = null;
      }
      await refreshOrganization();
      await refreshEntries();
    } catch (err) {
      organizationError =
        err instanceof Error ? err.message : String(err);
    }
  }

  async function handleAssignTags(
    entry: EntryRecord,
    tagIds: number[],
  ): Promise<void> {
    await entryTagsSetCommand({
      entryId: entry.id,
      tagIds: [...tagIds],
    });
    await refreshOrganization();
    await refreshEntryOrganization(entry.id);
  }

  async function handleAssignCollections(
    entry: EntryRecord,
    collectionIds: number[],
  ): Promise<void> {
    await entryCollectionsSetCommand({
      entryId: entry.id,
      collectionIds,
    });
    await refreshOrganization();
    await refreshEntryOrganization(entry.id);
    // The set call may have moved the row in or out of the current
    // collection; the unorganized counter depends on the same set.
    await refreshUnorganizedClearableCount();
  }

  /**
   * Drop handler the sidebar dispatches when the user drags a card
   * onto a user collection row. The helper never carries clipboard
   * content, snippets, hashes or asset references — the card
   * forwards only the entry id through the ClipVault-private MIME
   * type — so the parent's only job is to combine the new
   * membership with the existing set without dropping `Historial`
   * or other collections, and to refuse the operation when the
   * hydration cache is still in flight (the safe-noop branch the
   * `combineMemberships` helper implements).
   */
  async function handleCardDrop(
    event: CustomEvent<{ entryId: number; collectionId: number }>,
  ): Promise<void> {
    const { entryId, collectionId } = event.detail;
    // Step 1: validate entry id and collection id. The sidebar
    // already refused non-user collections before dispatching
    // `card-drop`, but the parent is the single source of truth for
    // the drop pipeline so the checks stay here as well.
    if (!Number.isFinite(entryId) || !Number.isInteger(entryId)) {
      return;
    }
    if (
      !Number.isFinite(collectionId) || !Number.isInteger(collectionId)
    ) {
      return;
    }
    const entry = entries.find((candidate) => candidate.id === entryId);
    if (!entry) {
      // The card is not visible right now; the drop is a no-op.
      return;
    }
    const targetCollection = organization?.collections.find(
      (collection) => collection.id === collectionId,
    );
    if (!targetCollection || targetCollection.kind !== "user") {
      // Step 2: refuse system collections and any non-user target
      // that slipped through the sidebar guard. Historial is the
      // only system collection today; future kinds inherit the
      // protection automatically.
      return;
    }
    // Step 11: avoid duplicate writes while the same drop is in
    // flight. The first drop holds the (entryId, collectionId) pair
    // until the backend round-trip resolves; a second drop on the
    // same target that lands before the first finishes is dropped
    // silently — the backend is idempotent so the result would be
    // the same.
    const dropKey = `${entryId}:${collectionId}`;
    if (dropInFlight.has(dropKey)) {
      return;
    }
    const nextDropInFlight = new Set(dropInFlight);
    nextDropInFlight.add(dropKey);
    dropInFlight = nextDropInFlight;
    // Step 4: read the associations from the cache only when they
    // are really loaded. Otherwise the entry may not be in the map
    // yet (initial paint) or in `pending` / `error` state. The
    // fallback goes through `entryCollectionsCommand`, the same
    // metadata-only bridge the modal uses, and never returns
    // content.
    const hydrationState = entryOrganizationHydration.get(entryId);
    let currentCollectionIds: number[] | null = null;
    if (hydrationState === "loaded") {
      currentCollectionIds =
        entryOrganization.get(entryId)?.collections.map((c) => c.id) ?? null;
    } else {
      // The hydration round is still in flight, the cache is empty
      // (first paint) or the previous hydration failed. Refuse to
      // send a destructive `[]`: ask the backend for the current set
      // and merge on top of it.
      try {
        currentCollectionIds = await entryCollectionsCommand({
          entryId,
        });
      } catch (error) {
        organizationError =
          error instanceof Error ? error.message : String(error);
        const reduced = new Set(dropInFlight);
        reduced.delete(dropKey);
        dropInFlight = reduced;
        return;
      }
    }
    // Step 6 / Step 7: build the merged membership set. The helper
    // refuses `[]` and partial lists when hydration is missing,
    // refuses the system collection as a drop target, refuses
    // already-member targets, and reattaches the system collection
    // automatically so the card never leaves the protected
    // membership.
    const result = combineMemberships(
      entryId,
      collectionId,
      currentCollectionIds,
      historyCollectionId,
    );
    if (result.safeNoop || result.nextCollectionIds === undefined) {
      const reduced = new Set(dropInFlight);
      reduced.delete(dropKey);
      dropInFlight = reduced;
      return;
    }
    try {
      // Step 8 / Step 9: reuse `entry_collections_set` and await
      // its resolution. The command returns the persisted list so
      // the cache below can mirror the backend truth.
      await entryCollectionsSetCommand({
        entryId,
        collectionIds: result.nextCollectionIds,
      });
      // Step 10: refresh organization, the entry hydration and the
      // unorganized counter so the card is visible in the target
      // collection and the rail reflects the new membership
      // immediately.
      await refreshOrganization();
      await refreshEntryOrganization(entryId);
      await refreshUnorganizedClearableCount();
    } catch (error) {
      organizationError =
        error instanceof Error ? error.message : String(error);
    } finally {
      const reduced = new Set(dropInFlight);
      reduced.delete(dropKey);
      dropInFlight = reduced;
    }
  }

  async function handleRemoveFromCollection(
    entry: EntryRecord,
    collectionId: number,
  ): Promise<void> {
    await entryRemoveFromCollectionCommand({
      entryId: entry.id,
      collectionId,
    });
    await refreshEntries();
    await refreshEntryOrganization(entry.id);
    await refreshUnorganizedClearableCount();
  }

  async function refreshOrganizationForAllEntries(): Promise<void> {
    await refreshOrganization();
    await hydrateEntryOrganization(entries, { force: true });
    await refreshUnorganizedClearableCount();
  }

  function handleAfterMutation(updated: EntryRecord): Promise<void> {
    entries = entries.map((existing) =>
      existing.id === updated.id ? updated : existing,
    );
    if (isFiltering) {
      visibleEntries = visibleEntries.map((existing) =>
        existing.id === updated.id ? updated : existing,
      );
    } else {
      visibleEntries = entries;
    }
    return Promise.resolve();
  }

  function refreshCapabilitiesFromBackend(): Promise<Capabilities> {
    return refreshCapabilitiesCommand();
  }

  function closeGuidance(): void {
    guidance = null;
    retryNotice = null;
    retryError = null;
  }

  function handleRetryGuidance(): Promise<void> {
    retryNotice = null;
    retryError = null;
    return retryGuidance({
      refresh: refreshCapabilitiesFromBackend,
      onResolved: (caps) => {
        capabilities = caps;
        guidance = null;
        retryNotice = "Capability refreshed. Press Paste latest to continue.";
      },
      onStillUnavailable: (caps) => {
        capabilities = caps;
        retryNotice = "The platform still reports the capability as unavailable.";
      },
      onRefreshError: (message) => {
        retryError = `Could not refresh capabilities: ${message}`;
      },
    });
  }

  /**
   * Run (or re-run) the active search. Empty queries restore the
   * active-collection cards; non-empty queries produce the same
   * `EntryRecord[]` the rail renders for the default view. Stale
   * responses are dropped through a monotonic `searchToken` so a
   * collection change or a quick succession of keystrokes cannot
   * overwrite a fresher result.
   */

  /**
   * Single, deterministic desktop-level listener for `Cmd+F`/`Ctrl+F`.
   * The listener is registered exactly once during `onMount` and is
   * removed in `onDestroy` so the desktop cannot leak subscribers on
   * hot reload or a remount.
   *
   * The handler:
   *   - bypasses when a modal-with-input is open (the focus is held
   *     inside the modal so the webview keeps its default behaviour
   *     and the shortcut does not steal it);
   *   - prevents the native browser action so a Tauri webview
   *     never searches through the clipboard content by accident;
   *   - focuses the existing search input without dispatching a
   *     synthetic `input` event so the existing search query is
   *     preserved across shortcut presses.
   */
  function onSearchShortcutKeydown(event: KeyboardEvent): void {
    if (!matchesSearchShortcut(event, shortcutPlatform)) {
      return;
    }
    if (openModal !== null) {
      return;
    }
    const target = event.target as HTMLElement | null;
    const searchInput = document.querySelector<HTMLInputElement>(
      "[data-testid='search-input']",
    );
    // If the user is already typing somewhere that is NOT the search
    // input (the title editor, the rename input, the collection
    // creator or any contenteditable), the shortcut MUST NOT steal
    // focus. Typing in the search input itself is the expected case.
    if (
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      (target instanceof HTMLElement && target.isContentEditable)
    ) {
      if (!searchInput || target !== searchInput) {
        return;
      }
    }
    event.preventDefault();
    event.stopPropagation();
    if (searchInput) {
      searchInput.focus();
      searchInput.select();
    }
  }

  let detachSearchShortcut: (() => void) | null = null;
  let detachDragOverGuard: (() => void) | null = null;
  let detachPointerDragController: (() => void) | null = null;

  /**
   * WebKit may deliver `dragover` to the document before the
   * scrollable child has a usable DataTransfer.types collection.
   * Preventing the default in one document-capture listener keeps
   * the browser's drop protocol enabled for an internal card drag;
   * the actual target and entry id are still validated by the
   * delegated drop handlers before any mutation occurs.
   */
  function onInternalDragOver(event: DragEvent): void {
    if (!hasActiveDragSession()) return;
    event.preventDefault();
    if (event.dataTransfer) {
      try {
        event.dataTransfer.dropEffect = "copy";
      } catch {
        /* readonly in some WebKit builds; target handlers still run. */
      }
    }
  }

  let searchToken = 0;
  async function performSearch(
    query: string,
    options: { silent?: boolean } = {},
  ): Promise<void> {
    const trimmed = query.trim();
    if (trimmed.length === 0) {
      searchController?.cancel();
      searchController = null;
      searchError = null;
      searching = false;
      searchStatus = "idle";
      visibleEntries = entries;
      return;
    }
    if (!options.silent) {
      searching = true;
    }
    const token = ++searchToken;
    if (searchController) {
      searchController.cancel();
      searchController = null;
    }
    searchError = null;
    const controller = runSearch({
      query: trimmed,
      debounceMs: SEARCH_DEBOUNCE_MS,
      invoke: async (q) => {
        const response: SearchResponse = await searchEntriesCommand({
          query: q,
          limit: RAIL_LIMIT,
          collectionId:
            selectedCollectionId !== null && !activeCollectionIsHistory
              ? selectedCollectionId
              : null,
          tagIds: [],
        });
        return response;
      },
    });
    searchController = controller;
    try {
      const response = await controller.result;
      if (token !== searchToken) {
        // A newer search has been scheduled; the previous round is
        // intentionally discarded so the rail never flashes a stale
        // list when the user keeps typing.
        return;
      }
      const records = response.hits
        .map((hit) => hit.record)
        .filter((record) => record !== undefined);
      visibleEntries = records;
      searchStatus = records.length === 0 ? "no_matches" : "ok";
    } catch (err) {
      if (token !== searchToken) return;
      searchError = err instanceof Error ? err.message : String(err);
      searchStatus = "empty";
    } finally {
      if (token === searchToken) {
        searching = false;
        searchController = null;
      }
    }
  }

  function handleSearchInput(value: string): void {
    searchQuery = value;
    void performSearch(value);
  }

  // ---- Modal coordinator --------------------------------------------------

  function openModalWith(
    id: ModalId,
    trigger: HTMLElement | null,
  ): void {
    modalReturnFocus = trigger;
    openModal = id;
  }

  function closeModal(): void {
    openModal = null;
    modalReturnFocus = null;
  }

  function onOpenDevelopment(event: MouseEvent): void {
    openModalWith("development", event.currentTarget as HTMLElement | null);
  }

  function onOpenPrivacy(event: MouseEvent): void {
    openModalWith("privacy", event.currentTarget as HTMLElement | null);
  }

  function onOpenRetention(event: MouseEvent): void {
    openModalWith("retention", event.currentTarget as HTMLElement | null);
  }

  function onOpenShortcut(event: MouseEvent): void {
    openModalWith("quick_paste_shortcut", event.currentTarget as HTMLElement | null);
  }

  function onRequestClearHistory(event: MouseEvent): void {
    modalReturnFocus = event.currentTarget as HTMLElement | null;
    void requestClearHistory();
  }

  function onPasteFailed(event: CustomEvent<PasteResponse>): void {
    if (event.detail.kind !== "pasted") {
      guidance = event.detail.guidance ?? null;
    }
  }

  function onModalCapabilitiesChanged(
    event: CustomEvent<Capabilities>,
  ): void {
    capabilities = event.detail;
  }

  function onModalRefresh(): void {
    void refresh();
  }

  function onModalEntriesChanged(
    event: CustomEvent<EntryRecord[]>,
  ): void {
    entries = event.detail;
    void hydrateEntryOrganization(entries);
  }

  // ---- Idempotent registrars ---------------------------------------------

  const registerQuickSearch = createQuickSearchRegistrar(
    defaultQuickPasteBridge,
    (activationError) => {
      quickSearchError = `Quick paste could not open: ${
        activationError instanceof Error
          ? activationError.message
          : String(activationError)
      }`;
    },
  );
  let unlistenQuickSearch: (() => void) | null = null;

  const registerHistoryUpdated = createHistoryUpdatedRegistrar();
  let unlistenHistoryUpdated: (() => void) | null = null;

  const registerOrganizationUpdated = createOrganizationUpdatedRegistrar();
  let unlistenOrganizationUpdated: (() => void) | null = null;

  async function handleHistoryUpdated(): Promise<void> {
    try {
      await refreshEntries();
      await refreshUnorganizedClearableCount();
    } catch (err) {
      console.warn(
        "history-updated refresh failed:",
        err instanceof Error ? err.message : String(err),
      );
    }
  }

  async function handleOrganizationUpdated(): Promise<void> {
    try {
      await refreshOrganizationForAllEntries();
    } catch (err) {
      console.warn(
        "organization-updated refresh failed:",
        err instanceof Error ? err.message : String(err),
      );
    }
  }

  async function handleQuickSearchActivation(): Promise<void> {
    searchQuery = "";
    quickSearchError = null;
    searchError = null;
    searching = false;
    searchStatus = "idle";
    if (searchController) {
      searchController.cancel();
      searchController = null;
    }
    visibleEntries = entries;
  }

  onMount(() => {
    void refresh();
    registerQuickSearch(handleQuickSearchActivation)
      .then((unlisten) => {
        unlistenQuickSearch = unlisten;
        quickSearchListenerStatus = "ready";
      })
      .catch((error) => {
        console.error("failed to register quick-search listener", error);
        quickSearchListenerStatus = "error";
        quickSearchError = `Quick-search listener unavailable: ${error instanceof Error ? error.message : String(error)}`;
      });
    registerHistoryUpdated(handleHistoryUpdated)
      .then((unlisten) => {
        unlistenHistoryUpdated = unlisten;
      })
      .catch((error) => {
        console.error("failed to register history-updated listener", error);
      });
    registerOrganizationUpdated(handleOrganizationUpdated)
      .then((unlisten) => {
        unlistenOrganizationUpdated = unlisten;
      })
      .catch((error) => {
        console.error(
          "failed to register organization-updated listener",
          error,
        );
      });
    document.addEventListener("keydown", onSearchShortcutKeydown, true);
    detachSearchShortcut = () => {
      document.removeEventListener(
        "keydown",
        onSearchShortcutKeydown,
        true,
      );
    };
    document.addEventListener("dragover", onInternalDragOver, true);
    detachDragOverGuard = () => {
      document.removeEventListener("dragover", onInternalDragOver, true);
    };
    detachPointerDragController = installPointerDragController(document);
  });

  onDestroy(() => {
    if (unlistenQuickSearch) {
      unlistenQuickSearch();
      unlistenQuickSearch = null;
    }
    if (unlistenHistoryUpdated) {
      unlistenHistoryUpdated();
      unlistenHistoryUpdated = null;
    }
    if (unlistenOrganizationUpdated) {
      unlistenOrganizationUpdated();
      unlistenOrganizationUpdated = null;
    }
    if (detachSearchShortcut) {
      detachSearchShortcut();
      detachSearchShortcut = null;
    }
    if (detachDragOverGuard) {
      detachDragOverGuard();
      detachDragOverGuard = null;
    }
    if (detachPointerDragController) {
      detachPointerDragController();
      detachPointerDragController = null;
    }
    if (searchController) {
      searchController.cancel();
      searchController = null;
    }
  });
</script>

<main>
  {#if organizationError}
    <p class="status error" role="alert" data-testid="organization-error">
      {organizationError}
    </p>
  {/if}

  {#if loading}
    <p class="status">Conectando con el backend…</p>
  {:else if error}
    <p class="status error" role="alert">
      No se pudo conectar con el core de ClipVault: {error}
    </p>
    <button type="button" on:click={() => void refresh()}>Reintentar</button>
  {:else if diagnostics}
    <DesktopToolbar
      searchQuery={searchQuery}
      searching={searching}
      openModal={openModal}
      searchShortcut={searchShortcutLabelText}
      searchShortcutAccessible={searchShortcutAccessibleText}
      onSearchInput={handleSearchInput}
      onOpenDevelopment={onOpenDevelopment}
      onOpenPrivacy={onOpenPrivacy}
      onOpenRetention={onOpenRetention}
      onOpenShortcut={onOpenShortcut}
      onRequestClearHistory={onRequestClearHistory}
    />

    <div class="layout">
      <OrganizationSidebar
        collections={organization?.collections ?? []}
        activeCollectionId={selectedCollectionId}
        on:select={(e) => selectCollectionFromSidebar(e)}
        on:create={(e) => handleCreateCollection(e)}
        on:rename={(e) => handleRenameCollection(e)}
        on:delete={(e) => handleDeleteCollection(e)}
        on:card-drop={(e) => handleCardDrop(e)}
      />
      <div class="layout-main">
        <p
          class="search-status muted"
          data-testid="search-status"
          data-search-status={searchStatus}
          aria-live="polite"
        >
          {#if searchError}
            <span class="error" data-testid="search-status-error">
              Búsqueda fallida: {searchError}
            </span>
          {:else if isFiltering && searching}
            <span data-testid="search-status-loading">Buscando…</span>
          {:else if isFiltering && searchStatus === "no_matches"}
            <span data-testid="search-status-no-matches">
              Sin coincidencias para "{searchQuery}".
            </span>
          {:else if isFiltering}
            <span data-testid="search-status-results">
              {visibleEntries.length} resultado{visibleEntries.length === 1 ? "" : "s"} para "{searchQuery}".
            </span>
          {/if}
        </p>

        <HistoryCardRail
          entries={visibleEntries}
          allTags={organization?.tags ?? []}
          allCollections={organization?.collections ?? []}
          activeCollectionId={selectedCollectionId}
          entryOrganization={entryOrganization}
          entryOrganizationHydration={entryOrganizationHydration}
          isFiltering={isFiltering}
          onTogglePin={(entry) => toggleFavorite(entry)}
          onRequestDelete={requestDelete}
          onAfterMutation={(entry) => handleAfterMutation(entry)}
          onAssignTags={(entry, tagIds) => handleAssignTags(entry, tagIds)}
          onAssignCollections={(entry, collectionIds) =>
            handleAssignCollections(entry, collectionIds)}
          onRemoveFromCollection={(entry, collectionId) =>
            handleRemoveFromCollection(entry, collectionId)}
        />
      </div>
    </div>
  {/if}
</main>

{#if pendingConfirmation}
  {@const confirmation = pendingConfirmation}
  <div
    class="confirm-dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="confirm-title"
    data-testid="confirm-dialog"
  >
    <article>
      {#if confirmation.kind === "delete"}
        <h2 id="confirm-title">Eliminar esta entrada</h2>
        <p>
          <code>{confirmation.label}</code>
        </p>
        <p class="muted">
          Esto elimina la entrada del historial local. La acción no se puede deshacer.
        </p>
        <div class="row">
          <button
            type="button"
            class="danger"
            data-testid="confirm-delete"
            on:click={() => runDelete(confirmation.id)}
          >
            Eliminar entrada
          </button>
          <button
            type="button"
            data-testid="cancel-delete"
            on:click={cancelConfirmation}
          >
            Cancelar
          </button>
        </div>
      {:else}
        <h2 id="confirm-title">Limpiar historial sin colección</h2>
        <p data-testid="confirm-clear-summary">
          {confirmation.count} captura{confirmation.count === 1 ? "" : "s"} no favorita{confirmation.count === 1 ? "" : "s"} y sin colección secundaria se eliminarán.
        </p>
        <p class="muted">
          Los favoritos, las entradas agrupadas en una colección secundaria y las que están en varias colecciones se conservan.
        </p>
        <div class="row">
          <button
            type="button"
            class="danger"
            data-testid="confirm-clear"
            on:click={() => runClearHistory()}
            disabled={!unorganizedClearableCountLoaded}
          >
            Limpiar historial sin colección
          </button>
          <button
            type="button"
            data-testid="cancel-clear"
            on:click={cancelConfirmation}
          >
            Cancelar
          </button>
        </div>
      {/if}
    </article>
  </div>
{/if}

<Modal
  open={openModal === "development"}
  titleId="development-title"
  title="Development"
  returnFocusTo={modalReturnFocus}
  onClose={closeModal}
>
  <DevelopmentModal
    {diagnostics}
    {capabilities}
    {activeApp}
    {entries}
    on:refresh={onModalRefresh}
    on:capabilitiesChanged={(e) => onModalCapabilitiesChanged(e)}
    on:entriesChanged={(e) => onModalEntriesChanged(e)}
    on:pasteFailed={(e) => onPasteFailed(e)}
  />
</Modal>

<Modal
  open={openModal === "privacy"}
  titleId="privacy-title"
  title="Privacidad"
  returnFocusTo={modalReturnFocus}
  onClose={closeModal}
>
  <PrivacyModal />
</Modal>

<Modal
  open={openModal === "retention"}
  titleId="retention-title"
  title="Retención del historial"
  returnFocusTo={modalReturnFocus}
  onClose={closeModal}
>
  <RetentionModal />
</Modal>

<Modal
  open={openModal === "quick_paste_shortcut"}
  titleId="shortcut-title"
  title="Atajo de pegado rápido"
  returnFocusTo={modalReturnFocus}
  onClose={closeModal}
>
  <QuickPasteShortcutModal
    {capabilities}
    listenerStatus={quickSearchListenerStatus}
    listenerError={quickSearchError}
    platformOs={diagnostics?.platform_os ?? null}
  />
</Modal>

{#if guidance}
  <PlatformGuidanceModal
    {guidance}
    {retryNotice}
    {retryError}
    onClose={closeGuidance}
    onRetry={handleRetryGuidance}
  />
{/if}

<style>
  :global(:root) {
    --cv-font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
    --cv-body: 0.9rem;
    --cv-muted: 0.78rem;
    --cv-title: 1.5rem;
    --cv-title-lg: 1.75rem;
    --cv-title-md: 1rem;
    --cv-title-sm: 0.95rem;
    --cv-control: 0.85rem;
    --cv-tag: 0.65rem;
    --cv-preview: 0.72rem;
    --cv-radius-sm: 6px;
    --cv-radius-md: 10px;
    --cv-radius-lg: 12px;
    --cv-bg-surface: #0e1116;
    --cv-bg-elevated: #161b22;
    --cv-bg-hover: rgba(255, 255, 255, 0.06);
    --cv-border: #30363d;
    --cv-border-strong: #475569;
    --cv-fg: #f0f4f8;
    --cv-fg-muted: #94a3b8;
    --cv-fg-error: #f87171;
    --cv-fg-ok: #4ade80;
    --cv-accent: #2563eb;
    --cv-accent-hover: #1d4ed8;
    --cv-danger: #b91c1c;
    --cv-danger-hover: #991b1b;
    --cv-modal-overlay: rgba(8, 11, 16, 0.78);
    --cv-focus-ring: rgba(37, 99, 235, 0.45);
    --cv-card-size: 240px;
    --cv-card-rail-height: calc(var(--cv-card-size, 240px) + 2.75rem);
  }

  :global(html, body) {
    margin: 0;
    padding: 0;
    font-family: var(--cv-font-family, -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif);
    background: #0e1116;
    color: var(--cv-fg, #f0f4f8);
    font-size: var(--cv-body, 0.9rem);
  }

  main {
    /* The desktop must fill the available work area horizontally so
     * the rail of cards has room to scroll. The previous cap of
     * 880px forced the rail to wrap into a single column on wider
     * monitors and re-shaped the cards; the layout now stretches to
     * the monitor width and keeps the rail's overflow-x on the
     * `.history-card-rail` container instead of the body.
     *
     * The bottom padding stays small: the desktop reserves just
     * enough vertical breathing room for the search-status bar and
     * the listener-status line without leaving the empty band the
     * previous `2rem` produced once the redundant title line was
     * removed. */
    box-sizing: border-box;
    width: 100%;
    max-width: none;
    margin: 0;
    padding: 1rem 1.25rem 1rem;
    line-height: 1.5;
  }

  .status {
    font-style: italic;
  }

  .status.error {
    color: var(--cv-fg-error, #f87171);
  }

  .muted {
    color: var(--cv-fg-muted, #94a3b8);
  }

  .error {
    color: var(--cv-fg-error, #f87171);
  }

  .search-status {
    margin: 0;
    min-height: 1.4em;
    font-size: var(--cv-muted, 0.78rem);
    /* The status bar reserves a single line of vertical space so the
     * rail column stays bounded even when the message text is long.
     * The rail shares its `--cv-card-rail-height` token with the
     * collection sidebar and any drift in this column would lift the
     * grid row, leaving an empty band beneath the sidebar.
     */
    max-height: 1.4em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /*
   * Two-column grid: the sidebar takes a fixed slice and the rail
   * owns the remaining width. The `minmax(0, 1fr)` on the rail
   * column is the key piece of CSS that prevents the rail from
   * *forcing* the grid track to grow with its content (a default
   * `minmax(auto, 1fr)` would let the row's intrinsic minimum
   * expand to fit every card, breaking the horizontal scroll). With
   * `0` as the minimum the rail can shrink to its container width
   * and the cards stay at their fixed `--cv-card-size`.
   */
  .layout {
    display: grid;
    grid-template-columns: minmax(180px, 220px) minmax(0, 1fr);
    gap: 1rem;
    align-items: flex-start;
    width: 100%;
    min-width: 0;
  }

  .layout-main {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    min-width: 0;
    width: 100%;
  }

  /*
   * The pointer drag fallback creates this node outside Svelte's component
   * tree. Keep it deliberately generic and non-interactive: it confirms the
   * drag gesture without copying clipboard text, titles, source metadata or
   * image pixels into a second DOM surface.
   */
  :global(.cv-pointer-drag-ghost) {
    position: fixed;
    top: 0;
    left: 0;
    z-index: 1000;
    width: 180px;
    height: 120px;
    box-sizing: border-box;
    padding: 0.65rem;
    border: 1px solid rgba(147, 197, 253, 0.68);
    border-radius: 12px;
    background: rgba(22, 27, 34, 0.94);
    color: #f0f4f8;
    box-shadow: 0 12px 28px rgba(0, 0, 0, 0.34);
    opacity: 0.94;
    pointer-events: none;
    user-select: none;
    -webkit-user-select: none;
    overflow: hidden;
    transform: translate3d(14px, 14px, 0);
    will-change: transform;
  }

  :global(.cv-pointer-dragging),
  :global(.cv-pointer-dragging *) {
    user-select: none;
    -webkit-user-select: none;
  }

  :global(.cv-pointer-drag-ghost-header) {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    color: #93c5fd;
    font-size: 0.78rem;
    font-weight: 700;
    letter-spacing: 0.02em;
  }

  :global(.cv-pointer-drag-ghost-marker) {
    width: 0.62rem;
    height: 0.62rem;
    flex: 0 0 auto;
    border-radius: 50%;
    background: #60a5fa;
    box-shadow: 0 0 0 3px rgba(96, 165, 250, 0.16);
  }

  :global(.cv-pointer-drag-ghost-body) {
    margin-top: 1.1rem;
    color: #f0f4f8;
    font-size: 0.95rem;
    font-weight: 600;
  }

  :global(.cv-pointer-drag-ghost-footer) {
    margin-top: 0.75rem;
    color: #9aa7b8;
    font-size: 0.68rem;
    white-space: nowrap;
  }

  @media (max-width: 720px) {
    .layout {
      grid-template-columns: 1fr;
    }
  }

  .confirm-dialog {
    position: fixed;
    inset: 0;
    background: var(--cv-modal-overlay, rgba(8, 11, 16, 0.78));
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 70;
  }

  .confirm-dialog article {
    background: var(--cv-bg-elevated, #161b22);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 10px);
    padding: 1.25rem 1.5rem;
    max-width: 420px;
    width: 90%;
  }

  .confirm-dialog h2 {
    margin: 0 0 0.5rem;
    font-size: 1rem;
  }

  .row {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    margin-bottom: 0.5rem;
    flex-wrap: wrap;
  }

  button {
    background: var(--cv-accent, #2563eb);
    color: white;
    border: 0;
    padding: 0.45rem 0.85rem;
    border-radius: var(--cv-radius-sm, 6px);
    cursor: pointer;
    font-size: var(--cv-control, 0.85rem);
  }

  button.danger {
    background: var(--cv-danger, #b91c1c);
  }

  button.danger:hover:not(:disabled) {
    background: var(--cv-danger-hover, #991b1b);
  }

  button:disabled {
    background: #374151;
    color: var(--cv-fg-muted, #94a3b8);
    cursor: not-allowed;
  }

  button:hover:not(:disabled) {
    background: var(--cv-accent-hover, #1d4ed8);
  }

  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
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
