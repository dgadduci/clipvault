<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import type { ImageImportFilePreview } from "./types";
  import Modal from "./Modal.svelte";
  import { t } from "./lib/localization.ts";

  export let open = false;
  export let files: ImageImportFilePreview[] = [];
  export let orderedFileIds: number[] = [];
  export let busy = false;
  export let progressPercent = 0;

  const dispatch = createEventDispatcher<{
    reorder: { orderedFileIds: number[] };
    confirm: { orderedFileIds: number[] };
    cancel: void;
  }>();

  const titleId = "image-import-order-title";
  const descriptionId = "image-import-order-description";

  $: filesById = new Map(files.map((file) => [file.id, file]));
  $: orderedFiles = orderedFileIds
    .map((id) => filesById.get(id))
    .filter((file): file is ImageImportFilePreview => file !== undefined);

  function displayName(file: ImageImportFilePreview): string {
    const name = file.display_name ?? $t("common.unknown");
    const normalizedName = name.toLowerCase();
    const sameNameFiles = files.filter(
      (candidate) =>
        (candidate.display_name ?? $t("common.unknown")).toLowerCase() ===
        normalizedName,
    );
    return sameNameFiles.length > 1
      ? `${name} · ${files.findIndex((candidate) => candidate.id === file.id) + 1}`
      : name;
  }

  function moveFile(index: number, offset: -1 | 1): void {
    const targetIndex = index + offset;
    if (busy || targetIndex < 0 || targetIndex >= orderedFileIds.length) return;
    const next = [...orderedFileIds];
    [next[index], next[targetIndex]] = [next[targetIndex], next[index]];
    dispatch("reorder", { orderedFileIds: next });
  }

  function cancel(): void {
    if (!busy) dispatch("cancel");
  }

  function confirm(): void {
    if (!busy && orderedFileIds.length === files.length && files.length > 0) {
      dispatch("confirm", { orderedFileIds: [...orderedFileIds] });
    }
  }
</script>

<Modal
  {open}
  {titleId}
  {descriptionId}
  title={$t("image_import.order.title")}
  {busy}
  onClose={cancel}
>
  <div class="image-import-order" data-testid="image-import-order-modal">
    <p id={descriptionId} class="image-import-order-description">
      {$t("image_import.order.description", { count: files.length })}
    </p>

    <ol class="image-import-order-list" aria-label={$t("image_import.order.list")}>
      {#each orderedFiles as file, index (file.id)}
        <li class="image-import-order-item" data-file-id={file.id}>
          <span class="image-import-order-number" aria-hidden="true">{index + 1}</span>
          <span class="image-import-order-name" title={displayName(file)}>
            {displayName(file)}
          </span>
          <div class="image-import-order-controls">
            <button
              type="button"
              aria-label={$t("image_import.order.move_up", {
                name: displayName(file),
              })}
              title={$t("image_import.order.move_up_short")}
              disabled={busy || index === 0}
              on:click={() => moveFile(index, -1)}
            >↑</button>
            <button
              type="button"
              aria-label={$t("image_import.order.move_down", {
                name: displayName(file),
              })}
              title={$t("image_import.order.move_down_short")}
              disabled={busy || index === orderedFiles.length - 1}
              on:click={() => moveFile(index, 1)}
            >↓</button>
          </div>
        </li>
      {/each}
    </ol>

    <div class="image-import-order-actions">
      {#if busy}
        <div class="image-import-progress">
          <div
            class="image-import-progress-track"
            role="progressbar"
            aria-valuemin="0"
            aria-valuemax="100"
            aria-valuenow={progressPercent}
            aria-valuetext={$t("image_import.order.progress", { percent: progressPercent })}
            aria-label={$t("image_import.order.progress", { percent: progressPercent })}
            data-testid="image-import-progress"
          >
            <span
              class="image-import-progress-fill"
              style={`width: ${progressPercent}%;`}
            ></span>
          </div>
          <span class="image-import-progress-value" aria-hidden="true">{progressPercent}%</span>
        </div>
      {:else}
        <span class="image-import-progress-placeholder" aria-hidden="true"></span>
      {/if}
      <div class="image-import-order-buttons">
        <button type="button" class="secondary" disabled={busy} on:click={cancel}>
          {$t("common.cancel")}
        </button>
        <button
          type="button"
          class="primary"
          disabled={busy || files.length === 0 || orderedFiles.length !== files.length}
          on:click={confirm}
          data-testid="image-import-order-confirm"
        >
          {busy ? $t("image_import.order.importing") : $t("image_import.order.confirm")}
        </button>
      </div>
    </div>
  </div>
</Modal>

<style>
  .image-import-order {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .image-import-order-description {
    margin: 0;
    color: var(--cv-fg-muted, #a7b0bc);
  }

  .image-import-order-list {
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    max-height: min(52vh, 28rem);
    overflow-y: auto;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .image-import-order-item {
    display: grid;
    grid-template-columns: 2rem minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.65rem;
    min-height: 2.7rem;
    padding: 0.35rem 0.5rem;
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-sm, 8px);
    background: var(--cv-bg, #0d1117);
  }

  .image-import-order-number {
    color: var(--cv-fg-muted, #a7b0bc);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .image-import-order-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .image-import-order-controls,
  .image-import-order-actions,
  .image-import-order-buttons {
    display: flex;
    align-items: center;
    gap: 0.45rem;
  }

  .image-import-order-controls button,
  .image-import-order-actions button {
    min-width: 2.25rem;
    min-height: 2.25rem;
    color: inherit;
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-sm, 8px);
    background: var(--cv-bg-elevated, #161b22);
    cursor: pointer;
  }

  .image-import-order-controls button:disabled,
  .image-import-order-actions button:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .image-import-order-actions {
    justify-content: space-between;
    padding-top: 0.25rem;
  }

  .image-import-progress,
  .image-import-progress-placeholder {
    width: min(10rem, 32%);
    min-width: 5rem;
  }

  .image-import-progress {
    display: flex;
    align-items: center;
    gap: 0.55rem;
  }

  .image-import-progress-track {
    flex: 1 1 auto;
    height: 0.5rem;
    overflow: hidden;
    border: 1px solid var(--cv-border, #30363d);
    border-radius: 999px;
    background: var(--cv-bg, #0d1117);
  }

  .image-import-progress-fill {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--cv-accent, #2563eb);
    transition: width 120ms linear;
  }

  .image-import-progress-value {
    min-width: 2.4rem;
    color: var(--cv-fg-muted, #a7b0bc);
    font-size: var(--cv-tag, 0.65rem);
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  .image-import-order-actions .primary {
    color: var(--cv-fg-on-accent, #fff);
    border-color: var(--cv-accent, #2563eb);
    background: var(--cv-accent, #2563eb);
  }

  .image-import-order-actions .secondary {
    background: transparent;
  }

  @media (max-width: 480px) {
    .image-import-progress,
    .image-import-progress-placeholder {
      width: 5rem;
      min-width: 4.5rem;
    }

    .image-import-progress {
      gap: 0.35rem;
    }

    .image-import-order-actions {
      gap: 0.35rem;
    }

    .image-import-order-buttons {
      gap: 0.35rem;
    }
  }
</style>
