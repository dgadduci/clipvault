// Pure helper the `peer-image-import` rail drives to combine
// the metadata-only text + image endpoints into a single
// newest-first list. The helper is the single source of truth for
// the contract the design + spec pin:
//
//   - each endpoint returns up to MAX_PER_STREAM_ROWS rows;
//   - the rail renders up to MAX_COMBINED_PAGE_ROWS combined rows;
//   - rows that came back from a page but were not displayed
//     because of the combined cap live in per-stream buffers so
//     they are promoted before another host request;
//   - text and image cursors advance independently so a stream
//     that runs out first does not force the other to skip;
//   - an exhausted stream is skipped via a `null` cursor on
//     `Next` so the runtime never re-requests the first page from
//     an exhausted stream while the other still has rows;
//   - the first page / `Anterior` / retry after `invalid_cursor`
//     path uses the empty cursor (`""`), which the bridge
//     collapses into "start from the beginning", so both
//     endpoints are always dialled instead of being silently
//     skipped;
//   - the rail is **paginada con ventana de 50**: cada página
//     visible contiene como máximo MAX_COMBINED_PAGE_ROWS filas y
//     `Siguiente` consume los buffers antes de pedir una nueva
//     página al host. Las filas visibles anteriores NO vuelven a
//     entrar al cómputo en una petición de tipo `append`; eso era
//     la regresión que la última revisión detectó, porque hacía
//     que las filas que la primera respuesta había dejado en el
//     buffer nunca llegaran a la ventana.
//
// The helper is intentionally framework-agnostic so the rail and
// the regression suite exercise the same code path: a future
// change that re-implements the merge locally would diverge
// from the production logic the moment Svelte's reactivity kicks
// in. Importing the helper from both surfaces keeps the wire
// contract under test in lockstep with the renderer.

import type {
  PeerHistoryBrowseResponse,
  PeerHistoryRow,
  PeerImageBrowseResponse,
  PeerImageBrowseRow,
} from "../types";

/**
 * Hard cap the rail applies to a single combined page. The cap
 * matches the documented per-stream cap so the merged list never
 * exceeds what a host can return for either endpoint, even when
 * the rows interleave. The constants stay here (and not in the
 * bridge / Rust module) so the renderer keeps the single source
 * of truth for the combined budget.
 */
export const MAX_COMBINED_PAGE_ROWS = 50;

/**
 * Per-stream cap the helper assumes the bridge returns for a
 * single page. Mirrors the Rust constant
 * `HISTORY_MAX_PAGE_ROWS` / `IMAGE_HISTORY_MAX_PAGE_ROWS` so
 * the helper never has to trim rows it received below the wire
 * contract.
 */
export const MAX_PER_STREAM_ROWS = 50;

/**
 * Row union the rail renders. The bridge splits text and image
 * endpoints so the rail can fetch them in parallel and merge the
 * responses into a single newest-first stream.
 */
export type RemoteRailRow =
  | { kind: "text"; row: PeerHistoryRow }
  | { kind: "image"; row: PeerImageBrowseRow };

export type TextOutcome =
  | { kind: "text"; response: PeerHistoryBrowseResponse }
  | { kind: "text-error"; err: unknown }
  | { kind: "text-skip" };

export type ImageOutcome =
  | { kind: "image"; response: PeerImageBrowseResponse }
  | { kind: "image-error"; err: unknown }
  | { kind: "image-skip" };

export interface PageState {
  rows: RemoteRailRow[];
  cursor: string;
  imageCursor: string;
  /** Rows the text endpoint returned but the combined cap hid. */
  textBuffer: RemoteRailRow[];
  /** Rows the image endpoint returned but the combined cap hid. */
  imageBuffer: RemoteRailRow[];
  snapshotId: string;
  error: string | null;
  /** True when both endpoints reported no more pages AND both buffers are empty. */
  exhausted: boolean;
  /** True when the helper wants the rail to re-issue the first page. */
  requestInvalidCursor: boolean;
  loading: boolean;
}

export interface ApplyOptions {
  /**
   * `false` for the first page, `Anterior`, and retries after an
   * `invalid_cursor` outcome; `true` for the `Siguiente` request
   * after all existing per-stream buffers have been promoted.
   */
  append: boolean;
}

/**
 * Sort the supplied rows newest-first by the wire timestamp.
 * The helper is the single source of truth for the cross-stream
 * ordering and stays decoupled from any Svelte runtime so the
 * tests can drive it standalone.
 */
function sortNewestFirst(rows: RemoteRailRow[]): RemoteRailRow[] {
  return rows.slice().sort((a, b) => {
    const ta = a.row.created_at;
    const tb = b.row.created_at;
    return ta < tb ? 1 : ta > tb ? -1 : 0;
  });
}

/**
 * Deduplicate the supplied rows by `remote_entry_id` while
 * preserving the order of the first occurrence. The helper is
 * intentionally a pure projection so the caller can choose how to
 * feed the rows in (newest-first, oldest-first, …).
 */
function dedupeByRemoteEntryId(rows: RemoteRailRow[]): RemoteRailRow[] {
  const seen = new Set<string>();
  const out: RemoteRailRow[] = [];
  for (const row of rows) {
    const id = row.row.remote_entry_id;
    if (seen.has(id)) continue;
    seen.add(id);
    out.push(row);
  }
  return out;
}

/**
 * Split the supplied hidden rows back into the per-stream
 * buffers the page state carries. The helper assumes the input is
 * already sorted + deduped; the rail only ever splits the rows
 * the `MAX_COMBINED_PAGE_ROWS` cap hid.
 */
function splitByStream(
  rows: RemoteRailRow[],
): { textBuffer: RemoteRailRow[]; imageBuffer: RemoteRailRow[] } {
  const textBuffer: RemoteRailRow[] = [];
  const imageBuffer: RemoteRailRow[] = [];
  for (const row of rows) {
    if (row.kind === "text") {
      textBuffer.push(row);
    } else {
      imageBuffer.push(row);
    }
  }
  return { textBuffer, imageBuffer };
}

/**
 * Combine the responses from the text + image endpoints into a
 * single merged page. The helper is the single source of truth
 * for the merge / dedupe / sort / cap contract: a regression that
 * re-implements the logic locally would diverge the moment
 * Svelte's reactivity mutates one of the inputs.
 *
 * The `append = true` path corresponds to the `Siguiente` event.
 * The helper intentionally does NOT include the previously visible
 * rows: those rows were already shown to the user. The new visible
 * window is built exclusively from the rows the host returned in
 * the latest response. The rail only triggers a new host request
 * after promoting all existing buffers, so the previous buffers
 * are empty by construction and need not be re-injected. The
 * combined window never
 * exceeds `MAX_COMBINED_PAGE_ROWS` and the rest goes back to the
 * per-stream buffers for the next advance.
 */
export function applyResponses(
  textOutcome: TextOutcome,
  imageOutcome: ImageOutcome,
  previous: PageState,
  options: ApplyOptions,
): PageState {
  // `options` documents the rail mode (`append: true` for the
  // `Siguiente` path that re-issues the request when both
  // per-stream buffers are empty, `append: false` for the
  // first / `Anterior` / retry-after-`invalid_cursor` paths).
  // The helper no longer branches on it: the new visible
  // window is built exclusively from the rows the host
  // returned in the latest response, never from
  // `previous.rows` or `previous.{textBuffer,imageBuffer}`
  // (those rows have already been shown to the user).
  void options;
  // Surface a typed failure from whichever endpoint reported it
  // first. The renderer collapses the message into the copy it
  // shows next to the retry button.
  if (textOutcome.kind === "text-error") {
    return {
      ...previous,
      loading: false,
      error: `text:${formatError(textOutcome.err)}`,
    };
  }
  if (imageOutcome.kind === "image-error") {
    return {
      ...previous,
      loading: false,
      error: `image:${formatError(imageOutcome.err)}`,
    };
  }

  const textResponse =
    textOutcome.kind === "text" ? textOutcome.response : null;
  const imageResponse =
    imageOutcome.kind === "image" ? imageOutcome.response : null;

  // Either endpoint collapsing to `invalid_cursor` surfaces the
  // typed reason and asks the rail to retry from the first page.
  // The retry MUST use the empty cursor (decoded by the bridge
  // as "start from the beginning") instead of `null`; passing
  // `null` would skip the matching endpoint and leave the rail
  // blank.
  if (textResponse !== null && textResponse.kind === "invalid_cursor") {
    return {
      ...previous,
      cursor: "",
      rows: [],
      textBuffer: [],
      imageBuffer: [],
      loading: false,
      error: "invalid_cursor:text",
      exhausted: false,
      requestInvalidCursor: true,
    };
  }
  if (imageResponse !== null && imageResponse.kind === "invalid_cursor") {
    return {
      ...previous,
      imageCursor: "",
      loading: false,
      error: "invalid_cursor:image",
      exhausted: false,
      requestInvalidCursor: true,
    };
  }

  // Text-side `peer_unavailable` clears the page entirely so the
  // caller can surface the no-capturas copy without leaking the
  // cached rows from the previous attempt.
  if (textResponse !== null && textResponse.kind === "peer_unavailable") {
    return {
      rows: [],
      cursor: "",
      imageCursor: "",
      textBuffer: [],
      imageBuffer: [],
      snapshotId: "",
      loading: false,
      error: `peer_unavailable:${textResponse.reason}`,
      exhausted: true,
      requestInvalidCursor: false,
    };
  }
  if (
    textResponse !== null &&
    textResponse.kind === "transport_unavailable"
  ) {
    return {
      ...previous,
      loading: false,
      error: `transport_unavailable:${textResponse.reason}`,
    };
  }
  if (
    imageResponse !== null &&
    imageResponse.kind === "transport_unavailable"
  ) {
    return {
      ...previous,
      loading: false,
      error: `transport_unavailable:image:${imageResponse.reason}`,
    };
  }
  if (imageResponse !== null && imageResponse.kind === "peer_unavailable") {
    return {
      ...previous,
      imageCursor: "",
      loading: false,
      error: `peer_unavailable:image:${imageResponse.reason}`,
    };
  }

  // Either endpoint collapsing to `ok` exposes the bounded page
  // the host minted. We translate each row into the
  // [`RemoteRailRow`] union the rail renders, then merge +
  // dedupe + sort + cap the combined set.
  const newTextRows: RemoteRailRow[] =
    textResponse !== null && textResponse.kind === "ok"
      ? textResponse.rows.map((row) => ({ kind: "text" as const, row }))
      : [];
  const newImageRows: RemoteRailRow[] =
    imageResponse !== null && imageResponse.kind === "ok"
      ? imageResponse.rows.map((row) => ({ kind: "image" as const, row }))
      : [];

  // The new visible window is built exclusively from the rows the
  // host returned in the latest response. The previous visible
  // rows are NOT re-injected: the rail is paginated, so the user
  // already saw them and a regression that re-injected them was
  // precisely what made the buffer rows invisible to the user.
  // The previous buffers are also NOT re-injected: the rail only
  // fires a `Siguiente` request when both buffers are empty, so
  // by construction the request that produced `newTextRows` and
  // `newImageRows` was triggered with empty buffers.
  const combined = sortNewestFirst(
    dedupeByRemoteEntryId(newTextRows.concat(newImageRows)),
  );
  const visible: RemoteRailRow[] = combined.slice(0, MAX_COMBINED_PAGE_ROWS);
  const hidden: RemoteRailRow[] = combined.slice(MAX_COMBINED_PAGE_ROWS);
  const { textBuffer, imageBuffer } = splitByStream(hidden);

  const nextCursor =
    textResponse !== null && textResponse.kind === "ok"
      ? textResponse.next_cursor
      : previous.cursor;
  const nextImageCursor =
    imageResponse !== null && imageResponse.kind === "ok"
      ? imageResponse.next_cursor
      : previous.imageCursor;
  const snapshotId =
    textResponse !== null && textResponse.kind === "ok"
      ? textResponse.snapshot_id
      : previous.snapshotId;

  // `exhausted` flips true only when BOTH streams reported no
  // more pages AND no buffered rows remain to display. The
  // pending-buffer case is the cornerstone of the spec scenario
  // "Siguiente with exhausted cursors and a non-empty buffer":
  // the user keeps advancing until the buffers drain.
  const textExhausted =
    textResponse === null ||
    nextCursor.length === 0;
  const imageExhausted =
    imageResponse === null ||
    nextImageCursor.length === 0;
  const noBufferedRows = textBuffer.length === 0 && imageBuffer.length === 0;
  const exhausted = textExhausted && imageExhausted && noBufferedRows;

  return {
    rows: visible,
    cursor: nextCursor,
    imageCursor: nextImageCursor,
    textBuffer,
    imageBuffer,
    snapshotId,
    error: null,
    loading: false,
    exhausted,
    requestInvalidCursor: false,
  };
}

function formatError(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/**
 * Promote the per-stream buffered rows to the visible window
 * without making a network request. The helper is the rail's
 * primary path for `Siguiente` when the previous response left
 * rows hidden in the buffers: those rows are already known to the
 * client, so promoting them is a synchronous operation the user
 * can trigger repeatedly until both buffers are empty.
 *
 * The promotion keeps the contract the design documents:
 *
 *   - the rows from `textBuffer` and `imageBuffer` are merged,
 *     deduped (the per-stream buffers already carry only
 *     unique `remote_entry_id`s, but the helper defends in depth)
 *     and sorted newest-first;
 *   - the combined window never exceeds `MAX_COMBINED_PAGE_ROWS`;
 *   - any rows that do not fit return to the per-stream buffers
 *     so the next advance can pick them up;
 *   - the cursors stay untouched: the rail only triggers the
 *     helper when both buffers are empty, so promoting a
 *     non-empty buffer is guaranteed not to skip any host page;
 *   - `exhausted` only flips true when both cursors are empty
 *     AND both buffers are empty.
 *
 * The helper is intentionally pure: a future rail that picks
 * up the promotion in a `reactive` block will always see the same
 * output the unit tests assert.
 */
export function promoteBuffers(previous: PageState): PageState {
  const combined = sortNewestFirst(
    dedupeByRemoteEntryId(
      previous.textBuffer.concat(previous.imageBuffer),
    ),
  );
  const visible: RemoteRailRow[] = combined.slice(0, MAX_COMBINED_PAGE_ROWS);
  const hidden: RemoteRailRow[] = combined.slice(MAX_COMBINED_PAGE_ROWS);
  const { textBuffer, imageBuffer } = splitByStream(hidden);
  const textExhausted = previous.cursor.length === 0;
  const imageExhausted = previous.imageCursor.length === 0;
  const noBufferedRows = textBuffer.length === 0 && imageBuffer.length === 0;
  return {
    rows: visible,
    cursor: previous.cursor,
    imageCursor: previous.imageCursor,
    textBuffer,
    imageBuffer,
    snapshotId: previous.snapshotId,
    error: previous.error,
    loading: false,
    exhausted: textExhausted && imageExhausted && noBufferedRows,
    requestInvalidCursor: false,
  };
}

/**
 * Helper the rail uses to compute the cursors it forwards to each
 * endpoint for the `Siguiente` request. The helper consults the
 * per-stream state to decide which cursor to forward verbatim and
 * which stream to skip via `null` so the runtime never re-requests
 * the first page from an exhausted stream.
 */
export function pickNextCursors(state: PageState): {
  text: string | null;
  image: string | null;
} {
  return {
    text: state.cursor.length > 0 ? state.cursor : null,
    image: state.imageCursor.length > 0 ? state.imageCursor : null,
  };
}

/**
 * Initial / retry helper. The first-page contract collapses the
 * empty cursor into "start from the beginning" so both endpoints
 * are always dialled. Passing `null` would skip the matching
 * endpoint and leave the rail blank.
 */
export const INITIAL_CURSORS: Readonly<{ text: ""; image: "" }> = Object.freeze({
  text: "",
  image: "",
});
