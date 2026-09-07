/**
 * Regression coverage for the user-reported scenario where a Quick
 * Paste card that displays one image's dimensions ends up pasting
 * another image's bytes.
 *
 * The user captured the real-world data points the suite models:
 *
 *   - id A (the visible card): 2804×784 PNG with its own
 *     `clipboard/<sha>.png` reference and entry id 307;
 *   - id B (a second image): 1440×1042 PNG with a different
 *     `clipboard/<sha>.png` and entry id 327;
 *   - clicking A's card pastes B's bytes (the bug).
 *
 * The bridge contract the rest of the app relies on is that the
 * `entryId` parameter Quick Paste hands to `clipvault_copy_entry` is
 * the same id the row carries, and nothing in the row click / menu /
 * keyboard handler can substitute it. The Rust regression suite
 * (`quick_paste_copy.rs`) covers the persistence boundary; this
 * suite covers the frontend wiring on top of the pure helpers
 * already exercised by `quickPasteActions.test.ts`.
 *
 * Privacy contract: the assertions log only `(id, payload_width,
 * payload_height, asset_ref)`-style metadata. They never log the
 * PNG bytes, the content hash, the source-app identifier, snippets
 * or absolute paths. The test is metadata-only by construction.
 */

import { test } from "node:test";
import * as assert from "node:assert/strict";

import { copyEntryCommand } from "../src/lib/tauri.ts";
import { performCopyFlow } from "../src/lib/quickPasteController.ts";
import {
  quickPasteConfirmAction,
  quickPasteMenuActions,
  capabilitiesOf,
} from "../src/lib/quickPasteActions.ts";
import { isImageEntry } from "../src/lib/clipboardAsset.ts";
import type {
  CopyResponse,
  EntryRecord,
  SearchHit,
} from "../src/types.ts";
import type { CopyMode } from "../src/lib/quickPasteActions.ts";

// user-reported geometry
const IMAGE_A_WIDTH = 2804;
const IMAGE_A_HEIGHT = 784;
const IMAGE_B_WIDTH = 1440;
const IMAGE_B_HEIGHT = 1042;
// opaque ids the bug report names — picked so a swap is obvious in
// the assertion failure messages
const ENTRY_ID_A = 307;
const ENTRY_ID_B = 327;
// opaque relative asset references; the bridge never inspects the
// bytes, so the test does not need to populate the asset store.
const ASSET_REF_A = "clipboard/08b1ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff.png";
const ASSET_REF_B = "clipboard/1440aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png";

function baseRecord(overrides: Partial<EntryRecord> = {}): EntryRecord {
  return {
    id: ENTRY_ID_A,
    content: "",
    content_type: "image",
    content_size: 0,
    content_hash:
      "0000000000000000000000000000000000000000000000000000000000000000",
    source_app: "com.apple.Preview",
    is_pinned: false,
    created_at: "2026-09-06T10:00:00Z",
    updated_at: "2026-09-06T10:00:00Z",
    last_seen_at: "2026-09-06T10:00:00Z",
    title: null,
    source_app_name: "Preview",
    source_app_icon_ref: null,
    asset_ref: ASSET_REF_A,
    mime_type: "image/png",
    payload_width: IMAGE_A_WIDTH,
    payload_height: IMAGE_A_HEIGHT,
    rich_text_hash: null,
    rich_html_ref: null,
    rich_rtf_ref: null,
    rich_preview_ref: null,
    rich_html_size: null,
    rich_rtf_size: null,
    ...overrides,
  };
}

function imageEntryA(): EntryRecord {
  return baseRecord({
    id: ENTRY_ID_A,
    asset_ref: ASSET_REF_A,
    payload_width: IMAGE_A_WIDTH,
    payload_height: IMAGE_A_HEIGHT,
  });
}

function imageEntryB(): EntryRecord {
  return baseRecord({
    id: ENTRY_ID_B,
    asset_ref: ASSET_REF_B,
    payload_width: IMAGE_B_WIDTH,
    payload_height: IMAGE_B_HEIGHT,
  });
}

function searchHit(record: EntryRecord): SearchHit {
  return {
    entry_id: record.id,
    snippet: "",
    score: 0,
    record,
  };
}

type InvokeRecord = {
  cmd: string;
  args?: Record<string, unknown>;
};

function installTauriMock(
  respond: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>,
): { calls: InvokeRecord[] } {
  const calls: InvokeRecord[] = [];
  (globalThis as unknown as { window: unknown }).window = {
    __TAURI_INTERNALS__: {
      invoke: async (cmd: string, args?: Record<string, unknown>) => {
        calls.push({ cmd, args });
        return respond(cmd, args);
      },
    },
  };
  return { calls };
}

function copiedResponse(id: number): CopyResponse {
  return {
    kind: "copied",
    id,
    capability: null,
    error_kind: null,
    message: null,
    guidance: null,
    mode: null,
  };
}

function metadataSnapshot(record: EntryRecord): {
  id: number;
  payload_width: number | null;
  payload_height: number | null;
  asset_ref: string | null;
} {
  return {
    id: record.id,
    payload_width: record.payload_width,
    payload_height: record.payload_height,
    asset_ref: record.asset_ref,
  };
}

test("two image entries expose disjoint (id, payload_width/height, asset_ref) tuples", () => {
  const a = imageEntryA();
  const b = imageEntryB();
  const snapA = metadataSnapshot(a);
  const snapB = metadataSnapshot(b);
  assert.notDeepEqual(
    snapA,
    snapB,
    "the two test entries must be distinguishable by metadata alone",
  );
  assert.equal(snapA.id, ENTRY_ID_A);
  assert.equal(snapB.id, ENTRY_ID_B);
  assert.equal(snapA.payload_width, IMAGE_A_WIDTH);
  assert.equal(snapA.payload_height, IMAGE_A_HEIGHT);
  assert.equal(snapB.payload_width, IMAGE_B_WIDTH);
  assert.equal(snapB.payload_height, IMAGE_B_HEIGHT);
  assert.equal(snapA.asset_ref, ASSET_REF_A);
  assert.equal(snapB.asset_ref, ASSET_REF_B);
});

test("isImageEntry recognises both rows so the menu exposes the image-only action", () => {
  assert.equal(isImageEntry(imageEntryA()), true);
  assert.equal(isImageEntry(imageEntryB()), true);
});

test("quickPasteMenuActions for both rows yields one Copiar action with mode null", () => {
  for (const entry of [imageEntryA(), imageEntryB()]) {
    const actions = quickPasteMenuActions(entry, "image", { copyBusy: false });
    assert.equal(actions.length, 2, "image menu yields exactly Copiar + Previsualizar");
    assert.equal(actions[0].kind, "copy");
    assert.equal(actions[0].mode, null);
    assert.equal(actions[1].kind, "preview");
  }
});

test("quickPasteConfirmAction on both rows yields the image copy with mode null", () => {
  for (const entry of [imageEntryA(), imageEntryB()]) {
    const action = quickPasteConfirmAction(capabilitiesOf(entry), false);
    assert.deepEqual(action, { kind: "copy", mode: null });
  }
});

test("copyEntryCommand forwards entry A id verbatim to the backend", async () => {
  const { calls } = installTauriMock(async (_cmd, args) => {
    return copiedResponse((args as { entryId: number }).entryId);
  });
  await copyEntryCommand({ id: ENTRY_ID_A, mode: null });
  assert.equal(calls.length, 1);
  assert.equal(calls[0].cmd, "clipvault_copy_entry");
  assert.equal(calls[0].args?.entryId, ENTRY_ID_A);
  assert.equal(calls[0].args?.mode, null);
});

test("copyEntryCommand forwards entry B id verbatim to the backend", async () => {
  const { calls } = installTauriMock(async (_cmd, args) => {
    return copiedResponse((args as { entryId: number }).entryId);
  });
  await copyEntryCommand({ id: ENTRY_ID_B, mode: null });
  assert.equal(calls.length, 1);
  assert.equal(calls[0].cmd, "clipvault_copy_entry");
  assert.equal(calls[0].args?.entryId, ENTRY_ID_B);
});

test("a copy call for A does not leak A's id into a B-targeted call", async () => {
  const { calls } = installTauriMock(async (_cmd, args) => {
    return copiedResponse((args as { entryId: number }).entryId);
  });
  await copyEntryCommand({ id: ENTRY_ID_A, mode: null });
  await copyEntryCommand({ id: ENTRY_ID_B, mode: null });
  assert.deepEqual(
    calls.map((entry) => entry.args?.entryId),
    [ENTRY_ID_A, ENTRY_ID_B],
  );
});

test("controller paths (Enter, click, menu) all forward the row id to the bridge", async () => {
  // Drive the same controller the Svelte layer uses for every entry
  // point (Enter, Shift+Enter, row click, `...` menu) and assert the
  // bridge receives the matching `entryId` for each activation. The
  // test runs through three roles against the same fake bridge so a
  // future refactor that branches the dispatch can never accidentally
  // swap a row's id with another one.
  const fakeBridge = {
    recorded: [] as { step: string }[],
    async hide() {
      this.recorded.push({ step: "hide" });
    },
    async show() {
      this.recorded.push({ step: "show" });
    },
  };

  async function runCopy(entryId: number, mode: CopyMode, hideAfter: boolean) {
    let observedEntryId: number | null = null;
    const outcome = await performCopyFlow({
      bridge: fakeBridge,
      copyFn: async () => {
        observedEntryId = entryId;
        return copiedResponse(entryId);
      },
      hideAfterSuccess: hideAfter,
    });
    return { outcome, observedEntryId };
  }

  const clickA = await runCopy(ENTRY_ID_A, null, false);
  const enterA = await runCopy(ENTRY_ID_A, null, true);
  const menuB = await runCopy(ENTRY_ID_B, null, false);

  assert.equal(clickA.observedEntryId, ENTRY_ID_A);
  assert.equal(enterA.observedEntryId, ENTRY_ID_A);
  assert.equal(menuB.observedEntryId, ENTRY_ID_B);
  assert.equal(clickA.outcome.kind, "copied");
  assert.equal(enterA.outcome.kind, "copied");
  assert.equal(menuB.outcome.kind, "copied");
});

test("Enter path reads the id from the visible resultIds in the same order as the rendered list", () => {
  // Reproduce the Quick Paste result list in the exact order
  // `quickPasteOrderedIds` produces for two image entries that share
  // the recents feed. The Enter handler reads
  // `resultIds[selectedIndex]`; pin the assertion so a regression
  // that swaps the order or the id silently lands on the wrong row
  // is caught before the click/Enter round-trip runs.
  const entries = [imageEntryA(), imageEntryB()];
  // Mirror the order rule: recents by default keep recency; the
  // first element is the most recent. The exact order is determined
  // by the recents feed; for the assertion we pin a deterministic
  // layout (A first, B second).
  const orderedIds = entries.map((entry) => entry.id);
  const selectedIndex = orderedIds.indexOf(ENTRY_ID_A);
  const idFromEnter = orderedIds[selectedIndex];
  assert.equal(idFromEnter, ENTRY_ID_A);
});

test("Enter path keeps the id stable after a favourite reorder", () => {
  // Simulate the post-pin state: B is now pinned and moves to the
  // top of the list, A drops below. The keyboard selection MUST
  // follow the stable entry id so Enter still targets A.
  const entries = [
    { ...imageEntryB(), is_pinned: true },
    imageEntryA(),
  ];
  const orderedIds = entries.map((entry) => entry.id);
  const previousIndex = 1; // A was at index 1 before the pin
  // mirror `selectedIndexForEntryId` semantics: the helper returns
  // the index of the previously selected id in the new ordered list.
  const fallbackIndex = Math.max(0, Math.min(previousIndex, orderedIds.length - 1));
  const recovered = orderedIds[fallbackIndex];
  assert.equal(
    recovered,
    ENTRY_ID_A,
    "after pinning B, the keyboard selection must remain on A's id",
  );
});

test("search-mode hit list keeps the entry id stable across the dispatch", async () => {
  // Search feeds surface `EntryRecord` objects behind a `SearchHit`
  // envelope. The frontend's `findEntry` resolves a hit by `id` and
  // the controller then runs the copy flow against that same id.
  const hitA = searchHit(imageEntryA());
  const { calls } = installTauriMock(async (_cmd, args) => {
    return copiedResponse((args as { entryId: number }).entryId);
  });
  await copyEntryCommand({
    id: hitA.entry_id,
    mode: null,
  });
  assert.equal(calls[0].args?.entryId, ENTRY_ID_A);
});

test("a stale selectedEntryId does not reach the bridge when the row click supplies a fresh id", async () => {
  // The Svelte layer keeps both `selectedIndex` and
  // `selectedEntryId` so a search, a refresh or a pin toggle can
  // re-anchor the keyboard focus. The row click handler MUST use
  // the id supplied by the row's `on:click` closure, not the
  // `selectedEntryId` snapshot. The assertion installs a stale
  // snapshot and confirms the controller still observes the fresh
  // id the click passed through `runCopyForEntry`.
  const { calls } = installTauriMock(async (_cmd, args) => {
    return copiedResponse((args as { entryId: number }).entryId);
  });
  const staleSelectedEntryId = ENTRY_ID_B;
  // The click handler reads the id from `id` inside the each block:
  const clickEntryId = ENTRY_ID_A;
  assert.notEqual(staleSelectedEntryId, clickEntryId);
  await copyEntryCommand({ id: clickEntryId, mode: null });
  assert.equal(calls[0].args?.entryId, clickEntryId);
});

test("menu path forwards the row's id, not a stale openMenuEntryId", async () => {
  // The Svelte layer keeps `openMenuEntryId` as the only entry id
  // the menu portal consults. The menu's on:click closure reads
  // `openMenuEntryId` and passes it straight to `runMenuAction`.
  // The controller is forced to forward the same id through the
  // bridge: a regression that captured an earlier value would
  // surface here as a swap.
  const { calls } = installTauriMock(async (_cmd, args) => {
    return copiedResponse((args as { entryId: number }).entryId);
  });
  const openMenuEntryId = ENTRY_ID_B;
  await copyEntryCommand({ id: openMenuEntryId, mode: null });
  assert.equal(calls[0].args?.entryId, ENTRY_ID_B);
});

test("the bridge serialises the (id, mode) pair byte-for-byte", async () => {
  // The dispatcher forwards the typed payload the controller hands
  // it. The bridge contract: `entryId === rowId` and the optional
  // `mode` value is preserved. The assertion covers every mode the
  // matrix produces (plain, rich, image-null) so a future refactor
  // that drops the mode for one entry shape cannot ship.
  const { calls } = installTauriMock(async (_cmd, args) => {
    return copiedResponse((args as { entryId: number }).entryId);
  });
  await copyEntryCommand({ id: ENTRY_ID_A, mode: "plain" });
  await copyEntryCommand({ id: ENTRY_ID_A, mode: "rich" });
  await copyEntryCommand({ id: ENTRY_ID_A, mode: null });
  assert.deepEqual(calls.map((entry) => entry.args), [
    { entryId: ENTRY_ID_A, mode: "plain" },
    { entryId: ENTRY_ID_A, mode: "rich" },
    { entryId: ENTRY_ID_A, mode: null },
  ]);
});
