<script lang="ts">
  import { t } from "./lib/localization";
  import type { DesktopSetupIntegration } from "./types";

  export let integration: DesktopSetupIntegration;
  export let onConfigure: (
    integration: DesktopSetupIntegration,
    trigger: HTMLElement,
  ) => void;
  export let onDismiss: (
    integration: DesktopSetupIntegration,
  ) => Promise<void>;

  let dismissing = false;
  let dismissError = false;

  $: titleKey = `app.desktop_setup_guidance.${integration}.title`;
  $: descriptionKey = `app.desktop_setup_guidance.${integration}.description`;

  function configure(event: MouseEvent): void {
    onConfigure(
      integration,
      event.currentTarget as HTMLElement,
    );
  }

  async function dismiss(): Promise<void> {
    if (dismissing) return;
    dismissing = true;
    dismissError = false;
    try {
      await onDismiss(integration);
    } catch {
      dismissError = true;
    } finally {
      dismissing = false;
    }
  }
</script>

<section
  class="contextual-setup-card"
  aria-labelledby="contextual-setup-title-{integration}"
  aria-busy={dismissing}
  data-testid="contextual-desktop-setup-card"
  data-integration={integration}
>
  <div class="copy">
    <h2 id="contextual-setup-title-{integration}">{$t(titleKey)}</h2>
    <p>{$t(descriptionKey)}</p>
    {#if dismissError}
      <p class="error" role="alert">{$t("app.desktop_setup_guidance.dismiss_error")}</p>
    {/if}
  </div>
  <div class="actions">
    <button
      type="button"
      class="primary"
      on:click={configure}
      data-testid="contextual-setup-configure"
    >
      {$t("app.desktop_setup_guidance.configure")}
    </button>
    <button
      type="button"
      class="secondary"
      on:click={dismiss}
      disabled={dismissing}
      data-testid="contextual-setup-dismiss"
    >
      {$t("app.desktop_setup_guidance.dismiss")}
    </button>
  </div>
</section>

<style>
  .contextual-setup-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    margin: 0 0 0.75rem;
    padding: 0.85rem 1rem;
    border: 1px solid var(--cv-border, #424242);
    border-radius: 0.75rem;
    background: var(--cv-surface, #202020);
  }

  .copy {
    min-width: 0;
  }

  h2 {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 650;
  }

  p {
    margin: 0.25rem 0 0;
    color: var(--cv-muted, #b7b7b7);
    font-size: 0.85rem;
    line-height: 1.4;
  }

  p.error {
    color: var(--cv-error, #ff8a80);
  }

  .actions {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 0.5rem;
  }

  button {
    min-height: 2.25rem;
    padding: 0.35rem 0.75rem;
    border-radius: 0.5rem;
    font: inherit;
    cursor: pointer;
  }

  button:focus-visible {
    outline: 2px solid var(--cv-focus, #8ab4f8);
    outline-offset: 2px;
  }

  @media (max-width: 700px) {
    .contextual-setup-card {
      align-items: flex-start;
      flex-direction: column;
    }

    .actions {
      flex-wrap: wrap;
    }
  }
</style>
