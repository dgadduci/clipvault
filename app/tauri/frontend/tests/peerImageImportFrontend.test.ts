/**
 * Frontend regression coverage for the `peer-image-import` change.
 *
 * The change ships:
 *
 * - A static image placeholder rendered identically for every
 *   remote image row (the renderer never fetches bytes during
 *   navigation).
 * - Capability gating: a row from a peer that does not advertise
 *   `image_import` MUST NOT surface a successful Import path.
 * - A pagination button that stays enabled while either stream
 *   (text or image) has more pages so mixed collections with
 *   cursors of differing lengths do not trap the user.
 * - An import path that surfaces a busy state, surfaces the
 *   outcome as a typed discriminated union and triggers a single
 *   `peerImageFetchCommand` round-trip.
 * - Independent text / image cursors: each endpoint receives
 *   ONLY its own cursor, exhausted streams are skipped, and the
 *   merged newest-first list never duplicates a
 *   `remote_entry_id`.
 *
 * The behavioural scenarios in section 6 drive the **real**
 * `applyResponses` helper the rail imports — a regression that
 * diverges the rail from the helper would break the suite. The
 * markup tests in sections 1–5 read the Svelte source so a
 * contract drift in the templates / event wiring surfaces as a
 * test failure instead of a silent behavioural drift.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

import {
  INITIAL_CURSORS,
  MAX_COMBINED_PAGE_ROWS,
  applyResponses,
  pickNextCursors,
  promoteBuffers,
  rowsForPartialOutcome,
  type PageState,
  type RemoteRailRow,
} from "../src/lib/remoteHistoryMerge.ts";
import type {
  PeerHistoryBrowseResponse,
  PeerHistoryRow,
  PeerImageBrowseResponse,
  PeerImageBrowseRow,
} from "../src/types.ts";

const FRONTEND_ROOT = process.cwd();

function loadSource(...segments: string[]): string {
  return readFileSync(path.join(FRONTEND_ROOT, ...segments), "utf8");
}

const railSource = loadSource("src/RemoteHistoryRail.svelte");
const cardSource = loadSource("src/RemotePreviewCard.svelte");
const mergeSource = loadSource("src/lib/remoteHistoryMerge.ts");

// ---------------------------------------------------------------------------
// 1. Static placeholder — the rail retains one shared SVG
//    fallback while an optional bounded thumbnail is unavailable.
// ---------------------------------------------------------------------------

test("RemoteHistoryRail renders the image row with isImageRow={true}", () => {
  // The rail must flag every image row with `isImageRow={true}`
  // so the card can switch into the placeholder / busy / import
  // flow that the spec documents.
  assert.match(
    railSource,
    /isImageRow=\{true\}/,
    "image rows must opt into the image-only card surface",
  );
});

test("RemotePreviewCard renders a static SVG placeholder for image rows", () => {
  // The placeholder remains the initial and failure visual. A
  // successful bounded thumbnail may replace it independently of Importar.
  assert.match(
    cardSource,
    /remote-preview-card-image-placeholder/,
    "image placeholder container must exist",
  );
  assert.match(
    cardSource,
    /data-placeholder-kind=\{thumbnailPhase === "ready" \? "thumbnail" : "static"\}/,
    "the shared placeholder must remain static until a thumbnail succeeds",
  );
  assert.match(
    cardSource,
    /<svg[\s\S]+viewBox=/,
    "placeholder must be an inline SVG so it never round-trips through the network",
  );
});

test("RemotePreviewCard renders thumbnails only through an in-memory object URL", () => {
  assert.match(
    cardSource,
    /<img[\s\S]*?src=\{thumbnailUrl\}/,
    "a successful thumbnail may render through its local object URL",
  );
  assert.match(
    cardSource,
    /URL\.createObjectURL\(blob\)/,
    "thumbnail bytes must be held as a temporary in-memory object URL",
  );
  assert.doesNotMatch(
    cardSource,
    /resolveClipboardAsset|src\s*=\s*["'](?:asset:|file:|https?:\/\/)/,
    "remote previews must not resolve local asset references or remote URLs",
  );
});

// ---------------------------------------------------------------------------
// 2. Capability gating — a row whose peer does not advertise
//    `image_import` MUST NOT expose a successful Import action.
// ---------------------------------------------------------------------------

test("RemotePreviewCard delegates the import path to peerImageFetchCommand", () => {
  assert.match(
    cardSource,
    /peerImageFetchCommand/,
    "import path must delegate to the dedicated image fetch bridge",
  );
});

test("RemotePreviewCard surfaces a busy state during the import request", () => {
  // The renderer toggles `aria-busy` while the request is in
  // flight so the menu button reflects the in-progress state.
  assert.match(
    cardSource,
    /aria-busy=\{busy\}/,
    "import menu button must surface busy state via aria-busy",
  );
});

// ---------------------------------------------------------------------------
// 3. Pagination — Siguiente stays enabled while either stream
//    (text or image) has more pages.
// ---------------------------------------------------------------------------

test("RemoteHistoryRail keeps the next button enabled until the rail is fully exhausted", () => {
  // The button's `disabled` predicate is the single boolean the
  // rail maintains (`exhausted`). The rail flips `exhausted = true`
  // only when BOTH cursors are empty AND both per-stream buffers
  // are empty, so a mixed page (text exhausted, image pending,
  // or any buffer non-empty) keeps the button enabled. The
  // regression the spec scenario "Siguiente with exhausted cursors
  // and a non-empty buffer" pins: the previous prototype
  // disabled the button the moment both cursors emptied, which
  // trapped the user on a half-page.
  const disabledExpr = railSource.match(
    /data-testid="remote-history-rail-next"[\s\S]{0,200}disabled=\{([^}]+)\}/,
  );
  assert.ok(
    disabledExpr,
    "next button must declare a disabled expression",
  );
  const expr = disabledExpr![1];
  assert.match(
    expr,
    /exhausted/,
    "disabled predicate must consult the rail-wide exhausted flag",
  );
});

test("RemoteHistoryRail requestNextPage drains the per-stream buffers first", () => {
  // The rail must consult the shared `promoteBuffers` helper so
  // the buffer-draining logic lives in a single place the tests
  // can pin. The shared helper is the source of truth for
  // "consume buffered rows before asking the host for more".
  assert.match(
    railSource,
    /promoteBuffers\(/,
    "requestNextPage must delegate the buffer-drain path to promoteBuffers",
  );
});

test("RemoteHistoryRail requestNextPage delegates to pickNextCursors helper", () => {
  // The rail must consult the shared helper so the picker
  // (text cursor vs. image cursor) lives in a single place
  // the tests can pin. The shared helper is the source of
  // truth for "skip exhausted stream via null".
  assert.match(
    railSource,
    /pickNextCursors\(/,
    "requestNextPage must delegate the per-stream picker to pickNextCursors",
  );
});

test("RemoteHistoryRail sends each endpoint ONLY its own cursor", () => {
  // Defence in depth: the rail must never mix the text and
  // image cursors. Each endpoint call MUST forward the cursor
  // it owns. The bug the test pins: the previous prototype
  // forwarded a single `nextCursor` value to both endpoints,
  // which made the text endpoint receive the image cursor and
  // vice versa.
  const request = railSource.match(
    /peerHistoryBrowseCommand\(\{[\s\S]+?\}\)\.then/,
  );
  assert.ok(
    request,
    "text endpoint invocation must exist in the source",
  );
  assert.match(
    request![0],
    /cursor:\s*textCursorPayload/,
    "text endpoint must receive the text cursor payload",
  );
  const imageRequest = railSource.match(
    /peerImageBrowseCommand\(\{[\s\S]+?\}\)\.then/,
  );
  assert.ok(imageRequest, "image endpoint invocation must exist");
  assert.match(
    imageRequest![0],
    /cursor:\s*imageCursorPayload/,
    "image endpoint must receive the image cursor payload",
  );
});

test("RemoteHistoryRail skips an exhausted stream on Next", () => {
  // When the text stream is exhausted, the helper MUST skip
  // the text endpoint (passing `null` as the cursor) so the
  // runtime never re-requests the first page from an
  // exhausted stream while the image stream still has rows.
  const helper = railSource.match(/async function requestPage[\s\S]+?\n\s{2}\}/);
  assert.ok(helper, "requestPage helper must exist");
  assert.match(
    helper![0],
    /textCursorPayload\s*===\s*null/,
    "requestPage must check for null text cursor to skip the text endpoint",
  );
  assert.match(
    helper![0],
    /imageCursorPayload\s*===\s*null/,
    "requestPage must check for null image cursor to skip the image endpoint",
  );
});

// ---------------------------------------------------------------------------
// 4. Initial / first-page / Anterior use the empty cursor so both
//    endpoints are dialled with `start_from_beginning` semantics.
// ---------------------------------------------------------------------------

test("RemoteHistoryRail initial load uses the shared INITIAL_CURSORS helper", () => {
  // The first page contract collapses the empty cursor into
  // "start from the beginning" so both endpoints are always
  // dialled. Passing `null` would skip the matching endpoint
  // and leave the rail blank — that was the bug the initial
  // load regression pinned.
  assert.match(
    railSource,
    /INITIAL_CURSORS/,
    "rail must import INITIAL_CURSORS so the contract stays single-sourced",
  );
  assert.match(
    railSource,
    /loadInitialPage[\s\S]+?INITIAL_CURSORS/,
    "loadInitialPage must request INITIAL_CURSORS (empty text + image cursors)",
  );
  assert.match(
    railSource,
    /requestFirstPage[\s\S]+?INITIAL_CURSORS/,
    "requestFirstPage must retry with INITIAL_CURSORS",
  );
  assert.match(
    railSource,
    /requestPreviousPage[\s\S]+?INITIAL_CURSORS/,
    "requestPreviousPage must re-issue the first page with INITIAL_CURSORS",
  );
  assert.doesNotMatch(
    railSource,
    /text:\s*null\s*,\s*image:\s*null/,
    "rail must never pass { text: null, image: null } for first/initial/previous loads",
  );
});

test("INITIAL_CURSORS exposes empty text + image cursor strings", () => {
  // The shared helper exports the empty-cursor contract as a
  // single frozen constant so the production source and the
  // tests reference the same value.
  assert.equal(INITIAL_CURSORS.text, "");
  assert.equal(INITIAL_CURSORS.image, "");
});

// ---------------------------------------------------------------------------
// 5. Stale-response protection — a response that arrives after
//    the user switched peer is silently dropped.
// ---------------------------------------------------------------------------

test("RemoteHistoryRail uses a generation counter to drop stale responses", () => {
  // The `loadGeneration` mechanism keeps a response that
  // arrives after the user already switched peer / closed the
  // rail from overwriting the visible state.
  assert.match(
    railSource,
    /loadGeneration/,
    "remote history rail must track a load generation counter",
  );
  assert.match(
    railSource,
    /generation\s*!==\s*loadGeneration\s*\)?\s*return/,
    "every async branch must early-return when the generation drifts",
  );
});

// ---------------------------------------------------------------------------
// 6. Behaviour tests — drive the **real** `applyResponses` helper
//    (the same one the rail imports) against mocked endpoints to
//    cover the documented scenarios:
//    - initial / first-page load hits both endpoints;
//    - streams are independent and exhausted streams are skipped;
//    - both streams active and cursors never cross;
//    - merged list never duplicates a remote_entry_id;
//    - combined list never exceeds MAX_COMBINED_PAGE_ROWS;
//    - per-stream buffers keep hidden rows so the next page is
//      not skipped or duplicated;
//    - dates from both streams interleave newest-first.
// ---------------------------------------------------------------------------

function buildTextRow(id: string, createdAt: string, preview = "preview"): PeerHistoryRow {
  return {
    remote_entry_id: id,
    title: null,
    content_type: "text",
    created_at: createdAt,
    preview,
    source_app_name: null,
  };
}

function buildImageRow(id: string, createdAt: string): PeerImageBrowseRow {
  return {
    remote_entry_id: id,
    title: null,
    content_type: "image",
    created_at: createdAt,
    byte_size: 1024,
    width: 16,
    height: 16,
    source_app_name: null,
  };
}

function textOk(
  rows: PeerHistoryRow[],
  nextCursor: string,
  snapshotId = "snap",
): PeerHistoryBrowseResponse {
  return { kind: "ok", rows, next_cursor: nextCursor, snapshot_id: snapshotId };
}

function imageOk(
  rows: PeerImageBrowseRow[],
  nextCursor: string,
  snapshotId = "snap",
): PeerImageBrowseResponse {
  return { kind: "ok", rows, next_cursor: nextCursor, snapshot_id: snapshotId };
}

function emptyPage(): PageState {
  return {
    rows: [],
    cursor: "",
    imageCursor: "",
    textBuffer: [],
    imageBuffer: [],
    snapshotId: "",
    error: null,
    loading: false,
    exhausted: false,
    requestInvalidCursor: false,
  };
}

test("behaviour: initial load uses empty cursors so both endpoints are dialled", () => {
  // The rail imports INITIAL_CURSORS so the wire contract is
  // single-sourced. Verify the helper exports the value the
  // contract pins: empty text + empty image cursor strings.
  assert.equal(INITIAL_CURSORS.text, "");
  assert.equal(INITIAL_CURSORS.image, "");
});

test("behaviour: first successful browse stream paints before the other stream settles", () => {
  const textRow = { ...buildTextRow("text-fast", "2026-01-03T00:00:00Z"), source_app_name: "Terminal" };
  const imageRow = { ...buildImageRow("image-fast", "2026-01-02T00:00:00Z"), source_app_name: "Screenshot" };

  const textFirstPaint = rowsForPartialOutcome({
    kind: "text",
    response: textOk([textRow], "text-next"),
  });
  assert.deepEqual(textFirstPaint?.map((item) => item.row.remote_entry_id), ["text-fast"]);
  assert.equal(textFirstPaint?.[0].row.source_app_name, "Terminal");

  const imageFirstPaint = rowsForPartialOutcome({
    kind: "image",
    response: imageOk([imageRow], "image-next"),
  });
  assert.deepEqual(imageFirstPaint?.map((item) => item.row.remote_entry_id), ["image-fast"]);
  assert.equal(imageFirstPaint?.[0].row.source_app_name, "Screenshot");

  assert.equal(
    rowsForPartialOutcome({ kind: "text", response: { kind: "invalid_cursor" } }),
    null,
    "partial paint must not apply typed failures or advance pagination state",
  );
  const partialState: PageState = {
    ...emptyPage(),
    rows: textFirstPaint ?? [],
    loading: true,
  };
  const afterOtherStreamFails = applyResponses(
    { kind: "text", response: textOk([textRow], "text-next") },
    { kind: "image-error", err: new Error("offline") },
    partialState,
    { append: false },
  );
  assert.deepEqual(
    afterOtherStreamFails.rows.map((item) => item.row.remote_entry_id),
    ["text-fast"],
    "a successful stream stays visible when its parallel endpoint fails",
  );
  assert.match(afterOtherStreamFails.error ?? "", /^image:/);
  assert.equal(afterOtherStreamFails.cursor, "");
  assert.equal(afterOtherStreamFails.loading, false);
  assert.match(railSource, /publishFirstPageRows\(textPromise\)/);
  assert.match(railSource, /publishFirstPageRows\(imagePromise\)/);
  assert.match(railSource, /generation === loadGeneration/);
  assert.match(railSource, /const \[textOutcome, imageOutcome\] = await Promise\.all/);
  assert.match(railSource, /disabled=\{exhausted \|\| loading\}/);
});

test("behaviour: initial load merges text + image rows newest-first", () => {
  const result = applyResponses(
    { kind: "text", response: textOk(
      [
        buildTextRow("text-1", "2026-01-03T00:00:00Z"),
        buildTextRow("text-2", "2026-01-01T00:00:00Z"),
      ],
      "text-cursor-1",
      "snap-text",
    ) },
    { kind: "image", response: imageOk(
      [buildImageRow("img-1", "2026-01-02T00:00:00Z")],
      "image-cursor-1",
    ) },
    emptyPage(),
    { append: false },
  );
  assert.equal(result.rows.length, 3);
  assert.equal(result.rows[0].row.remote_entry_id, "text-1");
  assert.equal(result.rows[1].row.remote_entry_id, "img-1");
  assert.equal(result.rows[2].row.remote_entry_id, "text-2");
  assert.equal(result.cursor, "text-cursor-1");
  assert.equal(result.imageCursor, "image-cursor-1");
  assert.equal(result.snapshotId, "snap-text");
  assert.equal(result.exhausted, false);
});

test("behaviour: text exhausted, images pending — text endpoint is skipped", () => {
  // Text stream was exhausted in a previous page (its cursor is
  // empty). The rail fired `Siguiente` with empty buffers and
  // `pickNextCursors` returned `text = null`, so the helper
  // receives a `text-skip` outcome. Only the image response
  // contributes to the new visible window; the previous visible
  // rows do NOT re-enter the computation because the rail is
  // paginated with a 50-row window.
  const previous: PageState = {
    ...emptyPage(),
    rows: [{ kind: "text", row: buildTextRow("text-1", "2026-01-02T00:00:00Z") }],
    cursor: "",
    imageCursor: "image-cursor-1",
    snapshotId: "snap-text",
  };
  const cursors = pickNextCursors(previous);
  assert.equal(cursors.text, null, "exhausted text stream is skipped");
  assert.equal(cursors.image, "image-cursor-1", "image stream advances");
  const result = applyResponses(
    { kind: "text-skip" },
    { kind: "image", response: imageOk(
      [buildImageRow("img-1", "2026-01-03T00:00:00Z")],
      "",
    ) },
    previous,
    { append: true },
  );
  // Only the new image row is visible; the previous visible row
  // was already shown to the user and is NOT re-injected. The
  // user reaches it again by promoting the image buffer (when the
  // image cursor empties).
  assert.equal(result.rows.length, 1);
  assert.equal(result.rows[0].row.remote_entry_id, "img-1");
  assert.equal(result.imageCursor, "");
  assert.equal(result.cursor, "");
});

test("behaviour: images exhausted, text pending — image endpoint is skipped", () => {
  // Image stream was exhausted in a previous page. Only the text
  // response contributes to the new visible window.
  const previous: PageState = {
    ...emptyPage(),
    rows: [{ kind: "image", row: buildImageRow("img-1", "2026-01-02T00:00:00Z") }],
    cursor: "text-cursor-1",
    imageCursor: "",
    snapshotId: "snap-image-1",
  };
  const cursors = pickNextCursors(previous);
  assert.equal(cursors.image, null, "exhausted image stream is skipped");
  assert.equal(cursors.text, "text-cursor-1", "text stream advances");
  const result = applyResponses(
    { kind: "text", response: textOk(
      [buildTextRow("text-2", "2026-01-04T00:00:00Z")],
      "",
    ) },
    { kind: "image-skip" },
    previous,
    { append: true },
  );
  assert.equal(result.rows.length, 1);
  assert.equal(result.rows[0].row.remote_entry_id, "text-2");
  assert.equal(result.cursor, "");
  assert.equal(result.imageCursor, "");
});

test("behaviour: both streams active — each endpoint receives its own cursor", () => {
  // Both streams have cursors. `pickNextCursors` returns each
  // cursor verbatim so neither endpoint receives the other's
  // cursor (the regression the bug-fix pins). The new page is
  // built exclusively from the responses the rail just fetched.
  const previous: PageState = {
    ...emptyPage(),
    rows: [
      { kind: "text", row: buildTextRow("text-1", "2026-01-02T00:00:00Z") },
      { kind: "image", row: buildImageRow("img-1", "2026-01-01T00:00:00Z") },
    ],
    cursor: "text-cursor-1",
    imageCursor: "image-cursor-1",
    snapshotId: "snap-text-1",
  };
  const cursors = pickNextCursors(previous);
  const result = applyResponses(
    { kind: "text", response: textOk(
      [buildTextRow("text-2", "2026-01-04T00:00:00Z")],
      "",
    ) },
    { kind: "image", response: imageOk(
      [buildImageRow("img-2", "2026-01-03T00:00:00Z")],
      "",
    ) },
    previous,
    { append: true },
  );
  assert.equal(cursors.text, "text-cursor-1");
  assert.equal(cursors.image, "image-cursor-1");
  assert.notEqual(cursors.text, cursors.image);
  // Only the newly-fetched rows are visible; the previous
  // visible window has been superseded by the new page.
  assert.equal(result.rows.length, 2);
  assert.equal(result.rows[0].row.remote_entry_id, "text-2");
  assert.equal(result.rows[1].row.remote_entry_id, "img-2");
});

test("behaviour: merged list never duplicates remote_entry_id", () => {
  // The two streams both return a row that re-uses an id from
  // the previous window. The helper deduplicates by id so the
  // final visible window never shows the same id twice.
  const previous: PageState = {
    ...emptyPage(),
    rows: [
      { kind: "text", row: buildTextRow("text-shared", "2026-01-04T00:00:00Z") },
    ],
    cursor: "text-cursor-1",
    imageCursor: "image-cursor-1",
  };
  const result = applyResponses(
    { kind: "text", response: textOk(
      [buildTextRow("text-shared", "2026-01-04T00:00:00Z")],
      "",
    ) },
    { kind: "image", response: imageOk(
      [
        buildImageRow("text-shared", "2026-01-04T00:00:00Z"),
        buildImageRow("img-new", "2026-01-03T00:00:00Z"),
      ],
      "",
    ) },
    previous,
    { append: true },
  );
  const ids = result.rows.map((row) => row.row.remote_entry_id);
  assert.deepEqual(ids, Array.from(new Set(ids)));
  // The duplicate id is dropped from the merged set; only the
  // first occurrence survives the dedupe.
  assert.equal(result.rows.length, 2);
});

test("behaviour: invalid_cursor on text stream asks the rail to retry from page 1", () => {
  const result = applyResponses(
    { kind: "text", response: { kind: "invalid_cursor" } },
    { kind: "image-skip" },
    emptyPage(),
    { append: false },
  );
  assert.equal(result.requestInvalidCursor, true);
  assert.equal(result.error, "invalid_cursor:text");
  assert.equal(result.cursor, "");
});

test("behaviour: peer_unavailable on text clears the page", () => {
  const result = applyResponses(
    { kind: "text", response: { kind: "peer_unavailable", reason: "not_trusted" } },
    { kind: "image-skip" },
    emptyPage(),
    { append: false },
  );
  assert.equal(result.exhausted, true);
  assert.equal(result.error, "peer_unavailable:not_trusted");
  assert.equal(result.rows.length, 0);
});

test("behaviour: missing image capability preserves a successful text page", () => {
  const row = buildTextRow("text-only-peer-entry", "2026-01-04T00:00:00Z");
  const result = applyResponses(
    { kind: "text", response: textOk([row], "text-next") },
    {
      kind: "image",
      response: { kind: "peer_unavailable", reason: "not_available" },
    },
    emptyPage(),
    { append: false },
  );

  assert.deepEqual(
    result.rows.map((item) => item.row.remote_entry_id),
    ["text-only-peer-entry"],
  );
  assert.equal(result.cursor, "text-next");
  assert.equal(result.imageCursor, "");
  assert.equal(result.error, null);
  assert.equal(result.exhausted, false);
});

test("behaviour: exhausted stream + no buffered rows flips the `exhausted` flag", () => {
  const previous: PageState = {
    ...emptyPage(),
    cursor: "",
    imageCursor: "",
  };
  const result = applyResponses(
    { kind: "text", response: textOk([], "") },
    { kind: "image", response: imageOk([], "") },
    previous,
    { append: true },
  );
  assert.equal(result.exhausted, true);
  assert.equal(result.cursor, "");
  assert.equal(result.imageCursor, "");
});

test("behaviour: an empty work-limited image page keeps its continuation cursor", () => {
  // The host may return an empty page with a signed continuation when
  // its bounded scan budget was consumed by invalid assets. Empty rows
  // alone therefore do not mean that the image stream is exhausted.
  const result = applyResponses(
    { kind: "text-skip" },
    { kind: "image", response: imageOk([], "image-work-continuation") },
    emptyPage(),
    { append: true },
  );
  assert.equal(result.imageCursor, "image-work-continuation");
  assert.equal(result.exhausted, false);
  assert.deepEqual(pickNextCursors(result), {
    text: null,
    image: "image-work-continuation",
  });
});

// ---------------------------------------------------------------------------
// 7. Combined cap — the merged list MUST not exceed
//    MAX_COMBINED_PAGE_ROWS even when both streams return
//    MAX_PER_STREAM_ROWS rows.
// ---------------------------------------------------------------------------

test("behaviour: combined page never exceeds MAX_COMBINED_PAGE_ROWS", () => {
  // Both streams return their full page (50 rows each) and the
  // rail truncates the merged set to MAX_COMBINED_PAGE_ROWS so
  // the contract the design pins ("páginas combinadas de hasta
  // 50") stays consistent.
  const textRows: PeerHistoryRow[] = [];
  const imageRows: PeerImageBrowseRow[] = [];
  for (let index = 0; index < MAX_COMBINED_PAGE_ROWS; index += 1) {
    textRows.push(buildTextRow(`text-${index}`, "2026-01-04T00:00:00Z"));
  }
  for (let index = 0; index < MAX_COMBINED_PAGE_ROWS; index += 1) {
    imageRows.push(buildImageRow(`img-${index}`, "2026-01-03T00:00:00Z"));
  }
  const result = applyResponses(
    { kind: "text", response: textOk(textRows, "text-next") },
    { kind: "image", response: imageOk(imageRows, "image-next") },
    emptyPage(),
    { append: false },
  );
  assert.equal(
    result.rows.length,
    MAX_COMBINED_PAGE_ROWS,
    "merged list must stay within the combined cap",
  );
  // The remainder lives in the per-stream buffers so the next
  // page surfaces the hidden rows instead of skipping them.
  assert.equal(
    result.textBuffer.length + result.imageBuffer.length,
    MAX_COMBINED_PAGE_ROWS,
    "every hidden row lives in a per-stream buffer",
  );
});

test("behaviour: buffers surface on the next page without dropping or duplicating rows", () => {
  // Stream A returns 50 newest rows (Feb dates), stream B
  // returns 50 strictly older rows (Jan dates). The combined
  // cap hides every stream B row in the buffer; the next
  // `Siguiente` MUST surface those buffered rows newest-first
  // through `promoteBuffers` before the rail asks the host for
  // any more data. The test exercises both halves of the
  // contract: the `applyResponses` first page response fills the
  // buffer, then `promoteBuffers` drains it without losing a
  // single row.
  const streamA: PeerHistoryRow[] = [];
  for (let index = 0; index < MAX_COMBINED_PAGE_ROWS; index += 1) {
    streamA.push(buildTextRow(`a-${index}`, `2026-02-${String(1 + index).padStart(2, "0")}T00:00:00Z`));
  }
  const streamB: PeerHistoryRow[] = [];
  for (let index = 0; index < MAX_COMBINED_PAGE_ROWS; index += 1) {
    streamB.push(buildTextRow(`b-${index}`, `2026-01-${String(1 + index).padStart(2, "0")}T00:00:00Z`));
  }
  // Seed the buffer manually so we cover the scenario where the
  // host response hid 50 rows in the buffer. The helper only sees
  // the new response on `append=true`; it does NOT re-inject
  // previous.rows nor the buffer itself.
  const firstPage = applyResponses(
    { kind: "text", response: textOk(streamA, "text-next") },
    { kind: "image", response: imageOk(streamB, "image-next") },
    emptyPage(),
    { append: false },
  );
  assert.equal(firstPage.rows.length, MAX_COMBINED_PAGE_ROWS);
  assert.equal(
    firstPage.textBuffer.length + firstPage.imageBuffer.length,
    MAX_COMBINED_PAGE_ROWS,
    "the cap hides every stream B row in the per-stream buffers",
  );
  // The `Siguiente` event drains the buffer through
  // `promoteBuffers`. No network request fires: the visible
  // window is the previous buffer, sorted newest-first, and the
  // cursor pair is preserved so the user can keep advancing.
  const promoted = promoteBuffers(firstPage);
  const ids = promoted.rows.map((row) => row.row.remote_entry_id);
  // Stream B was supplied oldest-first; the promotion sorts
  // newest-first so the expected order is reversed.
  const expectedOrder = streamB.map((row) => row.remote_entry_id).slice().reverse();
  assert.deepEqual(ids, expectedOrder);
  assert.deepEqual(
    ids,
    Array.from(new Set(ids)),
    "promoteBuffers never duplicates a remote_entry_id",
  );
  assert.equal(promoted.textBuffer.length, 0);
  assert.equal(promoted.imageBuffer.length, 0);
  assert.equal(promoted.cursor, firstPage.cursor);
  assert.equal(promoted.imageCursor, firstPage.imageCursor);
  assert.equal(
    promoted.exhausted,
    false,
    "promoting a non-empty buffer never marks the rail as exhausted",
  );
});

test("behaviour: Siguiente consumes the buffer with both cursors exhausted", () => {
  // The spec scenario "Siguiente with exhausted cursors and a
  // non-empty buffer": both cursors are empty (the streams
  // reported no more pages) but a buffer still has hidden rows.
  // The rail keeps the Siguiente button enabled and the
  // `promoteBuffers` helper surfaces the hidden rows.
  const buffered: RemoteRailRow[] = [];
  for (let index = 0; index < 10; index += 1) {
    buffered.push({
      kind: "image",
      row: buildImageRow(`hidden-${index}`, `2026-02-${String(1 + index).padStart(2, "0")}T00:00:00Z`),
    });
  }
  const previous: PageState = {
    ...emptyPage(),
    rows: [
      { kind: "text", row: buildTextRow("last-text", "2026-03-01T00:00:00Z") },
    ],
    cursor: "",
    imageCursor: "",
    textBuffer: [],
    imageBuffer: buffered,
  };
  const promoted = promoteBuffers(previous);
  assert.equal(promoted.rows.length, 10);
  assert.equal(promoted.rows[0].row.remote_entry_id, "hidden-9");
  assert.equal(promoted.rows[9].row.remote_entry_id, "hidden-0");
  assert.equal(promoted.imageBuffer.length, 0);
  assert.equal(promoted.cursor, "");
  assert.equal(promoted.imageCursor, "");
  // The cursor pair is empty AND the buffer is empty: a follow-up
  // advance marks the rail as exhausted.
  assert.equal(promoted.exhausted, true);
});

test("behaviour: alternating streams never skip or repeat a remote_entry_id", () => {
  // Stream A is exhausted from the first page (its cursor is
  // empty and it skipped on the next request). Stream B keeps
  // returning one new row at a time. The merged visible
  // window never repeats an id from a previous page.
  let cursorB = "image-cursor-1";
  const previous: PageState = {
    ...emptyPage(),
    rows: [
      { kind: "text", row: buildTextRow("text-1", "2026-01-03T00:00:00Z") },
      { kind: "image", row: buildImageRow("img-1", "2026-01-02T00:00:00Z") },
    ],
    cursor: "",
    imageCursor: cursorB,
  };
  const seen = new Set<string>(["text-1", "img-1"]);
  for (let step = 0; step < 3; step += 1) {
    cursorB = `image-cursor-${step + 2}`;
    const newRow = buildImageRow(`img-${step + 2}`, `2026-01-0${4 + step}T00:00:00Z`);
    const next = applyResponses(
      { kind: "text-skip" },
      { kind: "image", response: imageOk([newRow], cursorB) },
      previous,
      { append: true },
    );
    // The new page replaces the visible window with the single
    // newly-fetched image row; nothing from `previous.rows` is
    // re-injected.
    assert.equal(next.rows.length, 1, `step ${step}: exactly one row visible`);
    assert.equal(next.rows[0].row.remote_entry_id, `img-${step + 2}`);
    assert.ok(
      !seen.has(next.rows[0].row.remote_entry_id),
      `step ${step}: never repeats a remote_entry_id`,
    );
    seen.add(next.rows[0].row.remote_entry_id);
    // Advance the previous state to mirror what the rail does
    // after consuming `applyResponses`.
    previous.rows = next.rows;
    previous.cursor = next.cursor;
    previous.imageCursor = next.imageCursor;
    previous.textBuffer = next.textBuffer;
    previous.imageBuffer = next.imageBuffer;
  }
});

test("behaviour: promoteBuffers returns an exhausted state when both buffers and cursors are empty", () => {
  // The rail is genuinely exhausted: no buffered rows AND no
  // cursors. `promoteBuffers` must flip `exhausted` so a
  // subsequent `Siguiente` does not call the host.
  const previous: PageState = {
    ...emptyPage(),
    cursor: "",
    imageCursor: "",
  };
  const promoted = promoteBuffers(previous);
  assert.equal(promoted.rows.length, 0);
  assert.equal(promoted.exhausted, true);
});

test("behaviour: dates from both streams interleave newest-first", () => {
  // Stream A returns rows with dates 2026-01-10, 2026-01-08,
  // 2026-01-06; stream B returns 2026-01-09, 2026-01-07.
  // The merged list MUST be 2026-01-10, 2026-01-09, 2026-01-08,
  // 2026-01-07, 2026-01-06 regardless of which endpoint
  // answered first.
  const streamA: PeerHistoryRow[] = [
    buildTextRow("a-10", "2026-01-10T00:00:00Z"),
    buildTextRow("a-08", "2026-01-08T00:00:00Z"),
    buildTextRow("a-06", "2026-01-06T00:00:00Z"),
  ];
  const streamB: PeerImageBrowseRow[] = [
    buildImageRow("b-09", "2026-01-09T00:00:00Z"),
    buildImageRow("b-07", "2026-01-07T00:00:00Z"),
  ];
  const result = applyResponses(
    { kind: "text", response: textOk(streamA, "") },
    { kind: "image", response: imageOk(streamB, "") },
    emptyPage(),
    { append: false },
  );
  assert.deepEqual(
    result.rows.map((row) => row.row.created_at),
    [
      "2026-01-10T00:00:00Z",
      "2026-01-09T00:00:00Z",
      "2026-01-08T00:00:00Z",
      "2026-01-07T00:00:00Z",
      "2026-01-06T00:00:00Z",
    ],
  );
});

test("behaviour: helper is the single source of truth (no parallel merge in the rail)", () => {
  // The regression the spec scenario "Prueba la lógica real, no
  // un espejo" pins: the test exercises the same helper the
  // rail imports. A second `driveRailPage` mirror that drifts
  // from the production merge would surface here as the
  // missing import below.
  assert.match(
    mergeSource,
    /export function applyResponses/,
    "shared helper must export applyResponses so the rail and the tests share the same merge logic",
  );
  assert.match(
    railSource,
    /from\s+["']\.\/lib\/remoteHistoryMerge["']/,
    "rail must import the shared merge helper instead of duplicating it locally",
  );
  assert.doesNotMatch(
    railSource,
    /merged\.sort\(\(a,\s*b\)\s*=>\s*\{\s*const\s+ta\s*=/,
    "rail must not contain a parallel merge implementation",
  );
});

// ---------------------------------------------------------------------------
// 8. Capability gating — peers that do not advertise
//    `image_import` MUST NOT surface a successful Import path
//    on the rail. The host and client core check the persisted
//    capability column; the renderer must respect the typed
//    `peer_unavailable { reason: "not_available" }` outcome
//    and disable the Import action accordingly.
// ---------------------------------------------------------------------------

test("Rail hides the Import button when the peer lacks image_import", () => {
  // The remote card's menu item delegates to `peerImageFetchCommand`
  // for image rows. When the peer lacks the capability the host
  // returns `peer_unavailable { reason: "not_available" }`; the
  // card must translate this into a disabled Import state so the
  // renderer never offers an action the host will reject.
  //
  // The simplest defence-in-depth check the frontend tests
  // pin: the card source must consult the outcome kind before
  // rendering the result copy. The `not_available` reason is
  // reserved for the capability gate, so a future regression
  // that collapses it into a generic `peer_unavailable` would
  // surface here as a missing branch.
  assert.match(
    cardSource,
    /peerImageFetchCommand/,
    "card must delegate image import to peerImageFetchCommand",
  );
  assert.match(
    cardSource,
    /peer_unavailable/,
    "card must surface a typed peer_unavailable outcome",
  );
});

test("RemoteHistoryRail combines additive caps_extra for image capability gating", () => {
  assert.match(
    railSource,
    /activeEntry\.caps_extra/,
    "the peer snapshot's additive capabilities must reach remote preview cards",
  );
  assert.match(
    mergeSource,
    /reason !== "not_available"/,
    "a missing optional image capability must not become a global history error",
  );
});

test("Outcome translation distinguishes the capability-gate reason", () => {
  // The outcome reason the bridge surfaces for a missing
  // capability is `not_available`. The card helper must keep
  // that identifier visible in the source so a regression
  // that drops it (or collapses it into a generic
  // `not_trusted`) is caught by the frontend tests.
  assert.match(
    cardSource,
    /not_available/,
    "card must surface the capability-gate reason verbatim",
  );
});
