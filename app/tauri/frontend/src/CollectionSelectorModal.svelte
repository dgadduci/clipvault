<script lang="ts">
  /**
   * Multi-select collection picker used by the card menu's **Agregar
   * a colección** action. Mirrors the tag selector shape: a search
   * field, a checkbox-per-row list, **Guardar** / **Cancelar**
   * actions and an `Escape`-aware keyboard flow.
   *
   * The system `Historial` collection is filtered out of the
   * candidate list: it is always associated with every entry, so
   * the user cannot select it. The list is sorted case-insensitive
   * by name so the picker is deterministic.
   */
  import { createEventDispatcher, tick } from "svelte";
  import type { Collection } from "./types";

  export let open: boolean;
  export let candidateCollections: Collection[] = [];
  export let initialSelection: number[] = [];
  /**
   * `true` once the parent has loaded the entry's persisted
   * collection set from SQLite. The modal blocks **Guardar** while
   * this is `false` so the user cannot dispatch a stale `[]`
   * selection against an entry whose associations are still being
   * hydrated.
   */
  export let loaded: boolean = true;

  type DispatchEvents = {
    save: { collectionIds: number[] };
    cancel: void;
  };
  const dispatch = createEventDispatcher<DispatchEvents>();

  let search = "";
  let selection: Set<number> = new Set(initialSelection);
  let searchInputEl: HTMLInputElement | null = null;

  $: visibleCollections = (() => {
    // Hide system collections (Historial) — they cannot be selected.
    const candidates = candidateCollections.filter((c) => c.kind === "user");
    const term = search.trim().toLowerCase();
    const filtered = term
      ? candidates.filter((c) => c.name.toLowerCase().includes(term))
      : candidates;
    return [...filtered].sort((a, b) =>
      a.name.toLowerCase().localeCompare(b.name.toLowerCase()),
    );
  })();

  function toggle(collectionId: number): void {
    const next = new Set(selection);
    if (next.has(collectionId)) {
      next.delete(collectionId);
    } else {
      next.add(collectionId);
    }
    selection = next;
  }

  function isSelected(collectionId: number): boolean {
    return selection.has(collectionId);
  }

  function reset(initial: number[]): void {
    selection = new Set(initial);
    search = "";
  }

  $: if (open) {
    reset(initialSelection);
  }

  function focusSearch(): void {
    void tick().then(() => {
      searchInputEl?.focus();
    });
  }

  $: if (open) {
    focusSearch();
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      dispatch("cancel");
    }
  }

  function onSave(): void {
    if (!loaded) return;
    const ids = visibleCollections
      .map((c) => c.id)
      .filter((id) => selection.has(id));
    dispatch("save", { collectionIds: ids });
  }

  function onCancel(): void {
    dispatch("cancel");
  }
</script>

{#if open}
  <div
    class="selector-backdrop"
    role="presentation"
    on:click|self={onCancel}
    data-testid="collection-selector-backdrop"
  >
    <div
      class="selector-dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="collection-selector-title"
      on:keydown={handleKeydown}
      data-testid="collection-selector"
    >
      <header class="selector-header">
        <h2 id="collection-selector-title">Agregar a colección</h2>
        <p class="muted">
          Cada captura ya pertenece a <strong>Historial</strong>;
          selecciona aquí colecciones adicionales.
        </p>
      </header>
      {#if !loaded}
        <p
          class="selector-info"
          role="status"
          data-testid="collection-selector-loading"
        >
          Cargando colecciones guardadas… Guardar se habilitará cuando
          termine la carga para no sobrescribir la selección persistida.
        </p>
      {/if}
      <input
        type="search"
        bind:value={search}
        bind:this={searchInputEl}
        placeholder="Buscar colección"
        aria-label="Buscar colección"
        class="selector-search"
        data-testid="collection-selector-search"
      />
      <ul class="selector-list" role="listbox" aria-multiselectable="true">
        {#each visibleCollections as collection (collection.id)}
          <li
            role="option"
            aria-selected={isSelected(collection.id)}
            data-testid="collection-selector-row"
            data-collection-id={collection.id}
          >
            <label>
              <input
                type="checkbox"
                checked={isSelected(collection.id)}
                on:change={() => toggle(collection.id)}
                data-testid="collection-selector-checkbox"
              />
              <span>{collection.name}</span>
            </label>
          </li>
        {:else}
          <li class="empty" data-testid="collection-selector-empty">
            {#if search.trim().length > 0}
              No hay colecciones que coincidan con “{search.trim()}”.
            {:else}
              Aún no has creado colecciones. Usa el sidebar para
              crear la primera.
            {/if}
          </li>
        {/each}
      </ul>
      <footer class="selector-footer">
        <button
          type="button"
          class="secondary"
          on:click={onCancel}
          data-testid="collection-selector-cancel"
        >
          Cancelar
        </button>
        <button
          type="button"
          class="primary"
          on:click={onSave}
          disabled={!loaded}
          data-testid="collection-selector-save"
        >
          Guardar
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .selector-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(8, 11, 16, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 60;
  }
  .selector-dialog {
    width: min(420px, 92vw);
    max-height: 80vh;
    background: #161b22;
    border: 1px solid #30363d;
    border-radius: 10px;
    padding: 1rem 1.25rem 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    color: inherit;
  }
  .selector-header h2 {
    margin: 0;
    font-size: 1.05rem;
  }
  .selector-header .muted {
    margin: 0.25rem 0 0;
    color: #94a3b8;
    font-size: 0.85rem;
  }
  .selector-search {
    width: 100%;
    background: #0e1116;
    color: inherit;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 0.45rem 0.6rem;
    box-sizing: border-box;
    font: inherit;
  }
  .selector-list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    max-height: 320px;
    border: 1px solid #1f2937;
    border-radius: 6px;
  }
  .selector-list li {
    padding: 0.45rem 0.6rem;
    border-top: 1px solid #1f2937;
  }
  .selector-list li:first-child {
    border-top: 0;
  }
  .selector-list label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
  }
  .selector-list .empty {
    color: #94a3b8;
    font-style: italic;
  }
  /*
   * Non-blocking notice surfaced while the per-entry hydration is
   * still in flight. The modal stays usable (search stays enabled)
   * but **Guardar** is disabled until `loaded` becomes `true`.
   */
  .selector-info {
    margin: 0;
    color: #94a3b8;
    font-size: 0.8rem;
    font-style: italic;
  }
  .selector-footer {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }
  button.primary {
    background: #2563eb;
    color: white;
    border: 0;
    padding: 0.45rem 0.9rem;
    border-radius: 6px;
    cursor: pointer;
  }
  button.secondary {
    background: transparent;
    color: inherit;
    border: 1px solid #30363d;
    padding: 0.45rem 0.9rem;
    border-radius: 6px;
    cursor: pointer;
  }
</style>