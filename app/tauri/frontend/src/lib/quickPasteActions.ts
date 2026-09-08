// Pure helpers for the Quick Paste keyboard and per-entry menu
// flows.
//
// The module owns **no** UI or Tauri state. It exists so:
//   - the Svelte component can ask "what does Enter / Shift+Enter do
//     for this entry?" without re-implementing the capability rules
//     every render;
//   - the menu can compute the list of type-appropriate direct
//     actions from the entry capabilities (the regression that
//     accidentally re-rendered text actions on image rows is exactly
//     what this helper prevents);
//   - the order helper can keep favorites on top while preserving the
//     ranking the search or recents feed produces for each group.
//
// The helpers are pure: they never touch the DOM, the IPC layer or
// the clipboard. Every state mutation lives in `QuickPaste.svelte`,
// every side effect lives in `quickPasteController.ts`.

import type { EntryRecord, SearchHit } from "../types.ts";
import {
  hasRenderableRichText,
  isImageEntry,
} from "./clipboardAsset.ts";

/**
 * Mode the keyboard flow asks the backend to copy. `null` is the
 * "image default" — the Rust side ignores the mode for image rows
 * and runs the bitmap write.
 */
export type CopyMode = "plain" | "rich" | null;

/**
 * What a keyboard activation should do for the highlighted entry.
 *
 * - `"none"` means the input is a no-op (no selection, image-only
 *   `Shift+Enter`, …).
 * - `"copy"` means the frontend should run the copy-only flow with
 *   the supplied `mode`. The frontend never falls back to the paste
 *   flow when the copy path fails; the typed outcome decides whether
 *   the window stays hidden.
 */
export type QuickPasteEnterAction =
  | { kind: "none" }
  | { kind: "copy"; mode: CopyMode };

/**
 * Capability view the helpers use to decide which actions are
 * available for a given entry.
 *
 * The Svelte layer already computes `hasRenderableRichText` /
 * `isImageEntry` from the typed `EntryRecord`. The pure helpers
 * accept this precomputed view so the helpers stay framework-free
 * and the callers can pass either a fetched entry (rich metadata
 * available) or a search hit (rich metadata available through
 * `record`).
 */
export interface QuickPasteEntryCapabilities {
  /** Whether the entry is an image row. */
  isImage: boolean;
  /** Whether the entry exposes at least one rich representation. */
  hasRich: boolean;
}

/**
 * Resolve the capabilities of an entry from the typed shape the
 * frontend already hydrates. Used by the order helper to decide
 * favourites-first ordering, and by the action helpers when a
 * caller wants the menu to derive its actions from the entry
 * itself instead of passing a precomputed view.
 */
export function capabilitiesOf(
  entry: Pick<
    EntryRecord,
    "content_type" | "rich_text_hash" | "rich_html_ref" | "rich_rtf_ref" | "rich_preview_ref"
  >,
): QuickPasteEntryCapabilities {
  return {
    isImage: isImageEntry(entry),
    hasRich: hasRenderableRichText(entry),
  };
}

/**
 * What `Enter` does for the supplied entry.
 *
 * Modalities documented in `quick-paste/spec.md`:
 * - rich + plain: copy rich text;
 * - only plain: copy plain text;
 * - image: copy image;
 * - no selection: no-op.
 *
 * The function is total: every entry shape returns exactly one
 * structured action so the Svelte layer can branch on `kind`.
 */
export function quickPasteEnterAction(
  capabilities: QuickPasteEntryCapabilities,
): QuickPasteEnterAction {
  if (capabilities.isImage) {
    return { kind: "copy", mode: null };
  }
  if (capabilities.hasRich) {
    return { kind: "copy", mode: "rich" };
  }
  return { kind: "copy", mode: "plain" };
}

/**
 * What `Shift+Enter` does for the supplied entry.
 *
 * Modalities documented in `quick-paste/spec.md`:
 * - rich + plain: copy plain text;
 * - only plain: no-op (Enter already covered the only available
 *   representation);
 * - image: no-op;
 * - no selection: no-op.
 */
export function quickPasteShiftEnterAction(
  capabilities: QuickPasteEntryCapabilities,
): QuickPasteEnterAction {
  if (capabilities.isImage) {
    return { kind: "none" };
  }
  if (capabilities.hasRich) {
    return { kind: "copy", mode: "plain" };
  }
  return { kind: "none" };
}

/**
 * What the click / keyboard confirmation controller does for the
 * supplied entry.
 *
 * `shiftKey` selects between the Enter and Shift+Enter action
 * tables so a row click and a `Cmd+Shift+V` → Enter shortcut both
 * share the same dispatch. A plain click is `shiftKey: false`; a
 * Shift+Click is `shiftKey: true`.
 *
 * The helper exists so the Svelte layer can route the row's
 * `on:click` handler and the `Enter`/`Shift+Enter` keyboard
 * shortcuts through a single switch and a unit test can prove
 * that the click and the keyboard cannot drift apart by inspecting
 * one function.
 */
export function quickPasteConfirmAction(
  capabilities: QuickPasteEntryCapabilities,
  shiftKey: boolean,
): QuickPasteEnterAction {
  return shiftKey
    ? quickPasteShiftEnterAction(capabilities)
    : quickPasteEnterAction(capabilities);
}

/**
 * Single copy-only menu action rendered for an entry.
 *
 * The shape is intentionally self-describing: the caller renders each
 * item from its own fields (label, testid, aria-label, tooltip,
 * disabled, mode). The backend mode is the same value the menu
 * forwards to `copyEntryCommand` so the existing copy-only flow
 * stays a thin wrapper around `clipvault_copy_entry` and never
 * invokes a synthetic paste controller.
 *
 * `kind` discriminates the four documented actions:
 * - `"copy"` — the canonical copy action (image rows forward
 *   `mode: null`; plain text forwards `mode: "plain"`).
 * - `"copy-rich"` — rich-text copy; only enabled when the entry
 *   exposes a sanitised rich preview.
 * - `"copy-plain"` — plain-text copy; always enabled for a textual
 *   entry.
 * - `"preview"` — opens the in-window preview overlay. The action
 *   never writes to the clipboard and never mutates history.
 */
export type QuickPasteMenuAction =
  | {
      kind: "copy";
      label: string;
      testId: string;
      mode: "plain" | null;
      ariaLabel: string;
      tooltip: string;
      disabled: boolean;
    }
  | {
      kind: "copy-rich";
      label: string;
      testId: string;
      mode: "rich";
      ariaLabel: string;
      tooltip: string;
      disabled: boolean;
    }
  | {
      kind: "copy-plain";
      label: string;
      testId: string;
      mode: "plain";
      ariaLabel: string;
      tooltip: string;
      disabled: boolean;
    }
  | {
      kind: "preview";
      label: string;
      testId: string;
      ariaLabel: string;
      tooltip: string;
      disabled: boolean;
    };

/**
 * Visible label the Quick Paste menu renders for the canonical
 * copy action. The string is shared with the `Copiar` action the
 * image menu exposes so the two surfaces read identically and never
 * suggest a synthetic paste.
 */
export const QUICK_PASTE_COPY_LABEL = "Copiar";

/**
 * Visible label the Quick Paste menu renders for the rich-text
 * copy. The wording mirrors the spec the
 * `quick-paste-preview-ui/spec.md` change pins so the user never
 * has to relearn the action.
 */
export const QUICK_PASTE_COPY_RICH_LABEL = "Copiar texto enriquecido";

/**
 * Visible label the Quick Paste menu renders for the plain-text
 * copy. Mirrors the spec wording so the menu and the keyboard
 * shortcut share the same vocabulary.
 */
export const QUICK_PASTE_COPY_PLAIN_LABEL = "Copiar texto plano";

/**
 * Visible label the Quick Paste menu renders for the preview
 * action. The label is shared with the keyboard hint the design
 * documents so the user sees the same wording everywhere.
 */
export const QUICK_PASTE_PREVIEW_LABEL = "Previsualizar";

/**
 * Copy-only menu actions the Quick Paste row exposes for an entry.
 *
 * The Quick Paste matrix documented in
 * `openspec/changes/quick-paste-preview-ui/specs/quick-paste/spec.md`
 * is intentionally tighter than the desktop rail's:
 *
 * - plain (or any non-rich) entry → `Copiar` + `Previsualizar`;
 * - rich entry → `Copiar texto enriquecido` + `Copiar texto plano` +
 *   `Previsualizar`;
 * - image entry → `Copiar` + `Previsualizar`.
 *
 * The non-rich branch intentionally hides the rich-text copy: the
 * row already exposes the rich-disabled affordance on the rail, and
 * the Quick Paste palette is documented to be a compact tool the
 * user can drive with one keystroke per intent. The menu NEVER
 * surfaces a `Pegar` wording and NEVER routes through
 * `pasteEntryCommand`; the copy path is the same `clipvault_copy_entry`
 * flow `Enter` / `Shift+Enter` / row click consult, only with
 * `hideAfterSuccess: false` so the window stays visible after the
 * success branch and the user can run `Cmd/Ctrl+V` against the
 * just-copied representation.
 *
 * `Previsualizar` is always appended; it never writes to the
 * clipboard and is always enabled unless a copy / pin round-trip is
 * already in flight for the same row.
 */
export function quickPasteMenuActions(
  entry: Pick<
    EntryRecord,
    "content_type" | "rich_text_hash" | "rich_html_ref" | "rich_rtf_ref" | "rich_preview_ref"
  >,
  title: string,
  options: { copyBusy: boolean },
): QuickPasteMenuAction[] {
  const actions: QuickPasteMenuAction[] = [];
  const hasRich = hasRenderableRichText(entry);
  if (isImageEntry(entry)) {
    actions.push({
      kind: "copy",
      label: QUICK_PASTE_COPY_LABEL,
      testId: "quick-paste-menu-copy",
      mode: null,
      ariaLabel: `Copiar ${title}`,
      tooltip: "Copiar la imagen capturada.",
      disabled: options.copyBusy,
    });
  } else if (hasRich) {
    actions.push({
      kind: "copy-rich",
      label: QUICK_PASTE_COPY_RICH_LABEL,
      testId: "quick-paste-menu-copy-rich",
      mode: "rich",
      ariaLabel: `${QUICK_PASTE_COPY_RICH_LABEL} de ${title}`,
      tooltip: "Copiar la entrada conservando el formato.",
      disabled: options.copyBusy,
    });
    actions.push({
      kind: "copy-plain",
      label: QUICK_PASTE_COPY_PLAIN_LABEL,
      testId: "quick-paste-menu-copy-plain",
      mode: "plain",
      ariaLabel: `${QUICK_PASTE_COPY_PLAIN_LABEL} de ${title}`,
      tooltip: "Copiar únicamente el texto plano.",
      disabled: options.copyBusy,
    });
  } else {
    actions.push({
      kind: "copy",
      label: QUICK_PASTE_COPY_LABEL,
      testId: "quick-paste-menu-copy",
      mode: "plain",
      ariaLabel: `Copiar ${title}`,
      tooltip: "Copiar la entrada como texto plano.",
      disabled: options.copyBusy,
    });
  }
  actions.push({
    kind: "preview",
    label: QUICK_PASTE_PREVIEW_LABEL,
    testId: "quick-paste-menu-preview",
    ariaLabel: `${QUICK_PASTE_PREVIEW_LABEL} ${title}`,
    tooltip: "Mostrar la captura sin pegarla.",
    disabled: options.copyBusy,
  });
  return actions;
}

/**
 * Whether any copy-only menu action is available for the entry.
 *
 * Quick Paste hides the menu trigger when the entry has no action
 * at all so the layout stays compact for entries that don't need a
 * second affordance.
 */
export function hasAnyQuickPasteAction(
  entry: Pick<
    EntryRecord,
    "content_type" | "rich_text_hash" | "rich_html_ref" | "rich_rtf_ref" | "rich_preview_ref"
  >,
): boolean {
  // Today every Quick Paste entry exposes at least one copy-only
  // menu action (image → Copiar, text → plain text, rich → rich +
  // plain). The helper exists so a future entry shape without a
  // copy action can hide the menu trigger without a refactor.
  void entry;
  return true;
}

/**
 * Build the ordered list of ids the Quick Paste list renders.
 *
 * Modalities documented in `quick-paste/spec.md`:
 *
 *   - Recent mode: favourites come first; the favourite group and the
 *     non-favourite group are sorted by `created_at DESC` with `id DESC`
 *     as the tie-breaker so a newer capture never appears after an
 *     older one within the same group. The source-feed ordering is
 *     intentionally overridden: the previous contract trusted the
 *     recents feed to come pre-sorted, which silently broke whenever a
 *     `history-updated` event landed while a stale response was still
 *     pending.
 *   - Search mode: favourites come first; the favourite group and the
 *     non-favourite group preserve the ranking `SearchService`
 *     returned. Relevance is never replaced by capture time — when the
 *     user is searching, they expect matches by score.
 *
 * The helper is pure: it never mutates the input lists and never
 * rebuilds the records. The callers pass the typed `EntryRecord[]`
 * and `SearchHit[]` they already hydrate.
 */
export function quickPasteOrderedIds(
  mode: "recent" | "search" | "idle",
  recents: EntryRecord[],
  searchHits: SearchHit[],
): number[] {
  if (mode === "search") {
    return orderWithSearchRanking(searchHits);
  }
  if (mode === "idle") {
    return [];
  }
  return orderRecentsChronologically(recents);
}

/**
 * Order the search-result candidates with favourites first and the
 * `SearchService` ranking intact within each group. The helper is
 * total: an empty input yields an empty array.
 */
function orderWithSearchRanking(searchHits: SearchHit[]): number[] {
  if (searchHits.length === 0) {
    return [];
  }
  const positions = new Map<number, number>();
  searchHits.forEach((hit, index) => {
    positions.set(hit.entry_id, index);
  });
  const pinned: number[] = [];
  const unpinned: number[] = [];
  for (const hit of searchHits) {
    const bucket = hit.record.is_pinned ? pinned : unpinned;
    bucket.push(hit.entry_id);
  }
  pinned.sort((a, b) => {
    const posA = positions.get(a) ?? 0;
    const posB = positions.get(b) ?? 0;
    return posA - posB;
  });
  unpinned.sort((a, b) => {
    const posA = positions.get(a) ?? 0;
    const posB = positions.get(b) ?? 0;
    return posA - posB;
  });
  return [...pinned, ...unpinned];
}

/**
 * Order the recents feed chronologically. The favourite group comes
 * first; both the favourite and the non-favourite groups are sorted
 * by `created_at DESC` with `id DESC` as the tie-breaker. The helper
 * treats `created_at` as an ISO 8601 string the backend already
 * produces; a malformed value collapses to the empty string so the
 * comparator never throws.
 *
 * The helper is total: an empty input yields an empty array.
 */
function orderRecentsChronologically(recents: EntryRecord[]): number[] {
  if (recents.length === 0) {
    return [];
  }
  const records = new Map<number, EntryRecord>();
  const pinned: number[] = [];
  const unpinned: number[] = [];
  for (const record of recents) {
    records.set(record.id, record);
    const bucket = record.is_pinned ? pinned : unpinned;
    bucket.push(record.id);
  }
  const compareByCreatedAtDesc = (a: number, b: number): number => {
    const recordA = records.get(a);
    const recordB = records.get(b);
    const timeA = recordA ? recordA.created_at : "";
    const timeB = recordB ? recordB.created_at : "";
    if (timeA === timeB) {
      return b - a;
    }
    return timeA < timeB ? 1 : -1;
  };
  pinned.sort(compareByCreatedAtDesc);
  unpinned.sort(compareByCreatedAtDesc);
  return [...pinned, ...unpinned];
}

/**
 * Reorder a `selectedId` / `ids` pair after a pin toggle.
 *
 * When the affected entry moves from the unpinned group to the
 * pinned group, the selection MUST follow it so the user keeps
 * typing on the same row. When the entry moves the other way the
 * selection moves with it for symmetry.
 *
 * Returns the new index (or the original `selectedIndex` if the
 * id is no longer visible) and the new id list. The Svelte layer
 * re-applies both.
 */
export function preserveSelectionAfterReorder(
  ids: number[],
  currentIndex: number,
): { ids: number[]; selectedIndex: number } {
  if (ids.length === 0) {
    return { ids, selectedIndex: 0 };
  }
  if (currentIndex < 0 || currentIndex >= ids.length) {
    return { ids, selectedIndex: 0 };
  }
  return { ids, selectedIndex: currentIndex };
}

/**
 * Resolve the new selection index when the user clicks a row.
 *
 * Clicking a row MUST turn that row into the selected result so the
 * keyboard shortcut that follows (`Enter`/`Shift+Enter`) targets the
 * row the user just clicked. The helper returns the index of
 * `clickedEntryId` inside `resultIds`, clamped to the valid range so
 * an empty result list still produces a deterministic, non-negative
 * index the Svelte layer can apply without further bookkeeping.
 *
 * The function is total: an empty list yields `0`, an unknown id
 * yields the original index (clamped) so a stale click does not
 * silently land on a different row.
 */
export function selectedIndexForClick(
  resultIds: number[],
  clickedEntryId: number,
  currentIndex: number,
): number {
  if (resultIds.length === 0) {
    return 0;
  }
  const clicked = resultIds.indexOf(clickedEntryId);
  if (clicked >= 0) {
    return clicked;
  }
  if (currentIndex < 0 || currentIndex >= resultIds.length) {
    return 0;
  }
  return currentIndex;
}

/**
 * Resolve the selection index when the visible result set changes.
 *
 * The Quick Paste list can re-render after a search, a favourite
 * toggle or an explicit refresh; the selected entry MUST follow the
 * stable entry id so the user keeps typing on the same row. When the
 * id is no longer visible the helper falls back to a clamped index
 * (preferring the previous index, then `0`) so the keyboard flow
 * never lands on a stale row.
 *
 * The helper is pure: it never mutates `resultIds` and never touches
 * the DOM. The Svelte layer re-applies the returned index.
 */
export function selectedIndexForEntryId(
  resultIds: number[],
  selectedEntryId: number | null,
  fallbackIndex: number,
): number {
  if (resultIds.length === 0) {
    return 0;
  }
  if (selectedEntryId !== null) {
    const index = resultIds.indexOf(selectedEntryId);
    if (index >= 0) {
      return index;
    }
  }
  if (fallbackIndex >= 0 && fallbackIndex < resultIds.length) {
    return fallbackIndex;
  }
  return 0;
}

/**
 * Minimal contract a row element exposes for autoscroll. Defined
 * here so the helper can be exercised by `node:test` without a real
 * DOM polyfill — production rows already satisfy it through
 * `Element.scrollIntoView`.
 */
export interface ScrollableRowElement {
  scrollIntoView: (options?: ScrollIntoViewOptions) => void;
}

/**
 * Resolve the next selected index for an ArrowUp / ArrowDown keypress.
 *
 * The contract is **clamp** rather than wrap: pressing ArrowDown on
 * the last visible row MUST stay on that row instead of teleporting
 * back to the first. Wrapping was the previous baseline but it
 * silently moved the keyboard focus past the row the user was
 * reading, which surfaced as a regression during a second round of
 * manual QA on `preview-interaction-regressions`. The wrap-around
 * shortcuts (`Home` / `End`) are still the canonical way to jump
 * across the list; `ArrowDown` / `ArrowUp` are the focused
 * "one-row-at-a-time" navigation primitives.
 *
 * The helper is total: an empty `resultIds` collapses to `-1` so
 * the caller can detect "no row to select" without a second check,
 * and an out-of-range `currentIndex` (e.g. a stale closure from a
 * previous render) is normalised to `0` before applying the
 * delta. The function never mutates any input and never touches
 * the DOM.
 */
export function clampedSelectedIndex(
  resultIds: readonly number[],
  currentIndex: number,
  delta: number,
): number {
  const total = resultIds.length;
  if (total === 0) {
    return -1;
  }
  const safeCurrent =
    currentIndex < 0 || currentIndex >= total ? 0 : currentIndex;
  return Math.max(0, Math.min(total - 1, safeCurrent + delta));
}

/**
 * Minimal contract the list container exposes. The autoscroll helper
 * only needs to know the container so a unit test can assert that
 * the helper never invokes any DOM method on the container itself.
 */
export interface QuickPasteScrollContainer {
  readonly length: number;
}

/**
 * Scroll the selected row into view inside the Quick Paste list
 * container. The helper:
 *
 * - is a no-op when the index is out of range or the row is missing;
 * - only calls `scrollIntoView` on the row element, never on the
 *   container, so the desktop / window scroll surface cannot move;
 * - always passes `block: "nearest"` so the helper never forces the
 *   row to the top or bottom of the visible viewport unless the user
 *   navigates past the current edge.
 *
 * Production rows satisfy [`ScrollableRowElement`] natively
 * (`Element.scrollIntoView`); tests can pass a stub.
 */
export function scrollSelectedRowIntoView(
  container: QuickPasteScrollContainer,
  rows: readonly (ScrollableRowElement | null | undefined)[],
  selectedIndex: number,
): void {
  if (container.length === 0) {
    return;
  }
  if (selectedIndex < 0 || selectedIndex >= rows.length) {
    return;
  }
  const row = rows[selectedIndex];
  if (!row) {
    return;
  }
  row.scrollIntoView({ block: "nearest" });
}
