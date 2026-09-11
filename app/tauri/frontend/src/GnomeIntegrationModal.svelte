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
      lastError = err instanceof Error ? err.message : String(err);
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
      lastError = err instanceof Error ? err.message : String(err);
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
      lastError = err instanceof Error ? err.message : String(err);
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
      lastError = err instanceof Error ? err.message : String(err);
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
      lastError = err instanceof Error ? err.message : String(err);
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
  aria-label="Integración con GNOME Wayland"
>
  <header>
    <h2>Integración con GNOME Wayland</h2>
    <p class="muted">
      Identifica la aplicación origen de cada captura en sesiones GNOME
      Wayland. Sólo se comunica el identificador de la aplicación.
    </p>
  </header>

  {#if status?.kind === "not_applicable"}
    <article class="card-block">
      <p class="muted">
        Esta sesión ({status.session}) no necesita la integración. ClipVault
        usa los adaptadores nativos disponibles
        ({status.desktop}).
      </p>
    </article>
  {:else if status?.kind === "not_configured"}
    <article class="card-block">
      <p class="muted">
        La integración no está disponible en esta build
        ({status.reason}).
      </p>
    </article>
  {:else if status?.kind === "ready" && status.payload.applicable}
<article class="card-block">
    <h3>Estado de la integración</h3>
    <dl class="diag-list">
      <dt>Sesión</dt>
      <dd><code>{status.payload.session}</code></dd>
      <dt>Backend</dt>
      <dd><code>{status.payload.backend}</code></dd>
      <dt>Versión del protocolo</dt>
      <dd><code>{status.payload.protocol_version}</code></dd>
      <dt>Estado técnico</dt>
      <dd><code>{technicalState ?? "unknown"}</code></dd>
      <dt>Identificador publicado</dt>
      <dd><code>{identifier ?? "(none)"}</code></dd>
      {#if status.payload.detail}
        <dt>Detalle</dt>
        <dd><code>{status.payload.detail}</code></dd>
      {/if}
    </dl>
    {#if technicalState === "activation_pending"}
      <p class="muted">
        La extensión está instalada en
        <code>~/.local/share/gnome-shell/extensions/</code> pero GNOME Shell
        aún no la habilitó. Para terminar la activación usá
        <code>gnome-extensions enable clipvault@clipvault.app</code> desde
        una terminal, o abrí la app
        <strong>Extensiones</strong> de GNOME y activala desde ahí. Si la
        activación requiere reiniciar GNOME Shell, hacelo y volvé a
        iniciar ClipVault.
      </p>
    {:else if technicalState === "connected"}
      <p class="muted">
        Integración habilitada y enlace local establecido con la
        extensión. Esperando el primer identificador de aplicación
        desde GNOME Shell.
      </p>
    {:else if technicalState === "identified" || technicalState === "no_active_application"}
      <p class="muted">
        Integración conectada y publicando identificadores desde
        GNOME Shell.
      </p>
    {:else if technicalState === "disconnected" || technicalState === "communication_error"}
      <p class="muted">
        La extensión está instalada pero la conexión local se perdió.
        El listener reintentará automáticamente; si persiste, abrí
        <strong>Extensiones</strong> de GNOME y verificá que
        <code>clipvault@clipvault.app</code> siga habilitada.
      </p>
    {:else if technicalState === "incompatible"}
      <p class="muted">
        Esta versión de GNOME Shell no está soportada por la extensión.
        ClipVault seguirá capturando con el resto de los adaptadores.
      </p>
    {/if}
  </article>

    {#if shouldShowConsent}
      <article class="card-block" data-testid="gnome-consent-card">
        <h3>Activar integración GNOME</h3>
        <p>
          Permite que ClipVault identifique la aplicación nativa enfocada
          (Firefox, Terminal, Warp, etc.) en GNOME Wayland.
        </p>
        <ul>
          <li>Sólo se comunica el identificador de la aplicación.</li>
          <li>No se lee el contenido del portapapeles ni el de las ventanas.</li>
          <li>La decisión queda persistida y no se vuelve a pedir.</li>
          <li>Puedes revertirla desde este panel o desde Diagnóstico.</li>
        </ul>
        <div class="row">
          <button
            type="button"
            class="primary"
            on:click={() => applyConsent("accepted")}
            disabled={busy}
            data-testid="gnome-consent-accept"
          >
            {busy && lastAction === "accept" ? "Procesando…" : "Activar integración GNOME"}
          </button>
          <button
            type="button"
            class="secondary"
            on:click={() => applyConsent("declined")}
            disabled={busy}
            data-testid="gnome-consent-decline"
          >
            {busy && lastAction === "decline" ? "Guardando…" : "Ahora no"}
          </button>
        </div>
      </article>
    {:else if status.payload.consent === "accepted"}
      <article class="card-block" data-testid="gnome-install-card">
        <h3>Instalar</h3>
        <p class="muted">
          ClipVault instala la extensión localmente. La activación en
          GNOME requiere una acción manual del usuario (ver el panel
          de estado más abajo). Sin descargas externas, sin permisos
          de administrador.
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
              {busy ? "Instalando…" : "Instalar"}
            </button>
          {:else}
            <button
              type="button"
              class="secondary"
              on:click={retry}
              disabled={busy}
              data-testid="gnome-retry"
            >
              {busy ? "Reintentando…" : "Reintentar conexión"}
            </button>
            <button
              type="button"
              class="danger"
              on:click={deactivate}
              disabled={busy}
              data-testid="gnome-uninstall"
            >
              {busy ? "Desinstalando…" : "Deshabilitar"}
            </button>
          {/if}
        </div>
      </article>
    {:else if status.payload.consent === "disabled"}
      <article class="card-block">
        <h3>Integración deshabilitada</h3>
        <p class="muted">
          Desactivada manualmente. Puede volver a activarse en cualquier
          momento.
        </p>
        <div class="row">
          <button
            type="button"
            class="primary"
            on:click={() => applyConsent("accepted")}
            disabled={busy}
          >
            Reactivar
          </button>
        </div>
      </article>
    {/if}

    {#if shouldShowDeclined}
      <article class="card-block" data-testid="gnome-declined-card">
        <p class="muted">Has rechazado la integración. No volveremos a pedirla.</p>
        <div class="row">
          <button
            type="button"
            class="secondary"
            on:click={() => applyConsent("accepted")}
            disabled={busy}
          >
            Cambiar de opinión
          </button>
        </div>
      </article>
    {/if}

    {#if lastError}
      <p class="status error" role="alert" data-testid="gnome-error">{lastError}</p>
    {/if}
  {:else}
    <article class="card-block">
      <p class="muted">Recuperando el estado de la integración…</p>
      <div class="row">
        <button type="button" on:click={refresh} data-testid="gnome-refresh">Reintentar</button>
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
