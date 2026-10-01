<script lang="ts">
  /**
   * Modal hosting the GNOME Shell integration consent flow. The
   * surface is opt-in: it never appears on macOS, X11 or non-GNOME
   * Wayland sessions. The bridge is metadata-only; the panel never
   * reflects clipboard content or environment variables.
   */
  import { createEventDispatcher } from "svelte";
  import {
    gnomeIntegrationInstallCommand,
    gnomeIntegrationRetryCommand,
    gnomeIntegrationSetConsentCommand,
    gnomeIntegrationStatusCommand,
    gnomeIntegrationUninstallCommand,
  } from "./lib/tauri";
  import { describeGnomeIntegrationError } from "./lib/gnomeIntegrationError";
  import { t } from "./lib/localization";
  import type {
    GnomeConsentDecision,
    GnomeIntegrationPayload,
    GnomeIntegrationStatusResponse,
  } from "./types";

  export let initial: GnomeIntegrationStatusResponse | null = null;

  let status: GnomeIntegrationStatusResponse | null = initial;
  let busy = false;
  let lastError: string | null = null;
  let lastAction: "accept" | "decline" | null = null;

  const dispatch = createEventDispatcher<{
    statusChanged: GnomeIntegrationStatusResponse;
    installStarted: void;
    deactivated: void;
  }>();

  async function refresh(): Promise<void> {
    lastError = null;
    try {
      status = await gnomeIntegrationStatusCommand();
      if (status) {
        dispatch("statusChanged", status);
      }
    } catch (err) {
      lastError = describeGnomeIntegrationError(err);
    }
  }

  async function applyConsent(decision: GnomeConsentDecision): Promise<void> {
    if (busy) return;
    busy = true;
    lastError = null;
    lastAction = decision === "accepted" ? "accept" : "decline";
    try {
      const payload: GnomeIntegrationPayload = await gnomeIntegrationSetConsentCommand({
        decision,
      });
      status = { kind: "ready", payload };
      dispatch("statusChanged", status);
    } catch (err) {
      lastError = describeGnomeIntegrationError(err);
    } finally {
      busy = false;
    }
  }

  async function activate(): Promise<void> {
    if (busy) return;
    busy = true;
    lastError = null;
    try {
      dispatch("installStarted");
      await gnomeIntegrationInstallCommand();
      await refresh();
    } catch (err) {
      lastError = describeGnomeIntegrationError(err);
    } finally {
      busy = false;
    }
  }

  async function deactivate(): Promise<void> {
    if (busy) return;
    busy = true;
    lastError = null;
    try {
      await gnomeIntegrationUninstallCommand();
      dispatch("deactivated");
      await refresh();
    } catch (err) {
      lastError = describeGnomeIntegrationError(err);
    } finally {
      busy = false;
    }
  }

  async function retry(): Promise<void> {
    if (busy) return;
    busy = true;
    lastError = null;
    try {
      await gnomeIntegrationRetryCommand();
      await refresh();
    } catch (err) {
      lastError = describeGnomeIntegrationError(err);
    } finally {
      busy = false;
    }
  }

  $: shouldShowConsent =
    status?.kind === "ready" &&
    status.payload.consent === "unknown" &&
    status.payload.applicable;
  $: shouldShowDeclined =
    status?.kind === "ready" && status.payload.consent === "declined";
  $: isInstalled = status?.kind === "ready" && status.payload.installed;
  $: technicalState = status?.kind === "ready" ? status.payload.technical_state : null;
  $: identifier = status?.kind === "ready" ? status.payload.identifier : null;
</script>

<section
  class="gnome-modal"
  data-testid="gnome-integration-modal"
  aria-label={$t("gnome.title")}
>
  <header>
    <h2>{$t("gnome.title")}</h2>
    <p class="muted">
      {$t("gnome.description")}
    </p>
  </header>

  {#if status?.kind === "not_applicable"}
    <article class="card-block">
      <p class="muted">
        {$t("gnome.not_applicable", { session: status.session, desktop: status.desktop })}
      </p>
    </article>
  {:else if status?.kind === "not_configured"}
    <article class="card-block">
      <p class="muted">
        {$t("gnome.not_available_build", { reason: status.reason })}
      </p>
    </article>
  {:else if status?.kind === "ready" && status.payload.applicable}
<article class="card-block">
    <h3>{$t("gnome.status.title")}</h3>
    <dl class="diag-list">
      <dt>{$t("gnome.status.session")}</dt>
      <dd><code>{status.payload.session}</code></dd>
      <dt>{$t("gnome.status.backend")}</dt>
      <dd><code>{status.payload.backend}</code></dd>
      <dt>{$t("gnome.status.protocol_version")}</dt>
      <dd><code>{status.payload.protocol_version}</code></dd>
      <dt>{$t("gnome.status.technical_state")}</dt>
      <dd><code>{technicalState ?? $t("common.unknown")}</code></dd>
      <dt>{$t("gnome.status.published_identifier")}</dt>
      <dd><code>{identifier ?? $t("common.none")}</code></dd>
      {#if status.payload.detail}
        <dt>{$t("gnome.status.detail")}</dt>
        <dd><code>{status.payload.detail}</code></dd>
      {/if}
    </dl>
    {#if technicalState === "activation_pending"}
      <p class="muted">
        {$t("gnome.status.activation_pending")}
      </p>
    {:else if technicalState === "connected"}
      <p class="muted">
        {$t("gnome.status.connected")}
      </p>
    {:else if technicalState === "identified" || technicalState === "no_active_application"}
      <p class="muted">
        {$t("gnome.status.identified")}
      </p>
    {:else if technicalState === "disconnected" || technicalState === "communication_error"}
      <p class="muted">
        {$t("gnome.status.disconnected")}
      </p>
    {:else if technicalState === "incompatible"}
      <p class="muted">
        {$t("gnome.status.incompatible")}
      </p>
    {/if}
  </article>

    {#if shouldShowConsent}
      <article class="card-block" data-testid="gnome-consent-card">
        <h3>{$t("gnome.consent.title")}</h3>
        <p>
          {$t("gnome.consent.description")}
        </p>
        <ul>
          <li>{$t("gnome.consent.only_identifier")}</li>
          <li>{$t("gnome.consent.no_content")}</li>
          <li>{$t("gnome.consent.persisted")}</li>
          <li>{$t("gnome.consent.reversible")}</li>
        </ul>
        <div class="row">
          <button
            type="button"
            class="primary"
            on:click={() => applyConsent("accepted")}
            disabled={busy}
            data-testid="gnome-consent-accept"
          >
            {busy && lastAction === "accept" ? $t("gnome.processing") : $t("gnome.consent.activate")}
          </button>
          <button
            type="button"
            class="secondary"
            on:click={() => applyConsent("declined")}
            disabled={busy}
            data-testid="gnome-consent-decline"
          >
            {busy && lastAction === "decline" ? $t("gnome.saving") : $t("gnome.consent.not_now")}
          </button>
        </div>
      </article>
    {:else if status.payload.consent === "accepted"}
      <article class="card-block" data-testid="gnome-install-card">
        <h3>{$t("gnome.install.title")}</h3>
        <p class="muted">
          {$t("gnome.install.description")}
        </p>
        <div class="row">
          {#if !isInstalled}
            <button
              type="button"
              class="primary"
              on:click={activate}
              disabled={busy}
              data-testid="gnome-install-action"
            >
              {busy ? $t("gnome.install.installing") : $t("gnome.install.action")}
            </button>
          {:else}
            <button
              type="button"
              class="primary"
              on:click={activate}
              disabled={busy}
              data-testid="gnome-reinstall-action"
            >
              {busy ? $t("gnome.install.reinstalling") : $t("gnome.install.reinstall")}
            </button>
            <button
              type="button"
              class="secondary"
              on:click={retry}
              disabled={busy}
              data-testid="gnome-retry"
            >
              {busy ? $t("gnome.retrying") : $t("gnome.retry")}
            </button>
            <button
              type="button"
              class="danger"
              on:click={deactivate}
              disabled={busy}
              data-testid="gnome-uninstall"
            >
              {busy ? $t("gnome.uninstalling") : $t("gnome.disable")}
            </button>
          {/if}
        </div>
      </article>
    {:else if status.payload.consent === "disabled"}
      <article class="card-block">
        <h3>{$t("gnome.disabled.title")}</h3>
        <p class="muted">
          {$t("gnome.disabled.description")}
        </p>
        <div class="row">
          <button
            type="button"
            class="primary"
            on:click={() => applyConsent("accepted")}
            disabled={busy}
          >
            {$t("gnome.reactivate")}
          </button>
        </div>
      </article>
    {/if}

    {#if shouldShowDeclined}
      <article class="card-block" data-testid="gnome-declined-card">
        <p class="muted">{$t("gnome.declined.description")}</p>
        <div class="row">
          <button
            type="button"
            class="secondary"
            on:click={() => applyConsent("accepted")}
            disabled={busy}
          >
            {$t("gnome.declined.change_mind")}
          </button>
        </div>
      </article>
    {/if}

    {#if lastError}
      <p class="status error" role="alert" data-testid="gnome-error">{$t(lastError)}</p>
    {/if}
  {:else}
    <article class="card-block">
      <p class="muted">{$t("gnome.loading")}</p>
      <div class="row">
        <button type="button" on:click={refresh} data-testid="gnome-refresh">{$t("gnome.retry")}</button>
      </div>
    </article>
  {/if}
</section>

<style>
  .gnome-modal {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }

  header h2 {
    margin: 0 0 0.35rem 0;
    font-size: 1.05rem;
  }

  header p {
    margin: 0;
  }

  .card-block {
    background: var(--cv-bg-surface, #0e1116);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 10px);
    padding: 0.85rem 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .card-block h3 {
    margin: 0;
    font-size: 0.95rem;
  }

  ul {
    margin: 0;
    padding-left: 1.1rem;
  }

  .diag-list {
    display: grid;
    grid-template-columns: max-content 1fr;
    column-gap: 0.85rem;
    row-gap: 0.25rem;
    margin: 0;
  }

  .diag-list dt {
    color: var(--cv-fg-muted, #94a3b8);
    font-weight: 500;
  }

  .diag-list dd {
    margin: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    word-break: break-all;
  }

  .row {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    flex-wrap: wrap;
  }

  .muted {
    color: var(--cv-fg-muted, #94a3b8);
  }

  .status.error {
    color: var(--cv-fg-error, #f87171);
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

  button.secondary {
    background: var(--cv-bg-elevated, #1e2530);
  }

  button.danger {
    background: var(--cv-fg-error, #f87171);
  }

  button:disabled {
    background: #374151;
    color: #94a3b8;
    cursor: not-allowed;
  }
</style>
