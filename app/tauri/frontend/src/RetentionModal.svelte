<script lang="ts">
  /**
   * Modal hosting the retention-period controls. The modal owns the
   * retention selector, **Preview retention** and **Apply retention
   * now**; the destructive **Clear non-favorite history** button is
   * NOT rendered here (it lives behind the trash icon in the
   * desktop toolbar).
   *
   * `Preview retention` MUST remain a read-only operation: it never
   * mutates history. `Apply retention now` runs the existing
   * `clipvault_apply_retention` command through the same
   * destructive-action helper the inline section already used so a
   * backend rejection surfaces the same `confirmation_required`
   * outcome the rest of the app already understands.
   */
  import {
    applyRetentionCommand,
    retentionPreviewCommand,
    settingsGetCommand,
    settingsSetCommand,
  } from "./lib/tauri";
  import {
    describeRetentionPreview,
    describeRetentionResult,
  } from "./lib/management";
  import { onMount } from "svelte";
  import type { RetentionPolicy, Settings } from "./types";

  export let onSettingsChanged: (settings: Settings) => void = () => {};

  const retentionChoices: { value: RetentionPolicy; label: string }[] = [
    { value: "forever", label: "Conservar siempre" },
    { value: "days_7", label: "7 días" },
    { value: "days_30", label: "30 días" },
    { value: "days_90", label: "90 días" },
  ];

  let settings: Settings | null = null;
  let loading = true;
  let busy = false;
  let policyBusy: RetentionPolicy | null = null;
  let retentionMessage: string | null = null;
  let retentionError: string | null = null;
  let persistenceError: string | null = null;

  async function loadSettings(): Promise<void> {
    loading = true;
    try {
      settings = await settingsGetCommand();
    } catch (error) {
      persistenceError = error instanceof Error ? error.message : String(error);
    } finally {
      loading = false;
    }
  }

  async function applyRetentionPolicy(policy: RetentionPolicy): Promise<void> {
    if (!settings) return;
    retentionMessage = null;
    retentionError = null;
    policyBusy = policy;
    try {
      const next = await settingsSetCommand({ retention: policy });
      settings = next;
      onSettingsChanged(next);
      retentionMessage = `Política de retención actualizada: ${describeRetention(policy)}`;
    } catch (error) {
      persistenceError = error instanceof Error ? error.message : String(error);
    } finally {
      policyBusy = null;
    }
  }

  async function previewRetention(): Promise<void> {
    retentionMessage = null;
    retentionError = null;
    busy = true;
    try {
      const preview = await retentionPreviewCommand();
      retentionMessage = describeRetentionPreview(preview);
    } catch (error) {
      retentionError = error instanceof Error ? error.message : String(error);
    } finally {
      busy = false;
    }
  }

  async function runApplyRetention(): Promise<void> {
    retentionMessage = null;
    retentionError = null;
    busy = true;
    try {
      const result = await applyRetentionCommand();
      retentionMessage = describeRetentionResult(result);
    } catch (error) {
      retentionError = error instanceof Error ? error.message : String(error);
    } finally {
      busy = false;
    }
  }

  function describeRetention(policy: RetentionPolicy): string {
    const entry = retentionChoices.find((c) => c.value === policy);
    return entry ? entry.label : policy;
  }

  onMount(() => {
    void loadSettings();
  });
</script>

<section class="retention" data-testid="retention-modal">
  <article>
    <h3>Política de retención</h3>
    <p class="muted">
      Las entradas favoritas nunca se eliminan automáticamente.
    </p>
    {#if loading}
      <p class="muted">Cargando ajustes…</p>
    {:else if persistenceError}
      <p class="error" role="alert" data-testid="retention-load-error">
        {persistenceError}
      </p>
    {:else if settings}
      <div class="row" role="radiogroup" aria-label="Política de retención">
        {#each retentionChoices as choice (choice.value)}
          <label class="radio">
            <input
              type="radio"
              name="retention-policy"
              value={choice.value}
              checked={settings.retention === choice.value}
              on:change={() => applyRetentionPolicy(choice.value)}
              disabled={policyBusy !== null}
              data-testid="retention-radio-{choice.value}"
            />
            <span>{choice.label}</span>
          </label>
        {/each}
      </div>
    {/if}
  </article>

  <article>
    <h3>Acciones de retención</h3>
    <p class="muted">
      <strong>Preview retention</strong> calcula cuántas entradas no favoritas
      se eliminarían con la política actual sin tocar la base de datos.
      <strong>Apply retention now</strong> ejecuta la purga.
    </p>
    <div class="row">
      <button
        type="button"
        on:click={previewRetention}
        disabled={busy}
        data-testid="preview-retention"
      >
        {busy ? "Calculando…" : "Preview retention"}
      </button>
      <button
        type="button"
        on:click={runApplyRetention}
        disabled={busy}
        data-testid="apply-retention"
      >
        {busy ? "Aplicando…" : "Apply retention now"}
      </button>
    </div>
    {#if retentionMessage}
      <p class="muted" data-testid="retention-message">{retentionMessage}</p>
    {/if}
    {#if retentionError}
      <p
        class="error"
        role="alert"
        data-testid="retention-error"
      >
        {retentionError}
      </p>
    {/if}
  </article>
</section>

<style>
  .retention {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }
  .retention article {
    background: var(--cv-bg-surface, #0e1116);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 10px);
    padding: 0.85rem 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .retention h3 {
    margin: 0;
    font-size: var(--cv-title-sm, 0.95rem);
    font-weight: 600;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .radio {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }
  .muted {
    color: var(--cv-fg-muted, #94a3b8);
    margin: 0;
  }
  .error {
    color: var(--cv-fg-error, #f87171);
    margin: 0;
  }

  button {
    background: var(--cv-accent, #2563eb);
    color: white;
    border: 0;
    padding: 0.4rem 0.85rem;
    border-radius: var(--cv-radius-sm, 6px);
    cursor: pointer;
    font-size: 0.85rem;
  }

  button:disabled {
    background: #374151;
    color: #94a3b8;
    cursor: not-allowed;
  }

  button:hover:not(:disabled) {
    background: var(--cv-accent-hover, #1d4ed8);
  }
</style>