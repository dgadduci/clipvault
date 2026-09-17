<script lang="ts">
  /**
   * Colour editor for a single collection. Triggered from the
   * sidebar's colour square (double click, Enter or Space).
   *
   * The modal reuses the existing `Modal` shell — focus trap,
   * Escape handling, backdrop dismissal and return-focus — and
   * offers:
   *
   *   - the four documented palette swatches (rojo, amarillo, verde,
   *     azul) as one-click shortcuts;
   *   - a native `<input type="color">` so the user can also pick any
   *     opaque RGB hex value the document allows;
   *   - the HEX form reflected through the picker input so the user
   *     sees the canonical `#rrggbb` representation at all times;
   *   - **Guardar** / **Cancelar** actions.
   *
   * The colour preview, the HEX label and the quick swatches react to
   * the live value the user picks; only confirming with **Guardar**
   * dispatches `save` (and the parent propagates the value to the
   * `clipvault_collections_set_color` Tauri command). The modal
   * intentionally does NOT update any backend state on its own — the
   * parent owns the persistence round-trip so the sidebar refresh
   * stays atomic and the metadata-only event remains the single
   * signal the rest of the UI observes.
   */
  import { createEventDispatcher, onMount, tick } from "svelte";
  import type { Collection } from "./types";
  import Modal from "./Modal.svelte";

  export let open: boolean;
  export let collection: Collection | null;

  type DispatchEvents = {
    save: { colorHex: string };
    cancel: void;
  };
  const dispatch = createEventDispatcher<DispatchEvents>();

  /** Canonical palette the change contract documents. */
  const PALETTE: Array<{ name: string; hex: string }> = [
    { name: "Rojo", hex: "#c62828" },
    { name: "Amarillo", hex: "#a16207" },
    { name: "Verde", hex: "#2e7d32" },
    { name: "Azul", hex: "#1565c0" },
  ];

  /** Hex the user is currently editing. Lowercased, normalised. */
  let draftHex: string = "#1565c0";
  /** Mirror of `draftHex` formatted for the `<input type="color">`. */
  let pickerValue: string = "#1565c0";

  let pickerEl: HTMLInputElement | null = null;

  function normalise(hex: string): string | null {
    // Accept `#RRGGBB`, `#RGB`, or `#rrggbb`; emit `#rrggbb`. Any
    // other shape is rejected and the previous value is kept.
    const trimmed = hex.trim();
    if (!trimmed.startsWith("#")) return null;
    const body = trimmed.slice(1);
    if (body.length === 3) {
      const expanded = body
        .split("")
        .map((c) => c + c)
        .join("");
      return /^[0-9a-fA-F]{6}$/.test(expanded) ? `#${expanded.toLowerCase()}` : null;
    }
    return /^[0-9a-fA-F]{6}$/.test(body) ? `#${body.toLowerCase()}` : null;
  }

  $: if (collection) {
    // Sync the draft to the collection's persisted colour every time
    // the modal opens against a different row. Empty / malformed
    // values fall back to the documented default so the picker has
    // a coherent value to render.
    const next = normalise(collection.color_hex) ?? "#1565c0";
    draftHex = next;
    pickerValue = next;
  }

  function applyDraft(value: string): void {
    const normalised = normalise(value);
    if (normalised === null) return;
    draftHex = normalised;
    pickerValue = normalised;
  }

  function onPickerInput(event: Event): void {
    const target = event.currentTarget as HTMLInputElement;
    applyDraft(target.value);
  }

  function onHexInput(event: Event): void {
    const target = event.currentTarget as HTMLInputElement;
    applyDraft(target.value);
  }

  function onPaletteClick(hex: string): void {
    applyDraft(hex);
  }

  function onSave(): void {
    dispatch("save", { colorHex: draftHex });
  }

  function onCancel(): void {
    dispatch("cancel");
  }

  function onDialogKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      onCancel();
    }
  }

  onMount(() => {
    if (!open) return;
    void tick().then(() => {
      pickerEl?.focus();
    });
  });

  $: if (open) {
    void tick().then(() => {
      pickerEl?.focus();
    });
  }
</script>

<Modal
  {open}
  titleId="collection-color-modal-title"
  title={`Color de ${collection?.name ?? "colección"}`}
  busy={false}
  onClose={onCancel}
>
  <div
    class="color-modal"
    data-testid="collection-color-modal"
    data-collection-id={collection?.id ?? null}
    on:keydown={onDialogKeydown}
  >
    <div class="color-modal-preview-row">
      <span
        class="color-modal-preview"
        style="background-color: {draftHex};"
        aria-hidden="true"
        data-testid="collection-color-modal-preview"
        data-color={draftHex}
      ></span>
      <code
        class="color-modal-hex"
        data-testid="collection-color-modal-hex"
      >
        {draftHex}
      </code>
    </div>

    <div
      class="color-modal-palette"
      role="group"
      aria-label="Paleta base"
      data-testid="collection-color-modal-palette"
    >
      {#each PALETTE as swatch (swatch.hex)}
        <button
          type="button"
          class="color-modal-swatch"
          class:active={swatch.hex === draftHex}
          style="background-color: {swatch.hex};"
          aria-label={`${swatch.name} ${swatch.hex}`}
          title={`${swatch.name} ${swatch.hex}`}
          data-testid="collection-color-modal-swatch"
          data-color={swatch.hex}
          on:click={() => onPaletteClick(swatch.hex)}
        ></button>
      {/each}
    </div>

    <label class="color-modal-picker-label" for="collection-color-modal-picker">
      Color personalizado
      <input
        id="collection-color-modal-picker"
        bind:this={pickerEl}
        bind:value={pickerValue}
        on:input={onPickerInput}
        type="color"
        class="color-modal-picker"
        aria-label="Selector de color"
        data-testid="collection-color-modal-picker"
      />
    </label>

    <label class="color-modal-hex-label" for="collection-color-modal-hex-input">
      HEX
      <input
        id="collection-color-modal-hex-input"
        type="text"
        class="color-modal-hex-input"
        value={draftHex}
        on:input={onHexInput}
        spellcheck="false"
        autocomplete="off"
        aria-label="Valor hexadecimal del color"
        data-testid="collection-color-modal-hex-input"
      />
    </label>

    <footer class="color-modal-actions">
      <button
        type="button"
        class="modal-action modal-action-secondary"
        on:click={onCancel}
        data-testid="collection-color-modal-cancel"
      >
        Cancelar
      </button>
      <button
        type="button"
        class="modal-action modal-action-primary"
        on:click={onSave}
        data-testid="collection-color-modal-save"
      >
        Guardar
      </button>
    </footer>
  </div>
</Modal>

<style>
  :global(.color-modal) {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    min-width: 320px;
  }
  :global(.color-modal-preview-row) {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }
  :global(.color-modal-preview) {
    display: inline-block;
    width: 2.4rem;
    height: 2.4rem;
    border-radius: 6px;
    border: 1px solid var(--cv-border, #30363d);
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.25);
  }
  :global(.color-modal-hex) {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 1rem;
    color: var(--cv-fg, #f0f4f8);
    background: rgba(255, 255, 255, 0.04);
    padding: 0.25rem 0.5rem;
    border-radius: 4px;
    border: 1px solid var(--cv-border, #30363d);
    letter-spacing: 0.02em;
  }
  :global(.color-modal-palette) {
    display: flex;
    gap: 0.5rem;
  }
  :global(.color-modal-swatch) {
    appearance: none;
    border: 2px solid transparent;
    width: 1.85rem;
    height: 1.85rem;
    border-radius: 6px;
    cursor: pointer;
    padding: 0;
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.25);
  }
  :global(.color-modal-swatch.active) {
    border-color: var(--cv-focus-ring, rgba(37, 99, 235, 0.85));
    outline: 1px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.85));
    outline-offset: 1px;
  }
  :global(.color-modal-swatch:focus-visible) {
    border-color: var(--cv-focus-ring, rgba(37, 99, 235, 0.85));
    outline: 2px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.45));
    outline-offset: 1px;
  }
  :global(.color-modal-picker-label),
  :global(.color-modal-hex-label) {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    font-size: var(--cv-body, 0.9rem);
    color: var(--cv-fg, #f0f4f8);
  }
  :global(.color-modal-picker) {
    appearance: none;
    width: 100%;
    height: 2.4rem;
    border-radius: 6px;
    border: 1px solid var(--cv-border, #30363d);
    background: transparent;
    padding: 0.15rem;
    cursor: pointer;
  }
  :global(.color-modal-hex-input) {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.95rem;
    background: #0e1116;
    color: inherit;
    border: 1px solid var(--cv-border, #30363d);
    border-radius: 6px;
    padding: 0.35rem 0.5rem;
    width: 8rem;
    box-sizing: border-box;
  }
  :global(.color-modal-actions) {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  :global(.modal-action-primary) {
    background: var(--cv-accent, #2563eb);
    color: white;
  }
  :global(.modal-action-primary:hover:not(:disabled)) {
    background: #1d4ed8;
  }
</style>
