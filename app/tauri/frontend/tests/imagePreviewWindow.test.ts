import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";
import { imagePreviewRoute } from "../src/lib/imagePreviewWindow.ts";

const appSource = readFileSync(
  resolvePath(process.cwd(), "src", "App.svelte"),
  "utf8",
);
const quickPasteSource = readFileSync(
  resolvePath(process.cwd(), "src", "QuickPaste.svelte"),
  "utf8",
);
const previewPageSource = readFileSync(
  resolvePath(process.cwd(), "src", "ImagePreviewWindow.svelte"),
  "utf8",
);
const nativeWindowSource = readFileSync(
  resolvePath(process.cwd(), "../src-tauri/src/main.rs"),
  "utf8",
);
const previewCapability = JSON.parse(readFileSync(
  resolvePath(process.cwd(), "../src-tauri/capabilities/image-preview.json"),
  "utf8",
)) as { permissions: string[] };

test("image preview route carries only the local capture ID and origin", () => {
  const route = imagePreviewRoute(42, "quick-paste");
  const parsed = new URL(route, "http://localhost");

  assert.equal(parsed.pathname, "/preview.html");
  assert.deepEqual([...parsed.searchParams.entries()], [
    ["entry_id", "42"],
    ["origin", "quick-paste"],
  ]);
});

test("Desktop routes image captures to the native preview window", () => {
  assert.match(
    appSource,
    /if \(isImageEntry\(entry\)\) \{[\s\S]*?openImagePreviewWindow\(\s*entry,\s*"main"/,
  );
  assert.match(appSource, /previewEntry = entry;[\s\S]*?previewTrigger = trigger;/);
});

test("QuickPaste routes image captures to the native preview window", () => {
  assert.match(
    quickPasteSource,
    /if \(isImageEntry\(entry\)\) \{[\s\S]*?openImagePreviewWindow\(\s*entry,\s*"quick-paste"/,
  );
  assert.match(quickPasteSource, /previewEntryId = entryId;/);
});

test("the native preview is direct page content with rotation controls and reliable close", () => {
  assert.match(previewPageSource, /standaloneImage=\{true\}/);
  assert.match(previewPageSource, /window\.addEventListener\("keydown", onPreviewKeydown\)/);
  assert.match(previewPageSource, /window\.removeEventListener\("keydown", onPreviewKeydown\)/);
  assert.match(previewPageSource, /getCurrentWindow\(\)\.destroy\(\)/);
  assert.equal(previewPageSource.includes("image-preview-fallback-close"), false);
  assert.match(nativeWindowSource, /window\.label\(\) == "image-preview"[\s\S]*?window\.destroy\(\)/);
  assert.match(nativeWindowSource, /label == "main" \|\| label == "quick-paste"/);
  assert.ok(previewCapability.permissions.includes("core:window:allow-destroy"));
  assert.ok(
    readFileSync(resolvePath(process.cwd(), "src", "ClipboardPreview.svelte"), "utf8")
      .includes('data-testid="{testIdPrefix}-rotate-counterclockwise"'),
    "the preview toolbar must expose its rotation controls",
  );
});

test("the native image preview disables webview zoom and starts at its default scale", () => {
  const previewWindowSource = readFileSync(
    resolvePath(process.cwd(), "src", "lib", "imagePreviewWindow.ts"),
    "utf8",
  );

  assert.match(previewWindowSource, /zoomHotkeysEnabled:\s*false/);
  assert.match(previewPageSource, /getCurrentWebview\(\)\.setZoom\(1\)/);
  assert.ok(previewCapability.permissions.includes("core:webview:allow-set-webview-zoom"));
});
