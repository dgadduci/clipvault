<script lang="ts">
  import { ACTIVE_FILTER_SURFACES, compactFilterComboboxStyles } from "./lib/filterTokens.ts";
  import { t } from "./lib/localization.ts";
  import type { Collection, CollectionFilter as CollectionFilterValue } from "./types.ts";

  export let selected: CollectionFilterValue = { kind: "all" };
  export let options: Collection[] = [];
  export let ariaLabel: string = "";
  export let testId: string = "collection-filter";
  export let disabled: boolean = false;
  export let onChange: (next: CollectionFilterValue) => void = () => {};

  $: selectedValue = selected.kind === "all"
    ? "all"
    : selected.kind === "unassigned"
      ? "unassigned"
      : String(selected.collectionId);
  $: selectedTitle = selected.kind === "all"
    ? $t("filter.all")
    : selected.kind === "unassigned"
      ? $t("filter.collection_unassigned")
      : options.find((collection) => collection.id === selected.collectionId)?.name ??
        $t("filter.collection");

  function handleChange(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value;
    if (value === "all") {
      onChange({ kind: "all" });
      return;
    }
    if (value === "unassigned") {
      onChange({ kind: "unassigned" });
      return;
    }
    const collectionId = Number(value);
    if (!Number.isSafeInteger(collectionId) || collectionId <= 0) return;
    onChange({ kind: "collection", collectionId });
  }
</script>

<div
  class="collection-filter"
  data-testid={testId}
  data-filter-active={selected.kind !== "all"}
  data-filter-disabled={disabled}
  data-filter-width="110"
  data-filter-min-width="5.5rem"
  style={`${compactFilterComboboxStyles} --filter-active-background: ${ACTIVE_FILTER_SURFACES.collection};`}
>
  <svg
    class="collection-icon"
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
    <rect x="4" y="4" width="16" height="6" rx="1.5" />
    <rect x="4" y="14" width="16" height="6" rx="1.5" />
  </svg>
  <select
    value={selectedValue}
    aria-label={ariaLabel || $t("filter.collection")}
    title={selectedTitle}
    data-testid={`${testId}-select`}
    data-collection-filter-selected={selected.kind}
    data-filter-active={selected.kind !== "all"}
    disabled={disabled}
    on:change={handleChange}
  >
    <option value="all" title={$t("filter.all")}>{$t("filter.all")}</option>
    <option value="unassigned" title={$t("filter.collection_unassigned")}>{$t("filter.collection_unassigned")}</option>
    {#each options.filter((collection) => collection.kind === "user") as collection (collection.id)}
      <option value={String(collection.id)} title={collection.name}>{collection.name}</option>
    {/each}
  </select>
  <svg
    class="chevron"
    aria-hidden="true"
    focusable="false"
    width="12"
    height="12"
    viewBox="0 0 12 12"
    fill="currentColor"
  >
    <path d="M3 5l3 3 3-3" />
  </svg>
</div>

<style>
  .collection-filter {
    position: relative;
    display: inline-flex;
    align-items: center;
    flex: 0 0 110px;
    min-width: 5.5rem;
    max-width: 110px;
    width: 110px;
  }

  .collection-filter[data-filter-disabled="true"] {
    opacity: 0.56;
  }

  select {
    appearance: none;
    color-scheme: dark;
    box-sizing: border-box;
    width: 100%;
    min-width: 0;
    padding: 0.45rem 2rem 0.45rem 2rem;
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-sm, 6px);
    background: var(--cv-bg-elevated, #161b22);
    color: #fff;
    font: inherit;
    font-size: var(--cv-control, 0.85rem);
    text-overflow: ellipsis;
    cursor: pointer;
  }

  select:hover {
    background: var(--cv-bg-hover, rgba(255, 255, 255, 0.06));
  }

  select:disabled {
    cursor: not-allowed;
  }

  .collection-filter[data-filter-active="true"] select,
  .collection-filter[data-filter-active="true"] select:hover {
    background: var(--filter-active-background);
    color: #fff;
    border-color: rgba(255, 255, 255, 0.34);
  }

  .collection-filter[data-filter-active="true"] .collection-icon,
  .collection-filter[data-filter-active="true"] .chevron {
    color: #fff;
  }

  select:focus-visible {
    outline: 2px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.45));
    outline-offset: 2px;
  }

  select option {
    background-color: #161b22;
    color: #fff;
    color-scheme: dark;
  }

  .collection-icon,
  .chevron {
    position: absolute;
    pointer-events: none;
    color: var(--cv-muted, #8b949e);
  }

  .collection-icon {
    left: 0.65rem;
  }

  .chevron {
    right: 0.65rem;
  }
</style>
