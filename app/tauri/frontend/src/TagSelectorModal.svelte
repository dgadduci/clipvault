<script lang="ts">
  /**
   * Multi-select tag picker used by the card menu's **Agregar tag**
   * action. The component is accessible: a single search/create
   * input, a checkbox-per-row list, **Guardar** / **Cancelar**
   * actions and a keyboard flow that respects `Escape` (close
   * without saving), `Enter` (when the input still holds text, run
   * the **Añadir** step) and focusable close affordances.
   *
   * The component is intentionally controlled: the parent owns the
   * selected ids and the candidate list, and the modal just
   * renders the current state. Saving returns the full id set so
   * the parent can persist it atomically via `entry_tags_set`.
   * Cancelling returns nothing and the parent keeps its previous
   * state.
   *
   * The picker unifies search and creation into a single input
   * ("Buscar o crear tag…"). Typing filters the list as the user
   * types; pressing **Añadir** (or Enter while the input holds
   * text) either reuses an existing tag with the same normalised
   * identity or asks the core to create a brand-new tag. The
   * returned id is added to the local selection immediately; the
   * modal never invents or echoes clipboard content, hashes or
   * asset references, and the dispatched payload only carries the
   * resulting tag ids — never user-supplied names.
   */
  import { createEventDispatcher, tick } from "svelte";
  import type { Tag } from "./types";
  import { tagsCreateCommand } from "./lib/tauri";

  export let open: boolean;
  export let candidateTags: Tag[] = [];
  export let initialSelection: number[] = [];
  /**
   * `true` once the parent has loaded the entry's persisted tag set
   * from SQLite. The modal blocks **Guardar** while this is `false`
   * so the user cannot dispatch a stale `[]` selection against an
   * entry whose associations are still being hydrated. The
   * candidate list and the **Añadir** flow stay enabled so the
   * user can still browse and create tags while the cache loads.
   */
  export let loaded: boolean = true;

  type DispatchEvents = {
    save: { tagIds: number[] };
    cancel: void;
  };
  const dispatch = createEventDispatcher<DispatchEvents>();

  /** Unified input value: both search and create flows share it. */
  let search = "";
  /**
   * Selected tag ids. The parent owns the authoritative set; the
   * modal adds ids it received from the `tags_create` round-trip so
   * the user sees them in the list instantly. Cancelling discards
   * this local state, ensuring **Cancelar** leaves the entry's
   * associations untouched.
   */
  let selection: Set<number> = new Set(initialSelection);
  /**
   * Map of `{ id → Tag }` for tags created during this modal
   * session but not yet mirrored in the candidate snapshot. The
   * modal merges them with the parent-supplied candidates for
   * list rendering so a freshly created tag appears without
   * waiting for the `organization-updated` event the parent
   * consumes in the background.
   */
  let locallyCreated: Map<number, Tag> = new Map();
  let searchInputEl: HTMLInputElement | null = null;
  let adding = false;
  let saving = false;
  let addError: string | null = null;
  /** Last visible character count the modal observed. */
  const MAX_NAME_CHARS = 80;

  /**
   * Merged candidate list. Dedup by id so the freshly created tag
   * never renders twice while the parent snapshot still catches
   * up to the new row.
   */
  $: candidates = (() => {
    const seen = new Set<number>();
    const out: Tag[] = [];
    for (const tag of candidateTags) {
      if (!seen.has(tag.id)) {
        seen.add(tag.id);
        out.push(tag);
      }
    }
    for (const tag of locallyCreated.values()) {
      if (!seen.has(tag.id)) {
        seen.add(tag.id);
        out.push(tag);
      }
    }
    return out;
  })();

  /**
   * Case-insensitive substring filter on `display_name` and
   * `normalized_name`. Mirrors the core normalisation
   * (trim + whitespace collapse) so the modal and the repo agree
   * on what counts as a match.
   */
  $: filteredTags = (() => {
    const term = search.trim().split(/\s+/).join(" ").toLowerCase();
    if (!term) return candidates;
    return candidates.filter((tag) =>
      [tag.display_name, tag.normalized_name]
        .some((field) => field && field.toLowerCase().includes(term)),
    );
  })();

  $: trimmedInput = search.trim();
  /**
   * Save is only enabled once the per-entry hydration has finished
   * loading the persisted tag set. The user's selection is derived
   * from `initialSelection`; saving while the parent still owns the
   * "pending" state would race against the hydration round-trip and
   * could clobber persisted tags with a stale `[]` selection.
   *
   * The button is also disabled while the modal is mid-save or
   * mid-create, mirroring the existing `saving` / `adding` guards.
   */
  $: saveEnabled = loaded && !saving && !adding;
  /**
   * Existing tag whose normalised identity matches the current
   * input. When present, **Añadir** short-circuits to a selection
   * (case-insensitive reuse) instead of asking the core to create
   * a duplicate row.
   */
  $: matchingExistingTag = (() => {
    if (!trimmedInput) return null;
    const lowered = trimmedInput.toLowerCase();
    return (
      candidates.find(
        (tag) => tag.normalized_name.toLowerCase() === lowered,
      ) ?? null
    );
  })();

  $: addEnabled = trimmedInput.length > 0 && !adding;

  function toggle(tagId: number): void {
    const next = new Set(selection);
    if (next.has(tagId)) {
      next.delete(tagId);
    } else {
      next.add(tagId);
    }
    selection = next;
  }

  function isSelected(tagId: number): boolean {
    return selection.has(tagId);
  }

  /**
   * Reset the local state. Called whenever the modal opens so a
   * cancelled-then-reopened session never carries stale
   * selections or draft names.
   */
  function reset(initial: number[]): void {
    selection = new Set(initial);
    search = "";
    locallyCreated = new Map();
    addError = null;
    adding = false;
    saving = false;
  }

  $: if (open) {
    reset(initialSelection);
  }

  async function focusSearch(): Promise<void> {
    await tick();
    searchInputEl?.focus();
  }

  $: if (open) {
    void focusSearch();
  }

  function onDialogKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      onCancel();
    }
  }

  /**
   * Handle **Añadir**. Three paths:
   *
   *   1. input trimmed empty → no-op (button stays disabled);
   *   2. text matches an existing tag (case-insensitive) → reuse
   *      its id (no backend call), select it, clear the input;
   *   3. otherwise → call `clipvault_tags_create`, add the
   *      returned id to the local selection and snapshot.
   *
   * Re-entry is guarded by `adding`. Multiple rapid presses never
   * produce duplicate backend calls or duplicate rows: the snapshot
   * is merged by id and the backend itself is idempotent.
   */
  async function onAdd(): Promise<void> {
    addError = null;
    const trimmed = trimmedInput;
    if (!trimmed || adding) return;
    if (trimmed.length > MAX_NAME_CHARS) {
      addError = `El nombre supera los ${MAX_NAME_CHARS} caracteres.`;
      return;
    }
    // Path 2: match an existing tag case-insensitively. No
    // backend round-trip; the modal selects the existing id so a
    // second tap (or a save) keeps the entry's tag set pointing
    // at the same row the user can already see in the list.
    const existing = matchingExistingTag;
    if (existing !== null) {
      const next = new Set(selection);
      next.add(existing.id);
      selection = next;
      search = "";
      return;
    }
    adding = true;
    try {
      const created = await tagsCreateCommand({ name: trimmed });
      const next = new Map(locallyCreated);
      next.set(created.id, created);
      locallyCreated = next;
      const nextSelection = new Set(selection);
      nextSelection.add(created.id);
      selection = nextSelection;
      search = "";
    } catch (error) {
      addError = error instanceof Error ? error.message : String(error);
    } finally {
      adding = false;
    }
  }

  /**
   * Submit a typed name when the user presses Enter while the
   * input still holds text. The unified input makes the keyboard
   * affordance identical to the **Añadir** button: a single press
   * triggers exactly one backend call (or none, when the input
   * already matches an existing tag), and a save always forwards
   * the union of every selected id.
   */
  function onInputKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      event.preventDefault();
      void onAdd();
    } else if (event.key === "Escape") {
      // Inside the input, Escape first clears the text so a
      // second Escape is what closes the modal via the dialog
      // handler. Avoids accidental dismissal while the user is
      // typing.
      if (search.length > 0) {
        event.preventDefault();
        event.stopPropagation();
        search = "";
        addError = null;
      }
    }
  }

  /**
   * Persist the final selection atomically. If the input still
   * holds text the modal flushes it through the same **Añadir**
   * path so the **Guardar** button never silently drops a freshly
   * typed name. After dispatching, the parent's
   * `entry_tags_set` command runs and the backend emits
   * `clipvault://organization-updated` so the card refreshes
   * automatically.
   *
   * The dispatched `save` event is forwarded by the HistoryCard
   * into a Promise the parent mutation handler resolves. The modal
   * never closes itself: the card awaits the parent's persistence
   * round-trip and only flips `tagSelectorOpen` on resolution so a
   * backend rejection keeps the modal open with the error visible.
   * The button is also disabled while `loaded === false` so a save
   * never fires before the persisted tag set is in memory.
   */
  async function onSave(): Promise<void> {
    if (!saveEnabled) return;
    if (trimmedInput.length > 0) {
      addError = null;
      await onAdd();
      if (addError !== null) {
        // Surface the inline error; the modal stays open so the
        // user can fix the typed name without losing their
        // selection.
        return;
      }
    }
    saving = true;
    try {
      const ids = candidates
        .map((tag) => tag.id)
        .filter((id) => selection.has(id));
      dispatch("save", { tagIds: ids });
    } finally {
      saving = false;
    }
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
    data-testid="tag-selector-backdrop"
  >
    <div
      class="selector-dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="tag-selector-title"
      on:keydown={onDialogKeydown}
      data-testid="tag-selector"
    >
      <header class="selector-header">
        <h2 id="tag-selector-title">Agregar tags</h2>
        <p class="muted">
          Busca o crea un tag. La asociación se guarda atómicamente.
        </p>
      </header>
      <div class="selector-input-row">
        <input
          type="search"
          bind:value={search}
          bind:this={searchInputEl}
          placeholder="Buscar o crear tag…"
          aria-label="Buscar o crear tag"
          maxlength={MAX_NAME_CHARS}
          on:keydown={onInputKeydown}
          class="selector-search"
          data-testid="tag-selector-search"
        />
        <button
          type="button"
          class="secondary small"
          aria-label="Añadir tag a la selección"
          disabled={!addEnabled}
          on:click={() => void onAdd()}
          data-testid="tag-selector-create-add"
        >
          {adding ? "Añadiendo…" : "Añadir"}
        </button>
      </div>
{#if addError}
    <p
      class="selector-error"
      role="alert"
      data-testid="tag-selector-create-error"
    >
      {addError}
    </p>
  {/if}
  {#if !loaded}
    <p
      class="selector-info"
      role="status"
      data-testid="tag-selector-loading"
    >
      Cargando tags guardados… Guardar se habilitará cuando termine la
      carga para no sobrescribir la selección persistida.
    </p>
  {/if}
      <ul class="selector-list" role="listbox" aria-multiselectable="true">
        {#each filteredTags as tag (tag.id)}
          <li
            role="option"
            aria-selected={isSelected(tag.id)}
            data-testid="tag-selector-row"
            data-tag-id={tag.id}
          >
            <label>
              <input
                type="checkbox"
                checked={isSelected(tag.id)}
                on:change={() => toggle(tag.id)}
                data-testid="tag-selector-checkbox"
              />
              <span class="tag-label">{tag.display_name}</span>
            </label>
          </li>
        {:else}
          <li class="empty" data-testid="tag-selector-empty">
            {#if search.trim().length > 0}
              {#if matchingExistingTag}
                Pulsa **Añadir** para usar “{matchingExistingTag.display_name}”.
              {:else}
                Sin coincidencias. Pulsa **Añadir** para crear “{trimmedInput}”.
              {/if}
            {:else}
              Aún no has creado tags. Escribe un nombre para empezar.
            {/if}
          </li>
        {/each}
      </ul>
      <footer class="selector-footer">
        <button
          type="button"
          class="secondary"
          on:click={onCancel}
          data-testid="tag-selector-cancel"
        >
          Cancelar
        </button>
        <button
          type="button"
          class="primary"
          on:click={() => void onSave()}
          disabled={!saveEnabled}
          data-testid="tag-selector-save"
        >
          {saving ? "Guardando…" : "Guardar"}
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
  .selector-input-row {
    display: flex;
    gap: 0.4rem;
    align-items: center;
  }
  .selector-search {
    flex: 1;
    background: #0e1116;
    color: inherit;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 0.45rem 0.6rem;
    box-sizing: border-box;
    font: inherit;
  }
  .selector-error {
    margin: 0;
    color: #f87171;
    font-size: 0.8rem;
  }
  /*
   * Non-blocking notice surfaced while the per-entry hydration is
   * still in flight. The modal stays usable (search and **Añadir**
   * remain enabled) but **Guardar** is disabled until `loaded`
   * becomes `true`.
   */
  .selector-info {
    margin: 0;
    color: #94a3b8;
    font-size: 0.8rem;
    font-style: italic;
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
  button.ghost {
    background: transparent;
    color: #93c5fd;
    border: 0;
  }
  button.small {
    padding: 0.3rem 0.7rem;
    font-size: 0.8rem;
  }
  button:disabled {
    background: #374151;
    color: #94a3b8;
    cursor: not-allowed;
    border-color: #30363d;
  }
</style>
