import { test } from "node:test";
import assert from "node:assert/strict";

import { visibleCollectionRows } from "../src/lib/collectionTree.ts";
import { collectionsCreateSubcollectionCommand } from "../src/lib/tauri.ts";
import type { Collection } from "../src/types.ts";

function collection(
  id: number,
  name: string,
  parentId: number | null = null,
  kind: Collection["kind"] = "user",
): Collection {
  return {
    id,
    stable_key: kind === "system" ? "history" : null,
    parent_id: parentId,
    name,
    kind,
    color_hex: "#1565c0",
    created_at: "2026-01-02T03:04:05Z",
    updated_at: "2026-01-02T03:04:05Z",
  };
}

test("visible collection rows keep History first and sort each tree level", () => {
  const rows = visibleCollectionRows(
    [
      collection(1, "Historial", null, "system"),
      collection(2, "Trabajo"),
      collection(3, "Zeta", 2),
      collection(4, "Archivo"),
      collection(5, "Alpha", 2),
    ],
    new Set([2]),
  );

  assert.deepEqual(
    rows.map(({ collection: row, level }) => [row.name, level]),
    [
      ["Historial", 1],
      ["Archivo", 1],
      ["Trabajo", 1],
      ["Alpha", 2],
      ["Zeta", 2],
    ],
  );
});

test("collapsed roots hide children without changing other roots", () => {
  const rows = visibleCollectionRows(
    [collection(1, "Principal"), collection(2, "Hija", 1), collection(3, "Otra")],
    new Set(),
  );

  assert.deepEqual(
    rows.map(({ collection: row, level }) => [row.name, level]),
    [
      ["Otra", 1],
      ["Principal", 1],
    ],
  );
});

test("subcollection creation bridge sends its parent id and name", async () => {
  const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
  (globalThis as unknown as { window: unknown }).window = {
    __TAURI_INTERNALS__: {
      invoke: async (command: string, args?: Record<string, unknown>) => {
        calls.push({ command, args });
        return collection(12, "Clientes", 7);
      },
    },
  };

  const child = await collectionsCreateSubcollectionCommand({
    parentId: 7,
    name: "Clientes",
  });

  assert.deepEqual(calls, [
    {
      command: "clipvault_collections_create_subcollection",
      args: { parentId: 7, name: "Clientes" },
    },
  ]);
  assert.equal(child.parent_id, 7);
});
