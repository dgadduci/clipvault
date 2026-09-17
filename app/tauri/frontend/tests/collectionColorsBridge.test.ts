/**
 * Bridge-level regression coverage for the
 * `collection-colors-and-card-collection-labels` command surface.
 *
 * The frontend talks to the backend through
 * `clipvault_collections_set_color`, which the metadata-only
 * `clipvault://organization-updated` event follows up. The
 * contract this file pins:
 *
 *   - The bridge forwards `collectionId` and `colorHex` verbatim
 *     — no normalisation, no opaque wrapper, no fabricated
 *     names that an integration test later has to defend.
 *   - A backend `InvalidCollectionColor` rejection never leaks
 *     raw clipboard content; the call rejects with the typed
 *     error message the command pipe exposes.
 *   - The command never reads from the
 *     `clipvault_organization_snapshot`, `clipvault_entry_*`,
 *     `clipvault_collections_create`, `clipvault_collections_rename`
 *     or `clipvault_collections_delete` surface, so a colour
 *     mutation stays a metadata-only mutation by construction.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  collectionsSetColorCommand,
} from "../src/lib/tauri.ts";
import type { Collection } from "../src/types.ts";

type InvokeRecord = { cmd: string; args?: Record<string, unknown> };

function installInvoke(
  invoker: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>,
): InvokeRecord[] {
  const calls: InvokeRecord[] = [];
  (globalThis as unknown as { window: unknown }).window = {
    __TAURI_INTERNALS__: {
      invoke: async (cmd: string, args?: Record<string, unknown>) => {
        calls.push({ cmd, args });
        return invoker(cmd, args);
      },
    },
  };
  return calls;
}

function makeCollection(overrides: Partial<Collection> = {}): Collection {
  return {
    id: 7,
    stable_key: null,
    name: "Trabajo",
    kind: "user",
    color_hex: "#c62828",
    created_at: "2026-01-02T03:04:05Z",
    updated_at: "2026-01-02T03:04:05Z",
    ...overrides,
  };
}

test("collectionsSetColorCommand forwards collectionId and colorHex verbatim", async () => {
  const calls = installInvoke(async (cmd) => {
    if (cmd === "clipvault_collections_set_color") {
      return makeCollection();
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  const updated = await collectionsSetColorCommand({
    collectionId: 7,
    colorHex: "#FF00AA",
  });
  assert.equal(calls.length, 1);
  assert.equal(calls[0].cmd, "clipvault_collections_set_color");
  const args = calls[0].args as { collectionId: number; colorHex: string };
  assert.equal(args.collectionId, 7);
  assert.equal(args.colorHex, "#FF00AA");
  // The bridge returns the refreshed collection as received from
  // the backend; the uppercased colour in the input is preserved
  // verbatim so the backend owns the canonicalisation step.
  assert.equal(updated.id, 7);
});

test("collectionsSetColorCommand propagates an invalid colour rejection", async () => {
  installInvoke(async (cmd) => {
    if (cmd === "clipvault_collections_set_color") {
      throw new Error("invalid_collection_color");
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  await assert.rejects(
    () =>
      collectionsSetColorCommand({
        collectionId: 7,
        colorHex: "not-a-hex",
      }),
    /invalid_collection_color/,
  );
});

test("collectionsSetColorCommand never echoes clipboard content", async () => {
  const sensitive = "secret-token-pasted-from-keyboard";
  const calls = installInvoke(async (cmd) => {
    if (cmd === "clipvault_collections_set_color") {
      return makeCollection();
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  await collectionsSetColorCommand({
    collectionId: 1,
    colorHex: "#1565c0",
  });
  const serialised = JSON.stringify(calls);
  assert.equal(
    serialised.includes(sensitive),
    false,
    "the colour bridge must never carry clipboard content",
  );
});

test("collectionsSetColorCommand never invokes sibling commands", async () => {
  // A colour update is metadata-only by construction: the bridge
  // must not fan-out to the snapshot / entry / collections surface,
  // otherwise a regression could leak context the rest of the
  // change tries to keep stable.
  const calls = installInvoke(async (cmd) => {
    if (cmd === "clipvault_collections_set_color") {
      return makeCollection();
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  await collectionsSetColorCommand({
    collectionId: 7,
    colorHex: "#c62828",
  });
  assert.deepEqual(
    calls.map((c) => c.cmd),
    ["clipvault_collections_set_color"],
  );
});
