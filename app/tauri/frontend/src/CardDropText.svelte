<script lang="ts">
  /**
   * Inline drop indicator the desktop renders below the rail of
   * recent captures.
   *
   * The text is the simplest possible drag-and-drop target the
   * sidebar surfaces: it listens for the four drag events on its
   * own DOM node and uses the same helpers the sidebar relies on so
   * the contract the card and the rest of the desktop agree on
   * (private MIME, `text/plain` fallback and the in-memory drag
   * session) stays consistent.
   *
   * The visible behaviour is intentionally narrow:
   *
   *   - Default colour: muted gray (the desktop's neutral tone).
   *   - On `dragenter`: highlight (accent blue) so the user sees
   *     the drop is a valid target.
   *   - On `drop`: success tone (green) so the user knows the drop
   *     landed.
   *   - On `dragleave` outside the text or `dragend`: revert to the
   *     neutral tone so the text never stays highlighted when the
   *     drag was cancelled.
   *
   * The component never mutates the card, the entry, the
   * organisation or the bridge. The drop is purely a visual cue.
   * The actual mutation flow (`entry_collections_set` +
   * `combineMemberships` + `refreshOrganization`) still lives in
   * the sidebar's drop zone and `App.svelte::handleCardDrop`.
   *
   * Implementation notes:
   *
   *   - The element is a `<div>` (not a `<p>`) because some
   *     WebKit/Tauri builds treat `<p>` as a text-only element
   *     for hit-testing and the drag events can be lost at the
   *     paragraph boundary.
   *   - The component declares its own reactive state via the
   *     `$state` rune so the runtime re-renders the class list
   *     and the `data-drop-state` attribute the moment the
   *     handler fires.
   *   - The handlers live in `lib/cardDropTextHandlers.ts` so the
   *     integration tests exercise the EXACT factory the
   *     component imports — no drift between test and
   *     production.
   *   - The CSS uses `pointer-events: auto` explicitly so a
   *     regression that disabled events on a parent cannot
   *     silently mute this target.
   */
  import { createCardDropTextHandlers } from "./lib/cardDropTextHandlers";
  import {
    POINTER_DRAG_END_EVENT,
    type PointerDragDetail,
  } from "./lib/pointerDragAndDrop";

  type DropTextState = "idle" | "hover" | "dropped";

  let dropTextState = $state<DropTextState>("idle");
  let dropTextResetTimer: ReturnType<typeof setTimeout> | null = null;

  function clearDropTextResetTimer(): void {
    if (dropTextResetTimer !== null) {
      clearTimeout(dropTextResetTimer);
      dropTextResetTimer = null;
    }
  }

  function resetDropTextLater(): void {
    clearDropTextResetTimer();
    dropTextResetTimer = setTimeout(() => {
      dropTextState = "idle";
      dropTextResetTimer = null;
    }, 1800);
  }

  const handlers = createCardDropTextHandlers({
    setState(state) {
      dropTextState = state;
    },
    scheduleReset() {
      resetDropTextLater();
    },
    clearTimer() {
      clearDropTextResetTimer();
    },
  });

  // The component itself relies on the document-level `dragend`
  // listener the sidebar already installs. We mirror the cancel
  // contract here through a Svelte action so a foreign dragend
  // (Escape, drop outside any target) always returns the text to
  // `idle`. The action runs once on mount and is cleaned up on
  // destroy.
  function dragendCancel(_node: HTMLElement): { destroy(): void } {
    function onWindowDragEnd(): void {
      if (dropTextState === "dropped") {
        // The drop timer will revert; do nothing.
        return;
      }
      dropTextState = "idle";
    }
    document.addEventListener("dragend", onWindowDragEnd);
    document.addEventListener(POINTER_DRAG_END_EVENT, onWindowDragEnd);
    return {
      destroy(): void {
        document.removeEventListener("dragend", onWindowDragEnd);
        document.removeEventListener(POINTER_DRAG_END_EVENT, onWindowDragEnd);
      },
    };
  }

  function pointerDropIndicator(node: HTMLElement): { destroy(): void } {
    const onPointerDragOver = (event: Event): void => {
      handlers.onPointerDragOver(event as CustomEvent<PointerDragDetail>);
    };
    const onPointerDrop = (event: Event): void => {
      handlers.onPointerDrop(event as CustomEvent<PointerDragDetail>);
    };
    node.addEventListener("clipvault-pointer-drag-over", onPointerDragOver);
    node.addEventListener("clipvault-pointer-drop", onPointerDrop);
    return {
      destroy(): void {
        node.removeEventListener(
          "clipvault-pointer-drag-over",
          onPointerDragOver,
        );
        node.removeEventListener("clipvault-pointer-drop", onPointerDrop);
      },
    };
  }
</script>

<div
  class="card-drop-text"
  class:hover={dropTextState === "hover"}
  class:dropped={dropTextState === "dropped"}
  data-testid="card-drop-text"
  data-drop-state={dropTextState}
  role="note"
  aria-live="polite"
  tabindex="0"
  use:dragendCancel
  ondragenter={handlers.onDragEnter}
  ondragover={handlers.onDragOver}
  ondragleave={handlers.onDragLeave}
  ondrop={handlers.onDrop}
  use:pointerDropIndicator
>
  Suelta aquí una tarjeta capturada para confirmar que el drop
  funciona.
</div>

<style>
  /*
   * Drop indicator. The text sits directly below the rail so the
   * user can always see whether a drop landed. The colour change
   * is the single signal — the text never moves, never grows and
   * never animates beyond a smooth background/border fade.
   *
   * The `pointer-events: auto` rule is explicit so a regression
   * that disabled events on a parent (the desktop shell, the
   * layout column) cannot silently mute this target.
   */
  .card-drop-text {
    margin: 0.75rem 0 0;
    padding: 0.85rem 1rem;
    border-radius: var(--cv-radius-sm, 6px);
    border: 2px dashed var(--cv-border, #30363d);
    background: transparent;
    color: var(--cv-fg-muted, #94a3b8);
    font-size: var(--cv-body, 0.9rem);
    line-height: 1.45;
    pointer-events: auto;
    cursor: default;
    transition:
      background-color 0.18s ease-out,
      border-color 0.18s ease-out,
      color 0.18s ease-out;
    user-select: none;
  }
  .card-drop-text.hover {
    background: rgba(37, 99, 235, 0.32);
    border-color: var(--cv-accent, #2563eb);
    border-style: solid;
    color: var(--cv-fg, #f0f4f8);
  }
  .card-drop-text.dropped {
    background: rgba(74, 222, 128, 0.32);
    border-color: var(--cv-fg-ok, #4ade80);
    border-style: solid;
    color: var(--cv-fg, #f0f4f8);
  }
</style>
