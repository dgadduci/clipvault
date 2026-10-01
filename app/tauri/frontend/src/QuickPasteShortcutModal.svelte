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
  import { t } from "./lib/localization";
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
        return "shortcut.listener.ready";
      case "registering":
        return "shortcut.listener.registering";
      case "error":
        return "shortcut.listener.error";
      default:
        return "shortcut.listener.unknown";
    }
  }

  function describeCapability(enabled: boolean): string {
    return enabled
      ? "shortcut.capability.allowed"
      : "shortcut.capability.denied";
  }

  onMount(async () => {
    try {
      settings = await settingsGetCommand();
    } catch (error) {
      loadError = "shortcut.load_error";
    }
  });
</script>

<section class="shortcut" data-testid="shortcut-modal">
  <article>
    <h3>{$t("shortcut.effective.title")}</h3>
    <p class="shortcut-display">
      <kbd data-testid="shortcut-label">{hotkeyLabel}</kbd>
    </p>
    <p class="muted">
      {$t("shortcut.effective.description")}
    </p>
  </article>

  <article>
    <h3>{$t("shortcut.listener.title")}</h3>
    <p class="muted" data-testid="shortcut-listener-status">
      {$t(describeListener(listenerStatus))}
    </p>
    {#if listenerError}
      <p class="error" role="alert" data-testid="shortcut-listener-error">
        {$t(listenerError)}
      </p>
    {/if}
  </article>

  <article>
    <h3>{$t("shortcut.capability.title")}</h3>
    <p class="muted" data-testid="shortcut-capability-status">
      {$t(describeCapability(capabilityEnabled))}
    </p>
  </article>

  {#if loadError}
    <p class="error" role="alert" data-testid="shortcut-load-error">
      {$t(loadError)}
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
