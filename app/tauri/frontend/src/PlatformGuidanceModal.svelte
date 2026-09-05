<script lang="ts">
  import type { PlatformGuidance } from "./types";
  import { openPlatformSettingsCommand, type SettingsOpenResponse } from "./lib/tauri";

  export let guidance: PlatformGuidance;
  export let retryNotice: string | null;
  export let retryError: string | null;
  export let onClose: () => void;
  export let onRetry: () => Promise<void> | void;

  let opening = false;
  let fallbackSteps: string[] | null = null;
  let openError: string | null = null;
  let retrying = false;
  let titleEl: HTMLHeadingElement | undefined;

  function close(): void {
    onClose();
  }

  async function openSettings(): Promise<void> {
    if (!guidance.settings_target) {
      return;
    }
    opening = true;
    fallbackSteps = null;
    openError = null;
    try {
      const result: SettingsOpenResponse = await openPlatformSettingsCommand({
        target: guidance.settings_target,
      });
      if (result.kind === "fallback_required") {
        fallbackSteps = result.manual_steps;
      } else if (result.kind === "failed") {
        openError = result.reason;
      }
    } catch (err) {
      openError = err instanceof Error ? err.message : String(err);
    } finally {
      opening = false;
    }
  }

  async function retry(): Promise<void> {
    retrying = true;
    try {
      await onRetry();
    } finally {
      retrying = false;
    }
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  }

  // Focus the title when the modal mounts.
  $: if (titleEl) {
    titleEl.focus();
  }
</script>

<svelte:window on:keydown={onKeydown} />

<div
  class="overlay"
  role="presentation"
  on:click|self={close}
>
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="guidance-title"
    aria-describedby="guidance-summary"
  >
    <h2 id="guidance-title" class="title" bind:this={titleEl} tabindex="-1">
      {guidance.title}
    </h2>
    <p id="guidance-summary" class="summary">{guidance.summary}</p>

    {#if retryNotice}
      <p class="status" role="status">{retryNotice}</p>
    {/if}

    {#if retryError}
      <p class="error" role="alert">
        Reintentar could not refresh the capabilities: {retryError}
      </p>
    {/if}

    <section aria-labelledby="guidance-steps-title">
      <h3 id="guidance-steps-title" class="section-title">Pasos a seguir</h3>
      <ol class="steps">
        {#each guidance.steps as step, index (index)}
          <li>{step}</li>
        {/each}
      </ol>
    </section>

    {#if fallbackSteps && fallbackSteps.length > 0}
      <section class="fallback" aria-live="polite">
        <h3 class="section-title">Pasos manuales</h3>
        <p class="muted">
          ClipVault no pudo abrir la configuración automáticamente. Sigue estos pasos:
        </p>
        <ol class="steps">
          {#each fallbackSteps as step, index (index)}
            <li>{step}</li>
          {/each}
        </ol>
      </section>
    {/if}

    {#if openError}
      <p class="error" role="alert">
        No se pudo abrir la configuración del sistema: {openError}
      </p>
    {/if}

    <div class="actions">
      {#if guidance.can_open_settings && guidance.settings_target}
        <button
          type="button"
          class="primary"
          on:click={openSettings}
          disabled={opening || retrying}
        >
          {opening ? "Abriendo…" : "Abrir configuración"}
        </button>
      {/if}
      {#if guidance.retryable}
        <button type="button" on:click={retry} disabled={retrying}>
          {retrying ? "Reintentando…" : "Reintentar"}
        </button>
      {/if}
      <button type="button" on:click={close}>Cerrar</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(8, 11, 16, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
    z-index: 1000;
  }

  .modal {
    background: #161b22;
    color: #f0f4f8;
    border: 1px solid #30363d;
    border-radius: 12px;
    padding: 1.5rem 1.75rem;
    max-width: 520px;
    width: 100%;
    max-height: 90vh;
    overflow-y: auto;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.45);
  }

  .title {
    margin: 0 0 0.5rem;
    font-size: 1.2rem;
    outline: none;
  }

  .summary {
    margin: 0 0 1rem;
    line-height: 1.5;
  }

  .section-title {
    margin: 1rem 0 0.5rem;
    font-size: 0.95rem;
    color: #94a3b8;
    font-weight: 600;
  }

  .steps {
    margin: 0 0 1rem 1.25rem;
    padding: 0;
    line-height: 1.5;
  }

  .steps li {
    margin-bottom: 0.4rem;
  }

  .fallback {
    background: #1f2937;
    border-radius: 8px;
    padding: 0.75rem 1rem;
    margin: 1rem 0;
  }

  .muted {
    color: #94a3b8;
  }

  .error {
    color: #f87171;
    background: rgba(248, 113, 113, 0.08);
    padding: 0.5rem 0.75rem;
    border-radius: 6px;
    margin: 0 0 1rem;
  }

  .status {
    background: rgba(56, 189, 248, 0.08);
    color: #7dd3fc;
    padding: 0.5rem 0.75rem;
    border-radius: 6px;
    margin: 0 0 1rem;
  }

  .actions {
    display: flex;
    gap: 0.5rem;
    justify-content: flex-end;
    flex-wrap: wrap;
    margin-top: 1rem;
  }

  button {
    background: #2563eb;
    color: white;
    border: 0;
    padding: 0.5rem 1rem;
    border-radius: 6px;
    cursor: pointer;
    font: inherit;
  }

  button.primary {
    background: #2563eb;
  }

  button:hover:not(:disabled) {
    background: #1d4ed8;
  }

  button:disabled {
    background: #374151;
    color: #94a3b8;
    cursor: not-allowed;
  }
</style>
