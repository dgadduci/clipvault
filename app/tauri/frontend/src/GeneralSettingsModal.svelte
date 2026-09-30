<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import {
    captureControlGetCommand,
    captureControlSetCommand,
  } from "./lib/tauri.ts";
  import {
    listenCaptureControlChanged,
    listenCaptureControlError,
  } from "./lib/captureControlUpdates.ts";
  import { captureToggleShortcutLabel } from "./lib/clipboardCaptureControl.ts";

  export let open = false;
  export let platformOs: string | null = null;
  export let displayServer: string | null = null;

  let captureEnabled: boolean | null = null;
  let loading = false;
  let saving = false;
  let error: string | null = null;
  let requestId = 0;
  let disposed = false;
  let unlisten: (() => void) | null = null;
  let unlistenError: (() => void) | null = null;

  $: shortcut = captureToggleShortcutLabel(platformOs);
  $: if (open) void loadCaptureState();

  async function loadCaptureState(): Promise<void> {
    const currentRequest = ++requestId;
    loading = true;
    error = null;
    try {
      const enabled = await captureControlGetCommand();
      if (currentRequest === requestId) captureEnabled = enabled;
    } catch (loadError) {
      if (currentRequest === requestId) {
        error = loadError instanceof Error ? loadError.message : String(loadError);
      }
    } finally {
      if (currentRequest === requestId) loading = false;
    }
  }

  async function toggleCapture(): Promise<void> {
    if (captureEnabled === null || saving) return;
    saving = true;
    try {
      captureEnabled = await captureControlSetCommand({
        enabled: !captureEnabled,
      });
    } catch (saveError) {
      error = saveError instanceof Error ? saveError.message : String(saveError);
    } finally {
      saving = false;
    }
  }

  onMount(() => {
    listenCaptureControlChanged((enabled) => {
      captureEnabled = enabled;
      error = null;
    })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch((listenError) => {
        error = listenError instanceof Error ? listenError.message : String(listenError);
      });
    listenCaptureControlError(() => {
      error = "No se pudo guardar el cambio. El estado anterior sigue vigente.";
    })
      .then((stop) => {
        if (disposed) stop();
        else unlistenError = stop;
      })
      .catch((listenError) => {
        error = listenError instanceof Error ? listenError.message : String(listenError);
      });
    return () => {
      disposed = true;
      unlisten?.();
      unlisten = null;
      unlistenError?.();
      unlistenError = null;
    };
  });

  onDestroy(() => {
    disposed = true;
    unlisten?.();
    unlisten = null;
    unlistenError?.();
    unlistenError = null;
  });
</script>

<section class="general-settings" data-testid="general-settings-modal">
  <article data-testid="clipboard-capture-setting">
    <h3>Captura del portapapeles</h3>
    <p class="muted">
      Controla si ClipVault guarda nuevas capturas locales. El historial existente
      se conserva; las capturas recibidas de equipos vinculados no se pausan.
    </p>

    {#if loading && captureEnabled === null}
      <p class="muted" role="status" data-testid="capture-control-loading">
        Cargando estado…
      </p>
    {:else if captureEnabled !== null}
      <p class="state" data-testid="capture-control-state" aria-live="polite">
        Estado: {captureEnabled ? "Activa" : "Pausada"}
      </p>
      <div class="controls">
        <button
          type="button"
          class="secondary"
          on:click={toggleCapture}
          disabled={saving}
          aria-busy={saving}
          aria-pressed={captureEnabled}
          data-testid="capture-control-toggle"
        >
          {saving
            ? "Guardando…"
            : captureEnabled
              ? "Pausar capturas"
              : "Reanudar capturas"}
        </button>
        <span class="shortcut" data-testid="capture-control-shortcut">
          Atajo global: {shortcut}
        </span>
      </div>
      {#if platformOs === "linux" && displayServer === "wayland"}
        <p class="muted" data-testid="capture-control-wayland-note">
          En Wayland, el atajo requiere que la integración de GNOME o KDE esté
          habilitada en Development. También puedes cambiar este estado desde el
          menú del icono del sistema.
        </p>
      {/if}
    {/if}

    {#if error}
      <p class="error" role="alert" data-testid="capture-control-error">
        No se pudo actualizar la captura: {error}
      </p>
    {/if}
  </article>
</section>

<style>
  .general-settings {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }
  article {
    background: var(--cv-bg-surface, #0e1116);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 10px);
    padding: 0.85rem 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  h3 {
    margin: 0;
    font-size: var(--cv-title-sm, 0.95rem);
    font-weight: 600;
  }
  p {
    margin: 0;
  }
  .muted,
  .shortcut {
    color: var(--cv-fg-muted, #94a3b8);
    font-size: var(--cv-muted, 0.82rem);
  }
  .state {
    font-weight: 600;
  }
  .controls {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.75rem;
  }
  .error {
    color: var(--cv-danger, #f87171);
  }
</style>
