<script lang="ts">
  /**
   * Single desktop capture-type combobox. `App.svelte` owns the selection and
   * loads the complete type set for the active scope; this component only
   * renders canonical icons and translated labels and reports user changes.
   */
  import { onDestroy, tick } from "svelte";
  import { anchorPopupToViewport } from "./lib/anchorPopupToViewport.ts";
  import { ACTIVE_FILTER_SURFACES, compactFilterComboboxStyles } from "./lib/filterTokens.ts";
  import {
    CONTENT_TYPE_ICON_SPRITE,
    contentTypeIconId,
    contentTypeTranslationKey,
  } from "./lib/contentTypeIcons.ts";
  import type { ContentTypeValue } from "./lib/contentType.ts";
  import { t } from "./lib/localization.ts";

  export let selected: ContentTypeValue | null;
  export let options: ContentTypeValue[];
  export let ariaLabel: string = "";
  export let testId: string = "content-type-filter";
  export let onChange: (next: ContentTypeValue | null) => void = () => {};

  type ComboboxOption =
    | { kind: "all"; key: "all" }
    | { kind: "type"; key: string; contentType: ContentTypeValue };

  let open = false;
  let triggerEl: HTMLButtonElement | null = null;
  let listEl: HTMLUListElement | null = null;
  let activeIndex = -1;

  $: comboboxOptions = buildComboboxOptions(options);
  $: optionKeys = comboboxOptions.map((option) => option.key);
  $: activeKey = activeIndex >= 0 && activeIndex < optionKeys.length
    ? optionKeys[activeIndex]
    : null;
  $: triggerLabel = selected === null
    ? $t("filter.all")
    : $t(contentTypeTranslationKey(selected));
  $: triggerAriaExpanded = open ? "true" : "false";
  $: triggerAriaActiveDescendant = open && activeKey
    ? `${testId}-option-${activeKey}`
    : null;

  function buildComboboxOptions(raw: readonly ContentTypeValue[]): ComboboxOption[] {
    const result: ComboboxOption[] = [{ kind: "all", key: "all" }];
    for (const contentType of new Set(raw)) {
      result.push({ kind: "type", key: `type:${contentType}`, contentType });
    }
    return result;
  }

  function optionLabel(option: ComboboxOption): string {
    return option.kind === "all"
      ? $t("filter.all")
      : $t(contentTypeTranslationKey(option.contentType));
  }

  function matchSelected(value: ContentTypeValue | null, option: ComboboxOption): boolean {
    return value === null
      ? option.kind === "all"
      : option.kind === "type" && option.contentType === value;
  }

  function filterForKey(key: string): ContentTypeValue | null | undefined {
    if (key === "all") return null;
    const option = comboboxOptions.find((candidate) => candidate.key === key);
    return option?.kind === "type" ? option.contentType : undefined;
  }

  function indexForSelected(
    value: ContentTypeValue | null,
    list: ComboboxOption[],
  ): number {
    return list.findIndex((option) => matchSelected(value, option));
  }

  function focusActiveOption(): void {
    if (!listEl || activeKey === null) return;
    listEl.querySelector<HTMLElement>(
      `[data-testid="${testId}-option-${activeKey}"]`,
    )?.scrollIntoView({ block: "nearest" });
  }

  function openCombobox(): void {
    if (open) return;
    open = true;
    activeIndex = Math.max(0, indexForSelected(selected, comboboxOptions));
    ensureWindowListeners();
    void tick().then(focusActiveOption);
  }

  function closeCombobox(restoreFocus = false): void {
    if (!open) return;
    open = false;
    detachWindowListeners();
    if (restoreFocus) queueMicrotask(() => triggerEl?.focus());
  }

  function toggle(): void {
    if (open) closeCombobox(true);
    else openCombobox();
  }

  function selectKey(key: string): void {
    const next = filterForKey(key);
    if (next === undefined) return;
    onChange(next);
    closeCombobox(true);
  }

  function onTriggerKeydown(event: KeyboardEvent): void {
    if (open) {
      onListboxKeydown(event);
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp" ||
        event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      openCombobox();
      if (event.key === "ArrowUp") activeIndex = comboboxOptions.length - 1;
      void tick().then(focusActiveOption);
    } else if (event.key === "Escape" && open) {
      event.preventDefault();
      closeCombobox(true);
    }
  }

  function onListboxKeydown(event: KeyboardEvent): void {
    if (!open) return;
    if (event.key === "Escape") {
      event.preventDefault();
      closeCombobox(true);
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      activeIndex = (activeIndex + step + comboboxOptions.length) % comboboxOptions.length;
      focusActiveOption();
    } else if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      activeIndex = event.key === "Home" ? 0 : comboboxOptions.length - 1;
      focusActiveOption();
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (activeIndex >= 0 && activeIndex < optionKeys.length) {
        selectKey(optionKeys[activeIndex]);
      }
    }
  }

  function onWindowPointerDown(event: MouseEvent): void {
    if (!open) return;
    const target = event.target as HTMLElement | null;
    if (!target || triggerEl?.contains(target) || listEl?.contains(target)) return;
    closeCombobox(true);
  }

  function onWindowKeydown(event: KeyboardEvent): void {
    if (!open || event.key !== "Escape") return;
    event.preventDefault();
    closeCombobox(true);
  }

  let listenersInstalled = false;
  function ensureWindowListeners(): void {
    if (listenersInstalled || typeof document === "undefined") return;
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

  $: if (open) {
    activeIndex = Math.max(0, indexForSelected(selected, comboboxOptions));
  }

  onDestroy(detachWindowListeners);
</script>

<div
  class="content-type-filter"
  data-testid={testId}
  data-filter-width="110"
  data-filter-min-width="5.5rem"
  style={`${compactFilterComboboxStyles} --filter-active-background: ${ACTIVE_FILTER_SURFACES.contentType};`}
>
  {@html CONTENT_TYPE_ICON_SPRITE}
  <button
    type="button"
    class="trigger"
    role="combobox"
    aria-haspopup="listbox"
    aria-expanded={triggerAriaExpanded as "true" | "false"}
    aria-controls={`${testId}-listbox`}
    aria-label={ariaLabel || $t("filter.content_type")}
    aria-activedescendant={triggerAriaActiveDescendant}
    title={triggerLabel}
    bind:this={triggerEl}
    on:click={toggle}
    on:keydown={onTriggerKeydown}
    data-testid={`${testId}-trigger`}
    data-content-type-filter-selected={selected ?? "all"}
    data-filter-active={selected !== null}
  >
    <span class="trigger-icon" aria-hidden="true" data-testid={`${testId}-trigger-icon`}>
      <svg aria-hidden="true" focusable="false" width="15" height="15" viewBox="0 0 24 24">
        <use href={`#${contentTypeIconId(selected)}`} />
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
      aria-label={ariaLabel || $t("filter.content_type")}
      bind:this={listEl}
      use:anchorPopupToViewport={{ anchor: () => triggerEl, matchAnchorWidth: true }}
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
          title={optionLabel(option)}
          data-testid={`${testId}-option-${key}`}
          data-content-type-filter-key={key}
          on:click={() => selectKey(key)}
          on:mouseenter={() => (activeIndex = index)}
          on:keydown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              selectKey(key);
            }
          }}
        >
          <span class="option-icon" aria-hidden="true">
            <svg aria-hidden="true" focusable="false" width="16" height="16" viewBox="0 0 24 24">
              <use href={`#${contentTypeIconId(
                option.kind === "all" ? null : option.contentType,
              )}`} />
            </svg>
          </span>
          <span class="option-label">{optionLabel(option)}</span>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .content-type-filter {
    position: relative;
    display: inline-flex;
    align-items: center;
  }

  .trigger {
    width: 100%;
    min-width: 0;
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.45rem 0.65rem;
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-sm, 6px);
    background: var(--cv-bg-elevated, #161b22);
    color: var(--cv-fg, #f0f4f8);
    font: inherit;
    font-size: var(--cv-control, 0.85rem);
    cursor: pointer;
  }

  .trigger:hover {
    background: var(--cv-bg-hover, rgba(255, 255, 255, 0.06));
  }

  .trigger[data-filter-active="true"],
  .trigger[data-filter-active="true"]:hover {
    background: var(--filter-active-background);
    color: #fff;
    border-color: rgba(255, 255, 255, 0.34);
  }

  .trigger[data-filter-active="true"] .trigger-icon {
    color: #fff;
  }

  .trigger:focus-visible {
    outline: 2px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.45));
    outline-offset: 2px;
  }

  .trigger[aria-expanded="true"] {
    border-color: var(--cv-accent, #2563eb);
    box-shadow: 0 0 0 1px var(--cv-accent, #2563eb);
  }

  .trigger-icon,
  .option-icon {
    display: inline-flex;
    flex: 0 0 1.1rem;
    width: 1.1rem;
    height: 1.1rem;
    align-items: center;
    justify-content: center;
    color: var(--cv-fg-muted, #94a3b8);
  }

  .trigger-label,
  .option-label {
    flex: 1 1 auto;
    overflow: hidden;
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .listbox {
    position: fixed;
    top: 0;
    left: 0;
    right: auto;
    z-index: 30;
    display: flex;
    max-height: 18rem;
    flex-direction: column;
    gap: 0.1rem;
    overflow-y: auto;
    margin: 0;
    padding: 0.35rem;
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 10px);
    background: var(--cv-bg-elevated, #161b22);
    box-shadow: 0 14px 36px rgba(0, 0, 0, 0.34);
    list-style: none;
  }

  .listbox :global(li) {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.4rem 0.6rem;
    border-radius: var(--cv-radius-sm, 6px);
    color: var(--cv-fg, #f0f4f8);
    font-size: var(--cv-control, 0.85rem);
    cursor: pointer;
  }

  .listbox :global(li.active) {
    background: var(--cv-bg-hover, rgba(255, 255, 255, 0.08));
  }

  .listbox :global(li.selected) {
    color: var(--cv-accent, #2563eb);
  }
</style>
