<script lang="ts">
  /**
   * Read-only membership modal for a single history card.
   *
   * The modal is the destination of the overflow icon the
   * `clipboard-history-cards` chip row installs when the chips that
   * fit in the card do not cover every assigned user collection.
   * It reuses the shared `Modal` shell so the focus trap, Escape
   * handling, backdrop dismissal, initial focus and return-focus
   * match the rest of the desktop.
   *
   * The component is intentionally read-only:
   *
   *   - it never persists a mutation through any Tauri command;
   *   - it never carries clipboard content, hashes, snippets, asset
   *     references, paths or drag payload bytes in its props or
   *     event payloads;
   *   - it exposes the full membership set, including the protected
   *     system `Historial` collection, so the user can confirm what
   *     the card actually belongs to without leaving the desktop;
   *   - it lists every collection the rail already hydrated for the
   *     card through `assignedCollections` — the parent never opens
   *     a second source of truth, never installs global listeners
   *     per card and never subscribes to the organisation update
   *     event from inside this component.
   *
   * Accessibility surface:
   *
   *   - `role="dialog"` + `aria-modal="true"` are owned by the shared
   *     `Modal` shell so screen readers announce the dialog
   *     consistently with the rest of the desktop.
   *   - `aria-labelledby` is wired to a stable title id that
   *     includes the entry id so two open modals can never share the
   *     same label target.
   *   - Initial focus moves to the close button; closing the modal
   *     returns focus to the overflow icon the parent passes through
   *     `returnFocusTo`.
   *   - Escape, the backdrop and the close button are all
   *     indistinguishable as far as the user is concerned: every
   *     path closes the dialog and restores focus.
   */
  import type { Collection } from "./types";
  import { collectionColor } from "./lib/collectionColor";
  import Modal from "./Modal.svelte";

  export let open: boolean;
  export let entryId: number;
  export let displayTitle: string;
  export let assignedCollections: Collection[] = [];
  /** Element the modal returns focus to when it closes. */
  export let returnFocusTo: HTMLElement | null = null;

  const titleId = `collection-membership-modal-title-${entryId}`;

  /**
   * `Historial` membership must be visible alongside the user
   * collections the card belongs to. The collection set the rail
   * already hydrated carries the system row, so the modal simply
   * renders the same list — no extra bridge call, no extra snapshot.
   * Sorting keeps the system row first (matching the sidebar order)
   * and orders the user rows case-insensitively by name so the
   * dialog reads deterministically.
   */
  $: orderedCollections = (() => {
    const system = assignedCollections.filter((c) => c.kind === "system");
    const users = [...assignedCollections.filter((c) => c.kind === "user")].sort(
      (a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()),
    );
    return [...system, ...users];
  })();

  function closeModal(): void {
    open = false;
  }
</script>

<Modal
  {open}
  {titleId}
  title={`Colecciones de la captura`}
  busy={false}
  {returnFocusTo}
  onClose={closeModal}
>
  <div
    class="collection-membership-modal"
    data-testid="collection-membership-modal"
    data-entry-id={entryId}
    aria-describedby={`${titleId}-summary`}
  >
    <p
      class="collection-membership-modal-summary"
      id={`${titleId}-summary`}
      data-testid="collection-membership-modal-summary"
    >
      {assignedCollections.length > 0
        ? `Esta captura pertenece a ${assignedCollections.length} ${assignedCollections.length === 1 ? "colección" : "colecciones"}.`
        : "Esta captura no está incluida en ninguna colección adicional."}
    </p>

    {#if orderedCollections.length > 0}
      <ul
        class="collection-membership-list"
        data-testid="collection-membership-list"
        aria-label={`Colecciones de ${displayTitle}`}
      >
        {#each orderedCollections as collection (collection.id)}
          {@const colour = collectionColor(collection.color_hex)}
          <li
            class="collection-membership-chip"
            data-testid="collection-membership-chip"
            data-collection-id={collection.id}
            data-collection-kind={collection.kind}
            data-color={collection.color_hex}
            style="--chip-color: {colour};"
          >
            <span
              class="collection-membership-chip-swatch"
              aria-hidden="true"
              style="background-color: {colour};"
            ></span>
            <span class="collection-membership-chip-name">{collection.name}</span>
            {#if collection.kind === "system"}
              <span
                class="collection-membership-chip-badge"
                aria-label="Colección de sistema protegida"
              >
                sistema
              </span>
            {/if}
          </li>
        {/each}
      </ul>
    {:else}
      <p
        class="collection-membership-empty"
        data-testid="collection-membership-empty"
      >
        No hay colecciones asignadas a esta captura.
      </p>
    {/if}
  </div>
</Modal>

<style>
  :global(.collection-membership-modal) {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    min-width: 320px;
    max-height: min(60vh, 28rem);
  }
  :global(.collection-membership-modal-summary) {
    margin: 0;
    font-size: var(--cv-body, 0.9rem);
    line-height: 1.4;
    color: var(--cv-fg-muted, #94a3b8);
  }
  :global(.collection-membership-list) {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    overflow-y: auto;
    /* Internal scroll: long lists never grow the dialog beyond the
     * viewport clamp the `Modal` shell enforces through `max-height`. */
    max-height: min(50vh, 22rem);
    align-content: flex-start;
  }
  :global(.collection-membership-chip) {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(148, 163, 184, 0.35);
    color: var(--chip-color, #94a3b8);
    border-radius: 999px;
    padding: 0.1rem 0.5rem;
    font-size: var(--cv-tag, 0.65rem);
    line-height: 1.1;
    font-weight: 600;
    letter-spacing: 0.02em;
    max-width: 18rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  :global(.collection-membership-chip-swatch) {
    display: inline-block;
    width: 0.55rem;
    height: 0.55rem;
    border-radius: 50%;
    border: 1px solid rgba(0, 0, 0, 0.35);
    flex: 0 0 auto;
  }
  :global(.collection-membership-chip-name) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  :global(.collection-membership-chip-badge) {
    flex: 0 0 auto;
    background: #1f2937;
    color: #93c5fd;
    border-radius: 999px;
    font-size: 0.6rem;
    padding: 0.05rem 0.35rem;
    border: 1px solid #30363d;
    text-transform: lowercase;
  }
  :global(.collection-membership-empty) {
    margin: 0;
    padding: 0.5rem 0.65rem;
    border-radius: var(--cv-radius-sm, 6px);
    background: rgba(148, 163, 184, 0.08);
    border: 1px solid rgba(148, 163, 184, 0.25);
    color: var(--cv-fg-muted, #94a3b8);
    font-size: var(--cv-meta, 0.7rem);
    line-height: 1.3;
  }
</style>