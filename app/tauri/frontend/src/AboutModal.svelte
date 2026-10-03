<script lang="ts">
  /**
   * Modal exposing the canonical product identity for ClipVault.
   *
   * The global toolbar and native tray share this modal. Other
   * surfaces (the HistoryCard rail menu and Quick Paste) MUST NOT
   * surface a parallel "Acerca de" entry. The user reads the version
   * reported by the canonical Tauri command (`clipvault_diagnostics`),
   * never a hard-coded constant that the maintainer forgot to bump.
   *
   * The modal is metadata-only: it never echoes clipboard content,
   * source identifiers, asset paths or hashes. Closing the dialog
   * with Escape, the backdrop click or the explicit close button all
   * route through the shared `Modal` shell so the focus and keydown
   * listeners stay symmetric with every other modal the desktop
   * opens.
   */
  import type { Diagnostics } from "./types";
  import { t } from "./lib/localization.ts";
  import {
    checkForUpdates,
    installAvailableUpdate,
    isApplicationUpdaterEnabled,
    restartAfterUpdate,
    updateState,
  } from "./lib/applicationUpdates";

  export let diagnostics: Diagnostics | null = null;
  export let onClose: () => void;

  /**
   * Canonical version string. Always read from the diagnostics
   * payload (`Cargo.toml`'s `[workspace.package].version`, surfaced
   * through the `clipvault_diagnostics` Tauri command) — never from
   * a hard-coded Svelte constant. The accessor falls back to a
   * placeholder so the modal stays usable while the bootstrap call
   * has not resolved yet; the moment `diagnostics` populates the
   * real version replaces the placeholder so the modal never shows
   * a stale value.
   */
  $: displayVersion = (() => {
    const raw = diagnostics?.version?.trim();
    if (raw && raw.length > 0) return `v${raw}`;
    return "v—";
  })();
  $: updaterEnabled = isApplicationUpdaterEnabled();

  function runUpdateAction(): void {
    if ($updateState.status === "available") {
      void installAvailableUpdate();
    } else if ($updateState.status === "restart_required" ||
      ($updateState.status === "error" && $updateState.errorPhase === "restart")) {
      void restartAfterUpdate();
    } else {
      void checkForUpdates();
    }
  }
</script>

<section class="about-section" data-testid="about-modal">
  <article class="card-block">
    <h3 class="block-title">ClipVault</h3>
    <p class="muted">{$t("about.description")}</p>
    <dl class="meta-list">
      <dt>{$t("about.product")}</dt>
      <dd>ClipVault</dd>
      <dt>{$t("about.version")}</dt>
      <dd>
        <code data-testid="about-version">{displayVersion}</code>
      </dd>
    </dl>
  </article>
  <section class="update-block" data-testid="about-update" aria-live="polite">
    <p class="muted update-status" role="status" data-testid="about-update-status">
      {#if !updaterEnabled}
        {$t("about.update.unavailable")}
      {:else if $updateState.status === "checking"}
        {$t("about.update.checking")}
      {:else if $updateState.status === "current"}
        {$t("about.update.current")}
      {:else if $updateState.status === "available"}
        {$t("about.update.available", { version: $updateState.availableVersion ?? "" })}
      {:else if $updateState.status === "installing" && $updateState.progressPercent !== null}
        {$t("about.update.progress", { progress: $updateState.progressPercent })}
      {:else if $updateState.status === "installing"}
        {$t("about.update.installing")}
      {:else if $updateState.status === "restart_required"}
        {$t("about.update.restart_required", { version: $updateState.availableVersion ?? "" })}
      {:else if $updateState.status === "error" && $updateState.errorPhase === "restart"}
        {$t("about.update.restart_error")}
      {:else if $updateState.status === "error"}
        {$t("about.update.error")}
      {:else}
        {$t("about.update.idle")}
      {/if}
    </p>
    <div class="row" data-testid="about-update-actions">
      {#if !updaterEnabled}
        <button type="button" class="update-action" disabled>
          {$t("about.update.check")}
        </button>
      {:else if $updateState.status === "checking" || $updateState.status === "installing"}
        <button type="button" class="update-action" disabled>
          {$updateState.status === "checking"
            ? $t("about.update.checking_action")
            : $t("about.update.installing_action")}
        </button>
      {:else if $updateState.status === "available"}
        <button type="button" class="update-action" on:click={runUpdateAction}>
          {$t("about.update.install")}
        </button>
      {:else if $updateState.status === "restart_required" ||
        ($updateState.status === "error" && $updateState.errorPhase === "restart")}
        <button type="button" class="update-action" on:click={runUpdateAction}>
          {$t("about.update.restart")}
        </button>
      {:else}
        <button type="button" class="update-action" on:click={runUpdateAction}>
          {$updateState.status === "error"
            ? $t("about.update.retry")
            : $t("about.update.check")}
        </button>
      {/if}
    </div>
  </section>
  <div class="row" data-testid="about-actions">
    <button
      type="button"
      class="close"
      data-testid="about-close"
      on:click={onClose}
    >
      {$t("common.close")}
    </button>
  </div>
</section>

<style>
  .about-section {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .card-block {
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
  }

  .block-title {
    margin: 0;
    font-size: var(--cv-title-md, 1rem);
    font-weight: 600;
  }

  .muted {
    color: var(--cv-fg-muted, #94a3b8);
  }

  .meta-list {
    display: grid;
    grid-template-columns: max-content 1fr;
    column-gap: 0.85rem;
    row-gap: 0.25rem;
    margin: 0;
  }

  .meta-list dt {
    color: var(--cv-fg-muted, #94a3b8);
    font-weight: 500;
  }

  .meta-list dd {
    margin: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    word-break: break-all;
  }

  .row {
    display: flex;
    gap: 0.5rem;
    justify-content: flex-end;
  }

  .update-block {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding-top: 0.75rem;
    border-top: 1px solid var(--cv-border, #30363d);
  }

  .update-status {
    margin: 0;
  }

  button.update-action {
    background: var(--cv-surface-raised, #30363d);
    color: var(--cv-fg, #e6edf3);
    border: 1px solid var(--cv-border, #30363d);
    padding: 0.45rem 0.85rem;
    border-radius: var(--cv-radius-sm, 6px);
    cursor: pointer;
    font: inherit;
  }

  button.update-action:hover:not(:disabled) {
    background: var(--cv-surface-hover, #3b4654);
  }

  button.update-action:disabled {
    opacity: 0.7;
    cursor: wait;
  }

  button.close {
    background: var(--cv-accent, #2563eb);
    color: white;
    border: 0;
    padding: 0.45rem 0.85rem;
    border-radius: var(--cv-radius-sm, 6px);
    cursor: pointer;
    font: inherit;
  }

  button.close:hover {
    background: var(--cv-accent-hover, #1d4ed8);
  }

  kbd {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    background: rgba(148, 163, 184, 0.18);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: 4px;
    padding: 0 0.35rem;
  }
</style>
