import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";
import {
  IMAGE_PREVIEW_ZOOM_MAX,
  IMAGE_PREVIEW_ZOOM_MIN,
  IMAGE_PREVIEW_ZOOM_STEP,
  scaleImagePreviewZoom,
  zoomedImagePreviewSize,
} from "../src/lib/imagePreviewZoom.ts";

const previewSource = readFileSync(
  resolvePath(process.cwd(), "src", "ClipboardPreview.svelte"),
  "utf8",
);

test("image preview zoom math supports buttons and clamps gestures", () => {
  assert.equal(scaleImagePreviewZoom(1, IMAGE_PREVIEW_ZOOM_STEP), 1.25);
  assert.equal(scaleImagePreviewZoom(1, 1 / IMAGE_PREVIEW_ZOOM_STEP), 0.8);
  assert.equal(scaleImagePreviewZoom(3.5, IMAGE_PREVIEW_ZOOM_STEP), IMAGE_PREVIEW_ZOOM_MAX);
  assert.equal(scaleImagePreviewZoom(0.3, 0.5), IMAGE_PREVIEW_ZOOM_MIN);
  assert.equal(scaleImagePreviewZoom(1.4, Number.NaN), 1.4);
  assert.equal(scaleImagePreviewZoom(Number.POSITIVE_INFINITY, 1), 1);
});

test("zoomed image layout dimensions preserve aspect ratio", () => {
  const size = zoomedImagePreviewSize({ width: 1200, height: 600 }, 1.5);

  assert.deepEqual(size, { width: 1800, height: 900 });
  assert.equal(size!.width / size!.height, 2);
  assert.equal(zoomedImagePreviewSize(null, 2), null);
});

test("zoom buttons share the standalone image toolbar with rotation controls", () => {
  const actionsStart = previewSource.indexOf('<div class="cv-preview-header-actions">');
  const actionsEnd = previewSource.indexOf("</div>", actionsStart);
  assert.notEqual(actionsStart, -1);
  assert.notEqual(actionsEnd, -1);
  const actions = previewSource.slice(actionsStart, actionsEnd);

  assert.ok(actions.includes('data-testid="{testIdPrefix}-rotate-counterclockwise"'));
  assert.ok(actions.includes('data-testid="{testIdPrefix}-rotate-clockwise"'));
  assert.ok(actions.includes('data-testid="{testIdPrefix}-zoom-in"'));
  assert.ok(actions.includes('data-testid="{testIdPrefix}-zoom-out"'));
  assert.match(actions, /\{#if standaloneImage\}[\s\S]*?data-testid="\{testIdPrefix\}-zoom-in"/);
  assert.match(actions, /aria-label=\{\$t\("preview\.zoom_in"\)\}/);
  assert.match(actions, /aria-label=\{\$t\("preview\.zoom_out"\)\}/);
});

test("touchpad zoom is intercepted only for the standalone image while ordinary wheel remains available for scrolling", () => {
  assert.match(previewSource, /if \(!isStandaloneImagePreview\(\) \|\| \(!event\.ctrlKey && !event\.metaKey\)\) return;/);
  assert.match(previewSource, /function isStandaloneImagePreview\(\): boolean/);
  assert.match(previewSource, /event\.preventDefault\(\);[\s\S]*?Math\.exp\(-event\.deltaY \* 0\.002\)/);
  assert.match(previewSource, /document\.addEventListener\("gesturestart", onImageGestureStart, \{\s*capture: true,\s*passive: false/);
  assert.match(previewSource, /document\.addEventListener\("gesturechange", onImageGestureChange, \{\s*capture: true,\s*passive: false/);
  assert.match(previewSource, /document\.addEventListener\("gestureend", onImageGestureEnd, \{\s*capture: true,\s*passive: false/);
  assert.match(previewSource, /\$: displayedPreviewImageSize = zoomedImagePreviewSize\(/);
  assert.match(previewSource, /width: \$\{displayedPreviewImageSize\.width\}px; height: \$\{displayedPreviewImageSize\.height\}px;/);
  assert.doesNotMatch(previewSource, /style:transform=\{standaloneImage/);
  assert.match(previewSource, /document\.removeEventListener\("gesturestart", onImageGestureStart, true\)/);
  assert.match(previewSource, /document\.removeEventListener\("gesturechange", onImageGestureChange, true\)/);
  assert.match(previewSource, /document\.removeEventListener\("gestureend", onImageGestureEnd, true\)/);
});

test("only the standalone image preview scrolls on both axes when zoomed content exceeds its viewport", () => {
  const standaloneBodyRule = previewSource.match(
    /\.cv-preview-standalone \.cv-preview-body-image-standalone\s*\{([^}]+)\}/,
  )?.[1];
  const standaloneImageRule = previewSource.match(
    /\.cv-preview-image-standalone\s*\{([^}]+)\}/,
  )?.[1];

  assert.ok(standaloneBodyRule);
  assert.match(standaloneBodyRule, /overflow:\s*auto/);
  assert.match(standaloneBodyRule, /align-items:\s*flex-start/);
  assert.match(standaloneBodyRule, /justify-content:\s*flex-start/);
  assert.ok(standaloneImageRule);
  assert.match(standaloneImageRule, /max-width:\s*none/);
  assert.match(standaloneImageRule, /max-height:\s*none/);
  assert.match(standaloneImageRule, /margin:\s*auto/);

  const embeddedBodyRule = previewSource.match(
    /\.cv-preview-body-image\s*\{([^}]+)\}/,
  )?.[1];
  assert.ok(embeddedBodyRule);
  assert.match(embeddedBodyRule, /overflow:\s*hidden/);
});
