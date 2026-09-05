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
   * Empty-state and skeleton are rendered inside the rail so the
   * parent component stays free of layout markup.
   */
  import { onDestroy, onMount } from "svelte";
  import type { Collection, EntryRecord, Tag } from "./types";
  import HistoryCard from "./HistoryCard.svelte";

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

  /** Active card id (the only card whose menu is currently open). */
  let openCardId: number | null = null;

  function handleMenuToggle(event: CustomEvent<{ id: number; open: boolean }>) {
    const { id, open } = event.detail;
    openCardId = open ? id : null;
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
    if (openCardId == null) return;
    if (event.key === "Escape") {
      event.preventDefault();
      closeAllMenus();
    }
  }

  let detachWindow: (() => void) | null = null;

  onMount(() => {
    document.addEventListener("click", onWindowClick, true);
    document.addEventListener("keydown", onWindowKeydown, true);
    detachWindow = () => {
      document.removeEventListener("click", onWindowClick, true);
      document.removeEventListener("keydown", onWindowKeydown, true);
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
        on:menu-toggle={(e) => handleMenuToggle(e)}
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
