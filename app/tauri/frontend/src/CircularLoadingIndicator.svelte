<script lang="ts">
  export let label: string;
  export let testId = "circular-loading-indicator";
  export let variant: "stage" | "overlay" | "startup" = "stage";
</script>

<div
  class="loading-indicator"
  class:overlay={variant === "overlay"}
  class:startup={variant === "startup"}
  role="status"
  aria-label={label}
  aria-live="polite"
  data-testid={testId}
>
  <span class="loading-indicator-spinner" aria-hidden="true"></span>
</div>

<style>
  .loading-indicator {
    display: grid;
    place-items: center;
    width: 100%;
    min-height: var(--cv-card-rail-height, 16rem);
  }

  .loading-indicator.overlay {
    position: absolute;
    inset: 0;
    z-index: 1;
    min-height: 0;
    pointer-events: none;
  }

  .loading-indicator.startup {
    min-height: 100vh;
  }

  .loading-indicator-spinner {
    display: block;
    width: 2rem;
    height: 2rem;
    border: 3px solid var(--cv-border, #30363d);
    border-top-color: var(--cv-accent, #60a5fa);
    border-right-color: var(--cv-accent, #60a5fa);
    border-radius: 50%;
    animation: clipvault-loading-spin 0.72s linear infinite;
  }

  @keyframes clipvault-loading-spin {
    to { transform: rotate(360deg); }
  }

  @media (prefers-reduced-motion: reduce) {
    .loading-indicator-spinner {
      animation: none;
      border-top-color: var(--cv-accent, #60a5fa);
    }
  }
</style>
