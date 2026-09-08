<script lang="ts">
  /**
   * Combobox the desktop toolbar renders between the source-app
   * filter and the configuration menu. The combobox pins the visible
   * cards to a single tag, or restores the unfiltered view with
   * `Todas`.
   *
   * The component is fully presentational: the parent owns the
   * filter state (so it can compose it with the collection, query
   * and source-app facets), loads the options against the active
   * scope and forwards every change back through `onChange`. The
   * combobox only manages:
   *
   *   - its own open/close state with a single listbox instance;
   *   - the active descendant through `aria-activedescendant`;
   *   - keyboard navigation (ArrowUp/Down, Home/End, Enter, Escape);
   *   - the click-outside detection anchored on idempotent document
   *     listeners (the toolbar installs the same pattern);
   *   - the chip-style trigger that mirrors the source-app filter
   *     so the two comboboxes read as siblings.
   *
   * The combobox never displays the stable `id` as visible text;
   * only `display_name` reaches the surface. A selection of `null`
   * collapses to the `Todas` option, and the combobox never
   * preserves a value outside the active scope — the parent is the
   * single source of truth for the reset-on-scope-change branch.
   */
  import { onDestroy, tick } from "svelte";
  import type { TagFilter, TagFilterOption } from "./types.ts";
  import { filterComboboxStyles } from "./lib/filterTokens.ts";

  export let selected: TagFilter;
  export let options: TagFilterOption[];
  export let ariaLabel: string = "Filtrar por tag";
  /**
   * Stable id used to wire `aria-controls` to the listbox. Kept on
   * the parent so the rest of the desktop (toolbar regression
   * suite, drag-and-drop tests) can target the combobox without
   * depending on internal Svelte ids.
   */
  export let testId: string = "tag-filter";
  export let onChange: (next: TagFilter) => void = () => {};

  let open = false;
  let triggerEl: HTMLButtonElement | null = null;
  let listEl: HTMLUListElement | null = null;
  /**
   * Monotonic active-descendant index. `-1` means no row is
   * highlighted (the trigger itself owns focus); the combobox
   * clamps it to the option list bounds on every keyboard event.
   */
  let activeIndex = -1;

  /**
   * Combined option list. The first row is always the `Todas`
   * sentinel; the remaining rows are the supplied `options` (the
   * parent already filtered them against the active scope). The
   * trigger label and the keyboard matcher read from this
   * projection so the visible combobox and the underlying filter
   * state never drift.
   */
  type ComboboxOption =
    | { kind: "all"; key: string; display_name: string }
    | { kind: "tag"; key: string; option: TagFilterOption };
  $: comboboxOptions = buildComboboxOptions(options);
  $: optionKeys = comboboxOptions.map((option) => option.key);
  $: activeKey = activeIndex >= 0 && activeIndex < optionKeys.length
    ? optionKeys[activeIndex]
    : null;
  $: triggerLabel = labelForSelected(selected);
  $: triggerAriaExpanded = open ? "true" : "false";
  $: triggerAriaActiveDescendant = open && activeKey
    ? `${testId}-option-${activeKey}`
    : null;

  function buildComboboxOptions(
    raw: readonly TagFilterOption[],
  ): ComboboxOption[] {
    const result: ComboboxOption[] = [
      { kind: "all", key: "all", display_name: "Todas" },
    ];
    for (const option of raw) {
      result.push({
        kind: "tag",
        key: `tag:${option.id}`,
        option,
      });
    }
    return result;
  }

  function labelForSelected(value: TagFilter): string {
    if (value.kind === "all") return "Todas";
    return options.find((option) => option.id === value.tagId)?.display_name ??
      "Tag";
  }

  function matchSelected(
    value: TagFilter,
    option: ComboboxOption,
  ): boolean {
    if (value.kind === "all") return option.kind === "all";
    return option.kind === "tag" && option.option.id === value.tagId;
  }

  function filterForKey(key: string): TagFilter | null {
    if (key === "all") return { kind: "all" };
    if (key.startsWith("tag:")) {
      const raw = key.slice("tag:".length);
      const id = Number.parseInt(raw, 10);
      if (!Number.isFinite(id) || !Number.isInteger(id)) return null;
      return { kind: "tag", tagId: id };
    }
    return null;
  }

  /**
   * Pick the option that matches the active `selected` filter so
   * the combobox always opens with a deterministic row highlighted.
   */
  function indexForSelected(
    value: TagFilter,
    list: ComboboxOption[],
  ): number {
    return list.findIndex((option) => matchSelected(value, option));
  }

  function focusActiveOption(): void {
    if (!listEl) return;
    const target = listEl.querySelector<HTMLElement>(
      `[data-testid="${testId}-option-${activeKey}"]`,
    );
    target?.scrollIntoView({ block: "nearest" });
  }

  function openCombobox(): void {
    if (open) return;
    open = true;
    activeIndex = Math.max(0, indexForSelected(selected, comboboxOptions));
    ensureWindowListeners();
    void tick().then(() => {
      focusActiveOption();
    });
  }

  function closeCombobox(opts: { restoreFocus?: boolean } = {}): void {
    if (!open) return;
    open = false;
    detachWindowListeners();
    if (opts.restoreFocus) {
      queueMicrotask(() => {
        triggerEl?.focus();
      });
    }
  }

  function toggle(): void {
    if (open) {
      closeCombobox({ restoreFocus: true });
    } else {
      openCombobox();
    }
  }

  function selectKey(key: string): void {
    const next = filterForKey(key);
    if (!next) return;
    onChange(next);
    closeCombobox({ restoreFocus: true });
  }

  function onTriggerKeydown(event: KeyboardEvent): void {
    if (event.key === "ArrowDown" || event.key === "ArrowUp" ||
        event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      openCombobox();
      if (event.key === "ArrowUp" && comboboxOptions.length > 0) {
        activeIndex = comboboxOptions.length - 1;
      }
      void tick().then(() => focusActiveOption());
    } else if (event.key === "Escape" && open) {
      event.preventDefault();
      closeCombobox({ restoreFocus: true });
    }
  }

  function onListboxKeydown(event: KeyboardEvent): void {
    if (!open) return;
    if (event.key === "Escape") {
      event.preventDefault();
      closeCombobox({ restoreFocus: true });
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      if (comboboxOptions.length === 0) return;
      activeIndex = (activeIndex + 1) % comboboxOptions.length;
      focusActiveOption();
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      if (comboboxOptions.length === 0) return;
      activeIndex =
        (activeIndex - 1 + comboboxOptions.length) % comboboxOptions.length;
      focusActiveOption();
    } else if (event.key === "Home") {
      event.preventDefault();
      if (comboboxOptions.length === 0) return;
      activeIndex = 0;
      focusActiveOption();
    } else if (event.key === "End") {
      event.preventDefault();
      if (comboboxOptions.length === 0) return;
      activeIndex = comboboxOptions.length - 1;
      focusActiveOption();
    } else if (event.key === "Enter") {
      event.preventDefault();
      if (activeIndex >= 0 && activeIndex < optionKeys.length) {
        selectKey(optionKeys[activeIndex]);
      }
    }
  }

  function onWindowPointerDown(event: MouseEvent): void {
    if (!open) return;
    const target = event.target as HTMLElement | null;
    if (!target) return;
    if (triggerEl && (target === triggerEl || triggerEl.contains(target))) {
      return;
    }
    if (listEl && listEl.contains(target)) {
      return;
    }
    closeCombobox({ restoreFocus: true });
  }

  function onWindowKeydown(event: KeyboardEvent): void {
    if (!open) return;
    if (event.key === "Escape") {
      event.preventDefault();
      closeCombobox({ restoreFocus: true });
    }
  }

  let listenersInstalled = false;
  function ensureWindowListeners(): void {
    if (listenersInstalled) return;
    if (typeof document === "undefined") return;
    document.addEventListener("pointerdown", onWindowPointerDown, true);
    document.addEventListener("keydown", onWindowKeydown, true);
    listenersInstalled = true;
  }
  function detachWindowListeners(): void {
    if (typeof document === "undefined") return;
    document.removeEventListener("pointerdown", onWindowPointerDown, true);
    document.removeEventListener("keydown", onWindowKeydown, true);
    listenersInstalled = false;
  }

  /**
   * Reset `activeIndex` whenever `selected` changes through the
   * parent so the listbox keeps the highlight in sync with the
   * trigger label.
   */
  $: {
    if (open) {
      activeIndex = Math.max(0, indexForSelected(selected, comboboxOptions));
    }
  }

  onDestroy(() => {
    detachWindowListeners();
  });
</script>

<div
    class="tag-filter"
    data-testid={testId}
    data-filter-width="220"
    data-filter-min-width="11rem"
    style={filterComboboxStyles}
  >
  <button
    type="button"
    class="trigger"
    aria-haspopup="listbox"
    aria-expanded={triggerAriaExpanded as "true" | "false"}
    aria-controls={`${testId}-listbox`}
    aria-label={ariaLabel}
    aria-activedescendant={triggerAriaActiveDescendant}
    bind:this={triggerEl}
    on:click={toggle}
    on:keydown={onTriggerKeydown}
    data-testid={`${testId}-trigger`}
    data-tag-filter-selected={selected.kind}
  >
    <span
      class="trigger-icon"
      aria-hidden="true"
      data-testid={`${testId}-trigger-icon`}
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
      >
        <path d="M20 12.5 12.5 20 4 20 4 11.5 11.5 4 20 4 20 12.5Z" />
        <circle cx="9" cy="9" r="1.2" fill="currentColor" />
      </svg>
    </span>
    <span class="trigger-label">{triggerLabel}</span>
    <svg
      aria-hidden="true"
      focusable="false"
      width="12"
      height="12"
      viewBox="0 0 12 12"
      fill="currentColor"
    >
      <path d="M3 5l3 3 3-3" />
    </svg>
  </button>
  {#if open}
    <ul
      class="listbox"
      role="listbox"
      id={`${testId}-listbox`}
      aria-label={ariaLabel}
      bind:this={listEl}
      on:keydown={onListboxKeydown}
      data-testid={`${testId}-listbox`}
    >
      {#each comboboxOptions as option, index (optionKeys[index])}
        {@const key = optionKeys[index]}
        {@const isActive = activeIndex === index}
        {@const isSelected = matchSelected(selected, option)}
        <li
          role="option"
          id={`${testId}-option-${key}`}
          aria-selected={isSelected}
          class:active={isActive}
          class:selected={isSelected}
          data-testid={`${testId}-option-${key}`}
          data-tag-filter-key={key}
          on:click={() => selectKey(key)}
          on:mouseenter={() => (activeIndex = index)}
        >
          <span class="option-label">
            {option.kind === "all"
              ? option.display_name
              : option.option.display_name}
          </span>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .tag-filter {
    /* The combobox MUST keep the same width the `SourceAppFilter`
     * sibling reserves. The width token lives in `lib/filterTokens.ts`
     * so the two comboboxes cannot drift; the inline `style`
     * attribute on the root div sets the same value so a future
     * CSS refactor cannot accidentally re-introduce the
     * intrinsic-width regression the manual QA pass detected. */
    position: relative;
    display: inline-flex;
    align-items: center;
    flex: 0 1 220px;
    min-width: 11rem;
    max-width: 220px;
    width: 220px;
  }

  .trigger {
    width: 100%;
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    background: var(--cv-bg-elevated, #161b22);
    color: var(--cv-fg, #f0f4f8);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-sm, 6px);
    padding: 0.45rem 0.65rem;
    font: inherit;
    font-size: var(--cv-control, 0.85rem);
    cursor: pointer;
    min-width: 0;
  }

  .trigger:hover {
    background: var(--cv-bg-hover, rgba(255, 255, 255, 0.06));
  }

  .trigger:focus-visible {
    outline: 2px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.45));
    outline-offset: 2px;
  }

  .trigger[aria-expanded="true"] {
    border-color: var(--cv-accent, #2563eb);
    box-shadow: 0 0 0 1px var(--cv-accent, #2563eb);
  }

  .trigger-icon {
    width: 1.1rem;
    height: 1.1rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--cv-fg-muted, #94a3b8);
  }

  .trigger-label {
    flex: 1 1 auto;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .listbox {
    position: absolute;
    top: calc(100% + 0.35rem);
    left: 0;
    right: 0;
    z-index: 30;
    list-style: none;
    margin: 0;
    padding: 0.35rem;
    background: var(--cv-bg-elevated, #161b22);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 10px);
    box-shadow: 0 14px 36px rgba(0, 0, 0, 0.34);
    max-height: 18rem;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }

  .listbox :global(li) {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.4rem 0.6rem;
    border-radius: var(--cv-radius-sm, 6px);
    cursor: pointer;
    color: var(--cv-fg, #f0f4f8);
    font-size: var(--cv-control, 0.85rem);
  }

  .listbox :global(li.active) {
    background: var(--cv-bg-hover, rgba(255, 255, 255, 0.08));
  }

  .listbox :global(li.selected) {
    color: var(--cv-accent, #2563eb);
  }

  .option-label {
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  @media (max-width: 640px) {
    .tag-filter {
      flex: 1 1 100%;
    }
  }
</style>
