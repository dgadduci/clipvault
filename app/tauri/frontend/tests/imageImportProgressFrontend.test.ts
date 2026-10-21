import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

const modalSource = readFileSync(
  resolvePath(process.cwd(), "src", "ImageImportOrderModal.svelte"),
  "utf8",
);
const appSource = readFileSync(
  resolvePath(process.cwd(), "src", "App.svelte"),
  "utf8",
);
const typesSource = readFileSync(
  resolvePath(process.cwd(), "src", "types.ts"),
  "utf8",
);

test("image order modal exposes accessible numeric progress and keeps importing label", () => {
  assert.ok(modalSource.includes('role="progressbar"'));
  assert.ok(modalSource.includes("aria-valuenow={progressPercent}"));
  assert.ok(modalSource.includes("{progressPercent}%"));
  assert.ok(modalSource.includes('$t("image_import.order.importing")'));
});

test("image import progress is subscribed before the command and scoped to its session", () => {
  const handlerStart = appSource.indexOf("async function confirmDesktopImageImport");
  assert.notEqual(handlerStart, -1);
  const handler = appSource.slice(handlerStart, appSource.indexOf("function cancelDesktopImageImport", handlerStart));
  const listenerIndex = handler.indexOf("await listen<ImageImportProgressEvent>");
  const commandIndex = handler.indexOf("await completeImageImportCommand");
  assert.ok(listenerIndex >= 0 && commandIndex > listenerIndex);
  assert.ok(handler.includes("payload.session_id !== pending.sessionId"));
  assert.ok(handler.includes("calculateImageImportProgressPercent(payload)"));
  assert.ok(handler.indexOf("imageImportProgressPercent = 100") > commandIndex);
});

test("image import progress event type carries only the session and numeric counts", () => {
  const typeStart = typesSource.indexOf("export interface ImageImportProgressEvent");
  assert.notEqual(typeStart, -1);
  const typeBlock = typesSource.slice(typeStart, typesSource.indexOf("}\n", typeStart));
  assert.match(typeBlock, /session_id:\s*number/);
  assert.match(typeBlock, /bytes_read:\s*number/);
  assert.match(typeBlock, /total_bytes:\s*number/);
  assert.match(typeBlock, /completed:\s*number/);
  assert.match(typeBlock, /total:\s*number/);
  assert.doesNotMatch(typeBlock, /path|filename|hash|content/i);
});
