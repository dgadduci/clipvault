<script lang="ts">
  /**
   * Horizontal rail of recent capture cards. The component is the
   * single visual surface the recent-entries list owns; the rest of
   * `App.svelte` only supplies the data and the mutation callbacks.
   *
   * The rail enforces a single-open menu invariant: opening one
   * card's ellipsis menu automatically closes any other card's
   * menu, and the rail listens for outside clicks + Escape to close
   * the active menu without leaking event handlers between mounts.
   *
   * The rail also owns the local selection state the
   * `preview-interaction-regressions` change introduces: a single
   * `selectedEntryId` is forwarded to every card so the
   * `aria-selected` flag, the platform-aware preview-shortcut hint
   * and the `Cmd/Ctrl+Enter` keyboard matcher all see one source of
   * truth. The selection is purely visual — it is never persisted,
   * sent to the backend or copied into a payload.
   *
   * Empty-state and skeleton are rendered inside the rail so the
   * parent component stays free of layout markup.
   */
  import { onDestroy, onMount } from "svelte";
  import type { Collection, EntryRecord, Tag } from "./types";
  import HistoryCard from "./HistoryCard.svelte";
  import {
    horizontalRailNextSelectionId,
    type HorizontalRailDirection,
  } from "./lib/horizontalRailNavigation.ts";

  export let entries: EntryRecord[] = [];
  export let allTags: Tag[] = [];
  export let allCollections: Collection[] = [];
  export let activeCollectionId: number | null = null;
  /**
   * Whether the rail currently renders a search-filtered subset. The
   * flag drives the empty-state copy so a search that yields no
   * matches reads "Sin coincidencias" instead of the default
   * "Aún no has capturado nada" message.
   */
  export let isFiltering: boolean = false;
  export let entryOrganization: Map<
    number,
    { tags: Tag[]; collections: Collection[] }
  > = new Map();
  /**
   * Per-entry hydration state. The rail forwards the value to every
   * card so the **Editar tags** modal can gate **Guardar** on it. A
   * card with state `"pending"` must not let the user write a stale
   * `[]` selection against an entry whose associations are still in
   * flight.
   */
  export let entryOrganizationHydration: Map<number, "pending" | "loaded" | "error"> =
    new Map();
  export let onTogglePin: (entry: EntryRecord) => void = () => {};
  export let onRequestDelete: (entry: EntryRecord) => void = () => {};
  export let onAfterMutation: (entry: EntryRecord) => void = () => {};
  /**
   * Mutation callbacks return a `Promise<void>` so the HistoryCard
   * can await the persistence round-trip before closing the modal.
   * The rail never observes the rejection itself; the card owns the
   * error surface (`org-error` chip) so the failure stays scoped to
   * the card the user just acted on.
   */
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
   * Optional forwarder the rail re-emits when a card dispatches
   * `preview-request`. The Desktop coordinates a single preview
   * overlay from `App.svelte`; the rail only forwards the request
   * so the card never has to know whether it is mounted in a
   * preview-enabled rail or not.
   */
  export let onRequestPreview: (entry: EntryRecord) => void = () => {};

  /** Active card id (the only card whose menu is currently open). */
  let openCardId: number | null = null;
  /**
   * Local selection state. `null` means no card is selected; any
   * other value is the id of the entry the user picked by clicking
   * the non-interactive surface of a card. The state lives on the
   * rail because two cards racing a `mousedown` event must never
   * end up with two selected ids.
   *
   * The state is exposed to the parent through
   * `bind:selectedEntryId` so `App.svelte` can drive the
   * desktop-level `Cmd/Ctrl+Enter` keyboard matcher without going
   * through a global selector. The rail is the single source of
   * truth: the parent only reads the value to dispatch the
   * keyboard shortcut.
   */
  export let selectedEntryId: number | null = null;
  /**
   * Reference to the rendered card DOM keyed by entry id. The
   * `horizontalRailNextSelectionId` helper produces the next id and
   * the rail routes the corresponding element through
   * `scrollIntoView({ inline: "nearest" })` so a card that just
   * became active becomes visible inside the rail without moving
   * the desktop scroll surface. The map mirrors the keyed `{#each}`
   * the template already iterates so the helper never needs a
   * `querySelector` round-trip. The callback the rail hands to
   * each `HistoryCard` (the `onCardRef` prop) inserts / removes
   * entries as cards mount / unmount.
   */
  const cardEls: Map<number, HTMLElement> = new Map();
  /**
   * Track which entry ids the rail has ever rendered so the
   * `onCardRef` callback can clean up stale entries from the map
   * when a card unmounts because its entry left the visible scope
   * (delete, search, collection switch).
   */
  const lastKnownIds: Set<number> = new Set();
  /**
   * Drop registry entries whose entry id no longer maps to a
   * rendered card. The reactive dep on `entries` makes the cleanup
   * fire whenever the visible scope shrinks so an `ArrowLeft`
   * press after a delete never tries to scrollIntoView an unmounted
   * element.
   */
  $: {
    const currentIds = new Set(entries.map((entry) => entry.id));
    for (const id of lastKnownIds) {
      if (!currentIds.has(id)) {
        cardEls.delete(id);
      }
    }
    for (const id of currentIds) {
      lastKnownIds.add(id);
    }
  }

  /**
   * Insert / remove an entry from the card registry. The callback
   * runs once per mount (with the article element) and once per
   * unmount (with `null`); the rail keeps the map in lock-step
   * with the DOM so a `scrollIntoView` round-trip always lands on
   * a real, mounted element.
   */
  function registerCardRef(id: number, el: HTMLElement | null): void {
    if (el === null) {
      cardEls.delete(id);
      return;
    }
    cardEls.set(id, el);
  }

  $: visibleEntryIds = new Set(entries.map((entry) => entry.id));
  $: if (
    selectedEntryId !== null &&
    !visibleEntryIds.has(selectedEntryId)
  ) {
    // The selected entry is no longer in the visible scope (it was
    // deleted, filtered out, moved to another collection, or the
    // rail was rebuilt). The state is dropped silently so a stale
    // selection cannot survive a refresh / search / collection
    // change — the spec forbids persisting or restoring it.
    selectedEntryId = null;
  }

  function handleMenuToggle(event: CustomEvent<{ id: number; open: boolean }>) {
    const { id, open } = event.detail;
    openCardId = open ? id : null;
  }

  function handlePreviewRequest(
    event: CustomEvent<{ id: number }>,
  ): void {
    const entryId = event.detail.id;
    const entry = entries.find((candidate) => candidate.id === entryId);
    if (!entry) {
      // The entry is no longer in the visible scope (it may have
      // been deleted, filtered out, or moved to a different
      // collection); the rail silently drops the request so the
      // preview never opens against a stale id.
      return;
    }
    onRequestPreview(entry);
  }

  function handleSelectRequest(
    event: CustomEvent<{ id: number | null }>,
  ): void {
    selectedEntryId = event.detail.id;
  }

  function closeAllMenus(): void {
    openCardId = null;
  }

  function onWindowClick(event: MouseEvent): void {
    if (openCardId == null) return;
    const target = event.target as HTMLElement | null;
    if (target && target.closest("[data-testid='history-card-menu']")) {
      return;
    }
    if (target && target.closest("[data-testid='history-card-menu-trigger']")) {
      return;
    }
    closeAllMenus();
  }

  function onWindowKeydown(event: KeyboardEvent): void {
    // Escape clears the menu first (the rail's documented single-
    // menu invariant), then the selection. The order matches the
    // visual stacking the user expects: open menu → open selection
    // → no menu, no selection. Both paths call `preventDefault` only
    // when they actually consume the event so a typing surface
    // keeps its default Escape behaviour.
    if (openCardId !== null && event.key === "Escape") {
      const target = event.target as HTMLElement | null;
      if (
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        (target instanceof HTMLElement && target.isContentEditable)
      ) {
        return;
      }
      event.preventDefault();
      closeAllMenus();
      return;
    }
    if (selectedEntryId !== null && event.key === "Escape") {
      const target = event.target as HTMLElement | null;
      if (
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        (target instanceof HTMLElement && target.isContentEditable)
      ) {
        return;
      }
      event.preventDefault();
      selectedEntryId = null;
    }
  }

  /**
   * Horizontal keyboard navigation the rail exposes through
   * `ArrowLeft` / `ArrowRight`. The handler delegates the
   * selection-id math to the pure
   * `horizontalRailNextSelectionId` helper, mirrors the result on
   * the canonical `selectedEntryId` state, and asks the freshly
   * selected card to call `scrollIntoView({ inline: "nearest" })`
   * so it lands inside the visible portion of the rail.
   *
   * The previous baseline never intercepted the horizontal arrow
   * keys so pressing `ArrowRight` only triggered the native
   * overflow scroll the rail already owns; the selection id
   * stayed pinned to whatever row the user had clicked, and the
   * newly-visible card never lit up as active. This handler is the
   * single switch that drives both effects together.
   *
   * The matcher short-circuits against typing surfaces
   * (`input`, `textarea`, `contenteditable`) so the search field,
   * the title editor and the rename modal keep their default caret
   * movement. A rail with zero entries returns early so an empty
   * press never claims a `selectedEntryId`.
   *
   * `preventDefault()` runs only on the branches the rail actually
   * handles so a typing surface the user keeps pressing arrows
   * against still moves its caret / native scroll.
   */
  function onRailHorizontalKeydown(event: KeyboardEvent): void {
    const direction = mapHorizontalArrow(event.key);
    if (direction === null) return;
    const target = event.target as HTMLElement | null;
    if (
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      (target instanceof HTMLElement && target.isContentEditable)
    ) {
      // Keep the default caret movement on typing surfaces; the
      // helper would otherwise steal keystrokes the user is
      // sending to the search input.
      return;
    }
    if (entries.length === 0) return;
    const visibleIds = entries.map((entry) => entry.id);
    const navigation = horizontalRailNextSelectionId(
      visibleIds,
      selectedEntryId,
      direction,
    );
    if (navigation.nextId === null) {
      // Empty rail: nothing to navigate to. The handler returns
      // without claiming the event so the rail does not silently
      // consume keystrokes when the user is interacting with an
      // empty-state copy.
      return;
    }
    event.preventDefault();
    if (!navigation.moved) {
      // The user pressed the arrow against a row that already sits
      // at the boundary. The rail still wants to scroll the
      // existing card so the visual edge bumps, but the selection
      // id does not need to be re-written — Svelte would otherwise
      // drop the assignment and clear the highlight.
      scrollSelectedCardIntoView(navigation.nextId);
      return;
    }
    selectedEntryId = navigation.nextId;
    scrollSelectedCardIntoView(navigation.nextId);
  }

  /**
   * Translate the raw `KeyboardEvent.key` value into the typed
   * direction the pure helper consumes. Returning `null` for any
   * other key lets the caller short-circuit without a second
   * guard.
   */
  function mapHorizontalArrow(
    key: string,
  ): HorizontalRailDirection | null {
    if (key === "ArrowRight") return "right";
    if (key === "ArrowLeft") return "left";
    return null;
  }

  /**
   * Make sure the active card sits inside the rail's visible
   * portion. The helper only forwards to the DOM when the rail
   * already mounted the element (a `bind:this` registry maps
   * entry id to the element the template rendered); entries that
   * the rail has not yet mounted are silently skipped so a
   * transition between scopes never scrolls the wrong card.
   */
  function scrollSelectedCardIntoView(id: number): void {
    const card = cardEls.get(id);
    if (!card) return;
    if (typeof card.scrollIntoView !== "function") return;
    card.scrollIntoView({ block: "nearest", inline: "nearest" });
  }

  let detachWindow: (() => void) | null = null;

  onMount(() => {
    document.addEventListener("click", onWindowClick, true);
    document.addEventListener("keydown", onWindowKeydown, true);
    document.addEventListener("keydown", onRailHorizontalKeydown, true);
    detachWindow = () => {
      document.removeEventListener("click", onWindowClick, true);
      document.removeEventListener("keydown", onWindowKeydown, true);
      document.removeEventListener(
        "keydown",
        onRailHorizontalKeydown,
        true,
      );
    };
  });

  onDestroy(() => {
    detachWindow?.();
    detachWindow = null;
  });

  /**
   * Reactive lookup helpers. The helpers expose the
   * `entryOrganization` map to Svelte's compiler via the closure
   * capture in the IIFE so the template below re-renders whenever
   * `entryOrganization` is reassigned — the previous helpers were
   * called as bare function references, so the compiler could not
   * observe the dependency on the Map and the rail never re-rendered
   * after a tag mutation until another incidental event refreshed
   * it. Reassigning `entryOrganization` is the canonical signal the
   * parent (`App.svelte`) emits through `refreshEntryOrganization`;
   * these reactive lookups make the HistoryCard pick the new
   * assignment up immediately without waiting for the next
   * `clipvault://history-updated` cycle.
   *
   * `lookupHydration` follows the same pattern so the per-entry
   * hydration state ("pending" / "loaded" / "error") reaches the
   * card without the card reaching back into the Map. The default
   * is `"error"` so a card whose row was never part of the latest
   * hydration round surfaces its blocker instead of mistaking the
   * missing key for an empty tag set.
   */
  $: lookupTags = (id: number): Tag[] => entryOrganization.get(id)?.tags ?? [];
  $: lookupCollections = (id: number): Collection[] =>
    entryOrganization.get(id)?.collections ?? [];
  $: lookupHydration = (
    id: number,
  ): "pending" | "loaded" | "error" =>
    entryOrganizationHydration.get(id) ?? "error";
</script>

{#if entries.length === 0}
  <p class="empty" data-testid="history-rail-empty">
    {#if isFiltering}
      Sin coincidencias en esta colección.
    {:else if activeCollectionId !== null}
      No hay capturas en esta colección con los filtros actuales.
    {:else}
      Aún no has capturado nada. Copia texto y aparecerá aquí.
    {/if}
  </p>
{:else}
  <div
    class="rail"
    role="list"
    aria-label="Historial reciente"
    data-testid="history-rail"
  >
    {#each entries as entry (entry.id)}
      <HistoryCard
        {entry}
        assignedTags={lookupTags(entry.id)}
        assignedCollections={lookupCollections(entry.id)}
        entryOrganizationLoaded={lookupHydration(entry.id) === "loaded"}
        entryOrganizationState={lookupHydration(entry.id)}
        {allTags}
        {allCollections}
        {activeCollectionId}
        onAssignTags={onAssignTags}
        onAssignCollections={onAssignCollections}
        onRemoveFromCollection={onRemoveFromCollection}
        {onTogglePin}
        {onRequestDelete}
        {onAfterMutation}
        selected={selectedEntryId === entry.id}
        onSelect={(id: number | null) => {
          selectedEntryId = id;
        }}
        menuOpen={openCardId === entry.id}
        onCardRef={(el) => registerCardRef(entry.id, el)}
        on:menu-toggle={(e) => handleMenuToggle(e)}
        on:preview-request={(e) => handlePreviewRequest(e)}
        on:select-request={(e) => handleSelectRequest(e)}
      />
    {/each}
  </div>
{/if}

<style>
  /*
   * The rail owns the only horizontal scrollbar the desktop exposes
   * for the cards. The previous layout let the cards push the desktop
   * itself to the right when the user had many captures because the
   * flex row's intrinsic minimum was wider than the panel. The
   * fix is structural:
   *
   *   - `min-width: 0` is required for a flex child to shrink below
   *     its intrinsic content size. Without it, a wide rail could
   *     still grow the parent grid track and defeat the overflow
   *     boundary.
   *   - `width: 100%` keeps the rail pinned to its column.
   *   - `max-width: 100%` is a defensive cap for the rare case where
   *     a parent forgets to constrain the rail.
   *   - `overflow-x: auto` activates the horizontal scrollbar the
   *     spec demands while `overflow-y: hidden` keeps the bar
   *     vertically stable so the cards never introduce a phantom
   *     vertical scroll.
   *   - `box-sizing: border-box` matches the rest of the desktop
   *     and lets the padding play well with the scrollbar.
   *
   * The rail mirrors `--cv-card-rail-height` declared on the root
   * so the sidebar can read the same value and stay aligned with the
   * cards the rail carries.
   */
  .rail {
    --cv-card-size: 240px;
    box-sizing: border-box;
    display: flex;
    flex-direction: row;
    gap: 0.75rem;
    width: 100%;
    min-width: 0;
    max-width: 100%;
    height: var(
      --cv-card-rail-height,
      calc(var(--cv-card-size, 240px) + 2.75rem)
    );
    overflow-x: auto;
    overflow-y: hidden;
    padding: 0.25rem 0.25rem 0.75rem;
    scroll-snap-type: x proximity;
    -webkit-overflow-scrolling: touch;
  }

  /*
   * The card size is fixed through `--cv-card-size`. The `flex: 0 0`
   * triple is the contract that keeps a card from being squeezed by
   * the flex row (`flex-grow: 0`, `flex-shrink: 0`) while still
   * respecting the explicit width. Without the `0 0` a card would
   * shrink to fit the row and the squares would become rectangles.
   */
  .rail :global(.card) {
    flex: 0 0 var(--cv-card-size);
    width: var(--cv-card-size);
    scroll-snap-align: start;
  }

  .empty {
    margin: 0;
    padding: 1rem 1.25rem;
    background: #161b22;
    border: 1px dashed #30363d;
    border-radius: 10px;
    color: #94a3b8;
    font-style: italic;
  }
</style>
