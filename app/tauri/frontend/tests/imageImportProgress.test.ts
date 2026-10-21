import { test } from "node:test";
import assert from "node:assert/strict";
import { calculateImageImportProgressPercent } from "../src/lib/imageImportProgress.ts";

const progress = (
  bytes_read: number,
  total_bytes: number,
  completed: number,
  total: number,
) => ({ session_id: 1, bytes_read, total_bytes, completed, total });

test("image import progress is primarily driven by bytes read", () => {
  assert.equal(calculateImageImportProgressPercent(progress(500, 1000, 0, 2)), 45);
});

test("completed files contribute processing progress after bytes are read", () => {
  assert.equal(calculateImageImportProgressPercent(progress(1000, 1000, 1, 2)), 95);
});

test("event progress never reaches 100 before the import command resolves", () => {
  assert.equal(calculateImageImportProgressPercent(progress(1000, 1000, 2, 2)), 99);
});

test("file progress is used when total byte size is unavailable", () => {
  assert.equal(calculateImageImportProgressPercent(progress(0, 0, 1, 4)), 25);
});
