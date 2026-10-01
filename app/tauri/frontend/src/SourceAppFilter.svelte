<script lang="ts">
  /**
   * Combobox the desktop toolbar renders between the search input
   * and the configuration menu. The combobox pins the visible
   * cards to a single source application, to the synthetic
   * "Aplicación desconocida" branch or to "Todas" (no restriction).
   *
   * The component is fully presentational: the parent owns the
   * filter state (so it can compose it with collection, query and
   * tag facets), loads the options through the
   * `sourceApplicationsCommand` bridge and forwards every change
   * back through `onChange`. The combobox only manages:
   *
   *   - its own open/close state with a single listbox instance;
   *   - the active descendant through `aria-activedescendant`;
   *   - keyboard navigation (ArrowUp/Down, Home/End, Enter, Escape);
   *   - the click-outside detection anchored on idempotent document
   *     listeners (the toolbar installs the same pattern);
   *   - the icon resolver cache keyed by `icon_ref`, with explicit
   *     Blob URL cleanup on every option disappearance and on
   *     `onDestroy`.
   *
   * The component never displays the stable `source_app`
   * identifier as visible text; only `display_name` reaches the
   * surface. A failed icon resolution collapses to the generic
   * glyph so the filter selection is never blocked by a missing
   * icon.
   */
  import { onDestroy, tick } from "svelte";
  import { anchorPopupToViewport } from "./lib/anchorPopupToViewport.ts";
  import type {
    IconLoader,
    IconResolver,
  } from "./lib/iconResolver.ts";
  import { createIconResolver } from "./lib/iconResolver.ts";
  import { sourceAppIconCommand } from "./lib/tauri.ts";
  import { APP_FALLBACK_ICON_SVG } from "./lib/contentTypeIcons.ts";
  import { filterComboboxStyles } from "./lib/filterTokens.ts";
  import type {
    SourceAppFilter,
    SourceApplicationOption,
  } from "./types.ts";

  export let selected: SourceAppFilter;
  export let options: SourceApplicationOption[];
  export let ariaLabel: string = "Filtrar por aplicación fuente";
  /**
   * Stable id used to wire `aria-controls` to the listbox. Kept on
   * the parent so the rest of the desktop (toolbar regression
   * suite, drag-and-drop tests) can target the combobox without
   * depending on internal Svelte ids.
   */
  export let testId: string = "source-app-filter";
  export let onChange: (next: SourceAppFilter) => void = () => {};

  let open = false;
  let triggerEl: HTMLButtonElement | null = null;
  let listEl: HTMLUListElement | null = null;
  /**
   * Monotonic active-descendant index. `-1` means no row is
   * highlighted (the trigger itself owns focus); the combobox
   * clamps it to the option list bounds on every keyboard event.
   */
  let activeIndex = -1;

  $: optionKeys = options.map((option) => optionKey(option));
  $: activeKey = activeIndex >= 0 && activeIndex < optionKeys.length
    ? optionKeys[activeIndex]
    : null;
  $: triggerLabel = labelForSelected(selected);
  $: triggerAriaExpanded = open ? "true" : "false";
  $: triggerAriaActiveDescendant = open && activeKey ? `${testId}-option-${activeKey}` : null;
  $: selectedOptionIconUrl = selected.kind === "known" || selected.kind === "imported"
    ? iconUrls.get(filterKey(selected)) ?? null
    : null;

  /**
   * Translate a `SourceApplicationOption` into a stable listbox key.
   * The `Todas` and `Aplicación desconocida` rows share a `null`
   * `source_app`, so the key encodes the discriminator instead of
   * the identifier to keep every row addressable.
   */
  function optionKey(option: SourceApplicationOption): string {
    return filterKey(option.filter);
  }

  function filterKey(filter: SourceAppFilter): string {
    switch (filter.kind) {
      case "all": return "all";
      case "unknown": return "unknown";
      case "known": return `known:${filter.source_app}`;
      case "imported": return `imported:${encodeURIComponent(filter.display_name)}`;
    }
  }

  function labelForSelected(value: SourceAppFilter): string {
    switch (value.kind) {
      case "all":
        return "Todas";
      case "known":
        return options.find((option) => option.source_app === value.source_app)?.display_name ??
          "Aplicación";
      case "imported":
        return value.display_name;
      case "unknown":
        return "Aplicación desconocida";
    }
  }

  function matchSelected(
    value: SourceAppFilter,
    option: SourceApplicationOption,
  ): boolean {
    switch (value.kind) {
      case "all":
        return option.display_name === "Todas";
      case "unknown":
        return option.display_name === "Aplicación desconocida";
      case "known":
        return option.source_app === value.source_app;
      case "imported":
        return option.filter.kind === "imported" &&
          option.filter.display_name === value.display_name;
    }
  }

  function filterForKey(key: string): SourceAppFilter | null {
    return options.find((option) => optionKey(option) === key)?.filter ?? null;
  }

  /**
   * Pick the option that matches the active `selected` filter so
   * the combobox always opens with a deterministic row highlighted.
   */
  function indexForSelected(
    value: SourceAppFilter,
    list: SourceApplicationOption[],
  ): number {
    return list.findIndex((option) => {
      switch (value.kind) {
        case "all":
          return option.display_name === "Todas";
        case "unknown":
          return option.display_name === "Aplicación desconocida";
        case "known":
          return option.source_app === value.source_app;
        case "imported":
          return option.filter.kind === "imported" &&
            option.filter.display_name === value.display_name;
      }
    });
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
    activeIndex = Math.max(0, indexForSelected(selected, options));
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
      if (event.key === "ArrowUp" && options.length > 0) {
        activeIndex = options.length - 1;
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
      if (options.length === 0) return;
      activeIndex = (activeIndex + 1) % options.length;
      focusActiveOption();
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      if (options.length === 0) return;
      activeIndex = (activeIndex - 1 + options.length) % options.length;
      focusActiveOption();
    } else if (event.key === "Home") {
      event.preventDefault();
      if (options.length === 0) return;
      activeIndex = 0;
      focusActiveOption();
    } else if (event.key === "End") {
      event.preventDefault();
      if (options.length === 0) return;
      activeIndex = options.length - 1;
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

  // ------------------------------------------------------------------
  // Icon cache. The combobox owns a single IconResolver keyed by
  // `optionKey` -> icon_ref. The fallback flag the backend reports
  // drives the render path: when `fallback === true` the combobox
  // renders the generic glyph and never requests bytes from the
  // bridge. The resolver still resolves through the icon bridge for
  // the other options and revokes every Blob URL when the combobox
  // is torn down.
  // ------------------------------------------------------------------
  const tauriSourceAppIconLoader: IconLoader = {
    async loadIconBytes(ref: string): Promise<number[] | Uint8Array | null> {
      try {
        return await sourceAppIconCommand({ ref });
      } catch {
        return null;
      }
    },
  };
  const iconResolver: IconResolver = createIconResolver(tauriSourceAppIconLoader);

  /**
   * Map of `optionKey -> icon URL (or null when the resolver has
   * not yet produced a Blob)`. The combobox renders the fallback
   * glyph while a Blob is in flight so a slow bridge call cannot
   * flash an empty cell.
   */
  let iconUrls = new Map<string, string | null>();
  /**
   * Monotonic counter the reactive block below uses to drop stale
   * resolution promises. The bridge may resolve out of order when
   * the user picks a different option before the previous bridge
   * call completes.
   */
  let iconRefreshTokens = new Map<string, number>();

  async function refreshIcon(key: string, ref: string | null): Promise<void> {
    const token = (iconRefreshTokens.get(key) ?? 0) + 1;
    iconRefreshTokens.set(key, token);
    if (!ref) {
      if (iconRefreshTokens.get(key) === token) {
        const next = new Map(iconUrls);
        next.set(key, null);
        iconUrls = next;
      }
      return;
    }
    const resolution = await iconResolver.resolve(ref);
    if (iconRefreshTokens.get(key) !== token) {
      return;
    }
    const next = new Map(iconUrls);
    next.set(key, resolution.ok ? resolution.url : null);
    iconUrls = next;
  }

  $: {
    const next = new Map(iconUrls);
    for (let index = 0; index < options.length; index += 1) {
      const option = options[index];
      const key = optionKeys[index];
      if (!option.fallback && option.icon_ref) {
        if (!iconUrls.has(key)) {
          void refreshIcon(key, option.icon_ref);
        }
      } else {
        next.set(key, null);
      }
    }
    iconUrls = next;
  }

  /**
   * Reset `activeIndex` whenever `selected` changes through the
   * parent so the listbox keeps the highlight in sync with the
   * trigger label.
   */
  $: {
    if (open) {
      activeIndex = Math.max(0, indexForSelected(selected, options));
    }
  }

  onDestroy(() => {
    detachWindowListeners();
    iconResolver.release();
  });
</script>

<div
    class="source-app-filter"
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
    data-source-app-selected={selected.kind}
  >
    <span class="trigger-icon" aria-hidden="true" data-testid={`${testId}-trigger-icon`}>
      {#if (selected.kind === "known" || selected.kind === "imported") && selectedOptionIconUrl}
        <img
          src={selectedOptionIconUrl}
          alt=""
          data-testid={`${testId}-trigger-icon-img`}
        />
      {:else}
        {@html APP_FALLBACK_ICON_SVG}
      {/if}
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
      use:anchorPopupToViewport={{
        anchor: () => triggerEl,
        matchAnchorWidth: true,
      }}
      on:keydown={onListboxKeydown}
      data-testid={`${testId}-listbox`}
    >
      {#each options as option, index (optionKeys[index])}
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
          data-source-app-key={key}
          on:click={() => selectKey(key)}
          on:mouseenter={() => (activeIndex = index)}
        >
          <span class="option-icon" aria-hidden="true">
            {#if option.fallback || !iconUrls.get(key)}
              {@html APP_FALLBACK_ICON_SVG}
            {:else}
              <img src={iconUrls.get(key) ?? undefined} alt="" />
            {/if}
          </span>
          <span class="option-label">{option.display_name}</span>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .source-app-filter {
    /* The combobox shares its width with the `TagFilter` sibling.
     * The token module `lib/filterTokens.ts` owns the canonical
     * `220px` / `11rem` baseline; the inline `style` on the root
     * div applies the same declaration so a CSS refactor cannot
     * re-introduce the drift the manual QA pass detected. */
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

  .trigger-icon :global(svg) {
    width: 100%;
    height: 100%;
  }

  .trigger-icon :global(img) {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .trigger-label {
    flex: 1 1 auto;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .listbox {
    position: fixed;
    top: 0;
    left: 0;
    right: auto;
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

  .option-icon {
    width: 1rem;
    height: 1rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--cv-fg-muted, #94a3b8);
  }

  .option-icon :global(svg),
  .option-icon :global(img) {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .option-label {
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

</style>
