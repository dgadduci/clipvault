import { test } from "node:test";
import assert from "node:assert/strict";

import { projectSubcollectionVisibility } from "../src/lib/subcollectionVisibility.ts";
import type { Collection } from "../src/types.ts";

const collections = [
  { id: 10, parent_id: null },
  { id: 11, parent_id: 10 },
  { id: 12, parent_id: 10 },
  { id: 20, parent_id: null },
  { id: 21, parent_id: 20 },
] satisfies Pick<Collection, "id" | "parent_id">[];

const entries = [{ id: 1 }, { id: 2 }, { id: 3 }, { id: 4 }];
const memberships = new Map([
  [1, { collections: [{ id: 10 }] }],
  [2, { collections: [{ id: 10 }, { id: 11 }] }],
  [3, { collections: [{ id: 10 }, { id: 12 }, { id: 20 }] }],
  [4, { collections: [{ id: 10 }, { id: 21 }] }],
]);
const loaded = new Map(entries.map(({ id }) => [id, "loaded" as const]));

test("visible mode preserves the principal collection's direct results", () => {
  assert.deepEqual(
    projectSubcollectionVisibility(
      entries,
      10,
      collections,
      memberships,
      loaded,
      true,
    ),
    entries,
  );
});

test("hidden mode excludes entries shared with any direct child only", () => {
  assert.deepEqual(
    projectSubcollectionVisibility(
      entries,
      10,
      collections,
      memberships,
      loaded,
      false,
    ),
    [{ id: 1 }, { id: 4 }],
  );
});

test("hidden mode omits entries without confirmed membership metadata", () => {
  const states = new Map([
    [1, "loaded"],
    [2, "loaded"],
    [3, "error"],
  ] as const);
  const results = projectSubcollectionVisibility(
    [{ id: 1 }, { id: 2 }, { id: 3 }, { id: 4 }],
    10,
    collections,
    memberships,
    states,
    false,
  );
  assert.deepEqual(results, [{ id: 1 }]);
});

test("History and collections without children do not apply this filter", () => {
  assert.deepEqual(
    projectSubcollectionVisibility(
      entries,
      null,
      collections,
      memberships,
      new Map(),
      false,
    ),
    entries,
  );
  assert.deepEqual(
    projectSubcollectionVisibility(
      entries,
      30,
      collections,
      memberships,
      new Map(),
      false,
    ),
    entries,
  );
});
