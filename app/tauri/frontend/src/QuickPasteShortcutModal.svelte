<script lang="ts">
  /**
   * Read-only modal showing the effective quick-paste shortcut and
   * the listener/capability state the backend already exposes.
   *
   * This modal MUST stay informational in v0.1: it does not register
   * a second hotkey, does not replace the transient quick-paste
   * window and does not open it. The shortcut label comes from the
   * persisted `quick_paste_hotkey` setting the backend stored; if
   * none exists yet we fall back to the platform default.
   */
  import { onMount } from "svelte";
  import { settingsGetCommand } from "./lib/tauri";
  import {
    defaultQuickPasteShortcutLabel,
    platformFromDiagnostics,
    type Platform,
  } from "./lib/platform";
  import type { Capabilities, HotkeySpec, Settings } from "./types";

  export let capabilities: Capabilities | null = null;
  export let listenerStatus:
    | "registering"
    | "ready"
    | "error"
    | "unknown" = "unknown";
  export let listenerError: string | null = null;
  export let platformOs: string | null = null;

  let settings: Settings | null = null;
  let loadError: string | null = null;

  $: platform = platformFromDiagnostics(platformOs);

  $: hotkeyLabel = formatHotkey(settings?.quick_paste_hotkey ?? null, platform);
  $: capabilityEnabled = capabilities?.global_hotkey ?? false;

  function formatHotkey(spec: HotkeySpec | null, host: Platform): string {
    if (!spec) return defaultQuickPasteShortcutLabel(host);
    const parts: string[] = [];
    if (spec.cmd_or_ctrl) {
      parts.push(host === "macos" ? "Cmd" : "Ctrl");
    }
    if (spec.shift) parts.push("Shift");
    if (spec.alt) parts.push("Alt");
    if (spec.meta) parts.push("Meta");
    parts.push(spec.key.toUpperCase());
    return parts.join(" + ");
  }

  function describeListener(
    status: typeof listenerStatus,
  ): string {
    switch (status) {
      case "ready":
        return "El atajo global está activo.";
      case "registering":
        return "Registrando el atajo global…";
      case "error":
        return "No se pudo registrar el atajo global.";
      default:
        return "Estado del atajo no disponible.";
    }
  }

  function describeCapability(enabled: boolean): string {
    return enabled
      ? "La plataforma permite registrar atajos globales."
      : "La plataforma no permite registrar atajos globales.";
  }

  onMount(async () => {
    try {
      settings = await settingsGetCommand();
    } catch (error) {
      loadError = error instanceof Error ? error.message : String(error);
    }
  });
</script>

<section class="shortcut" data-testid="shortcut-modal">
  <article>
    <h3>Atajo efectivo</h3>
    <p class="shortcut-display">
      <kbd data-testid="shortcut-label">{hotkeyLabel}</kbd>
    </p>
    <p class="muted">
      El atajo abre la ventana transient de pegado rápido. Esta versión no
      permite editarlo: el módulo de teclado del sistema operativo es la
      fuente de verdad.
    </p>
  </article>

  <article>
    <h3>Estado del listener</h3>
    <p class="muted" data-testid="shortcut-listener-status">
      {describeListener(listenerStatus)}
    </p>
    {#if listenerError}
      <p class="error" role="alert" data-testid="shortcut-listener-error">
        {listenerError}
      </p>
    {/if}
  </article>

  <article>
    <h3>Capacidad de la plataforma</h3>
    <p class="muted" data-testid="shortcut-capability-status">
      {describeCapability(capabilityEnabled)}
    </p>
  </article>

  {#if loadError}
    <p class="error" role="alert" data-testid="shortcut-load-error">
      {loadError}
    </p>
  {/if}
</section>

<style>
  .shortcut {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }
  .shortcut article {
    background: var(--cv-bg-surface, #0e1116);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 10px);
    padding: 0.85rem 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .shortcut h3 {
    margin: 0;
    font-size: var(--cv-title-sm, 0.95rem);
    font-weight: 600;
  }
  .shortcut-display {
    margin: 0;
  }
  kbd {
    display: inline-block;
    background: #1f2937;
    color: #f0f4f8;
    border: 1px solid var(--cv-border, #30363d);
    border-radius: 6px;
    padding: 0.35rem 0.75rem;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.95rem;
  }
  .muted {
    color: var(--cv-fg-muted, #94a3b8);
    margin: 0;
  }
  .error {
    color: var(--cv-fg-error, #f87171);
    margin: 0;
  }
</style>