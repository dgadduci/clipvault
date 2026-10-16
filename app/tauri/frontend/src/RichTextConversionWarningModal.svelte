<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import Modal from "./Modal.svelte";
  import { t } from "./lib/localization.ts";

  export let open: boolean;
  export let entryId: number;
  export let displayTitle: string;
  export let returnFocusTo: HTMLElement | null = null;

  const dispatch = createEventDispatcher<{
    close: void;
    confirm: void;
  }>();

  $: titleId = `rich-text-conversion-title-${entryId}`;
  $: descriptionId = `rich-text-conversion-warning-${entryId}`;

  function close(): void {
    dispatch("close");
  }

  function confirm(): void {
    dispatch("confirm");
  }
</script>

<Modal
  {open}
  {titleId}
  descriptionId={descriptionId}
  title={$t("text_capture.edit.rich_conversion.title")}
  {returnFocusTo}
  onClose={close}
>
  <div
    class="rich-text-conversion-warning"
    data-testid="rich-text-conversion-warning-dialog"
    data-entry-id={entryId}
  >
    <p id={descriptionId} data-testid="rich-text-conversion-warning-message">
      {$t("text_capture.edit.rich_conversion.warning", { title: displayTitle })}
    </p>
    <div class="row">
      <button
        type="button"
        class="secondary"
        data-testid="rich-text-conversion-cancel"
        on:click={close}
      >
        {$t("common.cancel")}
      </button>
      <button
        type="button"
        class="primary"
        data-testid="rich-text-conversion-confirm"
        on:click={confirm}
      >
        {$t("text_capture.edit.rich_conversion.continue")}
      </button>
    </div>
  </div>
</Modal>

<style>
  .rich-text-conversion-warning p {
    margin: 0;
    line-height: 1.5;
    color: var(--cv-fg, #f0f4f8);
  }

  .row {
    display: flex;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: 0.65rem;
  }

  .row button {
    min-height: 2.4rem;
    padding: 0.5rem 0.85rem;
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-sm, 6px);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .secondary {
    color: var(--cv-fg, #f0f4f8);
    background: var(--cv-bg, #0d1117);
  }

  .primary {
    color: var(--cv-button-primary-fg, #fff);
    background: var(--cv-accent, #2563eb);
    border-color: var(--cv-accent, #2563eb) !important;
  }

  .row button:focus-visible {
    outline: 2px solid var(--cv-focus-ring, #60a5fa);
    outline-offset: 2px;
  }

  @media (max-width: 420px) {
    .row {
      flex-direction: column-reverse;
    }

    .row button {
      width: 100%;
    }
  }
</style>
