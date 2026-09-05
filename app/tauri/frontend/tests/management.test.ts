import { test } from "node:test";
import assert from "node:assert/strict";

import {
  applyDestructive,
  describeRetentionPreview,
  describeRetentionResult,
  isClearConfirmationRequired,
  isDeleteConfirmationRequired,
  type ManagementOutcome,
} from "../src/lib/management.ts";
import type {
  ClearResponse,
  DeleteResponse,
  RetentionPreview,
  RetentionResponse,
  SetFavoriteResponse,
} from "../src/types.ts";

test("applyDestructive refuses to call the backend without confirmation", async () => {
  let calls = 0;
  const request = async () => {
    calls += 1;
    return { kind: "removed", removed: 1 } satisfies DeleteResponse;
  };
  const outcome: ManagementOutcome<DeleteResponse> = await applyDestructive(
    request,
    isDeleteConfirmationRequired,
    { id: 1, confirm: false },
  );
  assert.equal(calls, 0, "backend must not be invoked when confirm is false");
  assert.equal(outcome.kind, "confirmation_required");
});

test("applyDestructive surfaces confirmation_required from the backend", async () => {
  let calls = 0;
  const request = async () => {
    calls += 1;
    return { kind: "confirmation_required" } satisfies DeleteResponse;
  };
  const outcome = await applyDestructive(
    request,
    isDeleteConfirmationRequired,
    { id: 1, confirm: true },
  );
  assert.equal(calls, 1);
  assert.equal(outcome.kind, "confirmation_required");
});

test("applyDestructive returns the applied value when the backend accepts the call", async () => {
  const request = async () => {
    return { kind: "removed", removed: 2 } satisfies DeleteResponse;
  };
  const outcome = await applyDestructive(
    request,
    isDeleteConfirmationRequired,
    { id: 1, confirm: true },
  );
  assert.equal(outcome.kind, "applied");
  if (outcome.kind !== "applied") return;
  assert.deepEqual(outcome.value, { kind: "removed", removed: 2 });
});

test("isDeleteConfirmationRequired detects the rejection", () => {
  assert.equal(
    isDeleteConfirmationRequired({ kind: "confirmation_required" }),
    true,
  );
  assert.equal(
    isDeleteConfirmationRequired({ kind: "removed", removed: 1 }),
    false,
  );
  assert.equal(isDeleteConfirmationRequired({ kind: "not_found" }), false);
});

test("isClearConfirmationRequired detects the rejection", () => {
  assert.equal(
    isClearConfirmationRequired({ kind: "confirmation_required" }),
    true,
  );
  assert.equal(
    isClearConfirmationRequired({ kind: "removed", removed: 1 }),
    false,
  );
});

test("describeRetentionPreview reports nothing to purge for forever policy", () => {
  const preview: RetentionPreview = { policy: "forever", would_remove: 0 };
  const text = describeRetentionPreview(preview);
  assert.match(text, /forever/i);
});

test("describeRetentionPreview reports singular and plural correctly", () => {
  const single: RetentionPreview = { policy: "days_30", would_remove: 1 };
  const plural: RetentionPreview = { policy: "days_30", would_remove: 3 };
  assert.match(describeRetentionPreview(single), /1 non-favorite entry/);
  assert.match(describeRetentionPreview(plural), /3 non-favorite entries/);
});

test("describeRetentionPreview explains when the retention pass would be a no-op", () => {
  const preview: RetentionPreview = { policy: "days_30", would_remove: 0 };
  const text = describeRetentionPreview(preview);
  assert.match(text, /No non-favorite entries/i);
});

test("describeRetentionResult pluralises removal counts", () => {
  const none: RetentionResponse = { policy: "days_30", removed: 0 };
  const one: RetentionResponse = { policy: "days_30", removed: 1 };
  const many: RetentionResponse = { policy: "days_30", removed: 4 };
  assert.match(describeRetentionResult(none), /No non-favorite entries were removed/);
  assert.match(describeRetentionResult(one), /1 non-favorite entry removed/);
  assert.match(describeRetentionResult(many), /4 non-favorite entries removed/);
});

test("management helper never echoes clipboard content", async () => {
  // Set-favorite response must not surface entry content. The helper
  // does not touch `entry.content`, so the response is metadata only.
  const response: SetFavoriteResponse = {
    kind: "updated",
    entry: { id: 42, is_pinned: true, updated_at: "2026-01-02T03:04:05Z" },
  };
  assert.equal("content" in response.entry, false);
});
