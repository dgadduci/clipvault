<script lang="ts">
  /**
   * Shared modal shell.
   *
   * The desktop coordinates four configuration modals through a single
   * discriminated state. Every modal reuses this component so the
   * accessibility, focus and teardown invariants live in one place:
   *
   *   - `role="dialog"` + `aria-modal="true"` for AT semantics.
   *   - `aria-labelledby` wired to the slot-provided title so screen
   *     readers announce the dialog correctly.
   *   - Initial focus moves to the first focusable child once the
   *     dialog mounts and returns to the trigger element when it
   *     closes.
   *   - Tab / Shift+Tab navigation stays inside the dialog until it
   *     closes (focus trap).
   *   - Escape closes the dialog and clicking the backdrop closes it
   *     when no blocking operation prevents the close.
   *   - Document listeners are installed on mount, detached on
   *     destroy and never re-attached across re-renders.
   *
   * The component is intentionally presentational: it forwards an
   * `onClose` callback and a `busy` flag so the parent can keep the
   * modal open while a destructive operation is in flight (the
   * backdrop / Escape / focus trap stay disabled in that window).
   */
  import { onDestroy, tick } from "svelte";

  export let open: boolean = false;
  export let titleId: string;
  export let title: string;
  export let busy: boolean = false;
  /** Element returned focus when the dialog closes. */
  export let returnFocusTo: HTMLElement | null = null;
  export let onClose: () => void;

  let dialogEl: HTMLDivElement | undefined;
  let previouslyFocused: HTMLElement | null = null;

  function focusableElements(): HTMLElement[] {
    if (!dialogEl) return [];
    const selector =
      'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';
    return Array.from(dialogEl.querySelectorAll<HTMLElement>(selector)).filter(
      (el) => !el.hasAttribute("aria-hidden"),
    );
  }

  function focusFirst(): void {
    if (!dialogEl) return;
    const targets = focusableElements();
    if (targets.length > 0) {
      targets[0].focus();
    } else {
      dialogEl.focus();
    }
  }

  function trapFocus(event: KeyboardEvent): void {
    if (event.key !== "Tab" || busy) return;
    const targets = focusableElements();
    if (targets.length === 0) {
      event.preventDefault();
      return;
    }
    const first = targets[0];
    const last = targets[targets.length - 1];
    const active = document.activeElement as HTMLElement | null;
    if (event.shiftKey && active === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && active === last) {
      event.preventDefault();
      first.focus();
    }
  }

  function onKeydown(event: KeyboardEvent): void {
    if (!open) return;
    if (event.key === "Escape") {
      if (busy) return;
      event.preventDefault();
      onClose();
      return;
    }
    trapFocus(event);
  }

  function onBackdropClick(event: MouseEvent): void {
    if (busy) return;
    if (event.target === event.currentTarget) {
      onClose();
    }
  }

  async function focusOnOpen(openNow: boolean): Promise<void> {
    if (!openNow) {
      if (returnFocusTo && document.contains(returnFocusTo)) {
        returnFocusTo.focus();
      } else if (previouslyFocused && document.contains(previouslyFocused)) {
        previouslyFocused.focus();
      }
      previouslyFocused = null;
      return;
    }
    previouslyFocused = document.activeElement as HTMLElement | null;
    await tick();
    focusFirst();
  }

  $: void focusOnOpen(open);

  function detachDocument(): void {
    document.removeEventListener("keydown", onKeydown, true);
  }

  function attachDocument(): void {
    document.addEventListener("keydown", onKeydown, true);
  }

  $: if (open) {
    attachDocument();
  } else {
    detachDocument();
  }

  onDestroy(() => {
    detachDocument();
    if (returnFocusTo && document.contains(returnFocusTo)) {
      returnFocusTo.focus();
    } else if (previouslyFocused && document.contains(previouslyFocused)) {
      previouslyFocused.focus();
    }
  });
</script>

{#if open}
  <div
    class="modal-overlay"
    data-testid="modal-overlay"
    role="presentation"
    on:click={onBackdropClick}
  >
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby={titleId}
      bind:this={dialogEl}
      tabindex="-1"
      data-busy={busy}
    >
      <header class="modal-header">
        <h2 class="modal-title" id={titleId} data-testid="modal-title">
          {title}
        </h2>
        <button
          type="button"
          class="modal-close"
          aria-label="Cerrar"
          title="Cerrar"
          data-testid="modal-close"
          on:click={onClose}
          disabled={busy}
        >
          ×
        </button>
      </header>
      <div class="modal-body">
        <slot />
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(8, 11, 16, 0.75);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: 3rem 1rem 1rem;
    z-index: 60;
  }

  .modal {
    background: var(--cv-bg-elevated, #161b22);
    color: var(--cv-fg, #f0f4f8);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 12px);
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.5);
    width: 100%;
    max-width: 640px;
    max-height: calc(100vh - 4rem);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }

  .modal[data-busy="true"] {
    cursor: progress;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 1rem 1.25rem;
    border-bottom: 1px solid var(--cv-border, #30363d);
  }

  .modal-title {
    margin: 0;
    font-size: var(--cv-title-md, 1rem);
    font-weight: 600;
    color: var(--cv-fg, #f0f4f8);
  }

  .modal-close {
    background: transparent;
    color: inherit;
    border: 0;
    padding: 0.25rem 0.5rem;
    border-radius: var(--cv-radius-sm, 6px);
    cursor: pointer;
    font-size: 1.1rem;
    line-height: 1;
  }

  .modal-close:hover:not(:disabled) {
    background: var(--cv-bg-hover, rgba(255, 255, 255, 0.08));
  }

  .modal-close:disabled {
    color: var(--cv-fg-muted, #94a3b8);
    cursor: not-allowed;
  }

  .modal-body {
    padding: 1rem 1.25rem 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
</style>