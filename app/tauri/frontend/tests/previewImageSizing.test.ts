import { test } from "node:test";
import assert from "node:assert/strict";
import {
  clampPreviewCardSize,
  fitPreviewImage,
} from "../src/lib/previewImageSizing.ts";

test("preview image sizing uses 80 percent and preserves portrait proportions", () => {
  assert.deepEqual(fitPreviewImage(600, 1200, 1000, 1000), {
    width: 480,
    height: 960,
  });
});

test("preview image sizing fits landscape images to the available width", () => {
  const size = fitPreviewImage(2400, 1200, 500, 900);
  assert.ok(size);
  assert.equal(size.width, 500);
  assert.equal(size.height, 250);
  assert.equal(size.width / size.height, 2);
});

test("preview image sizing fits portrait images to the available height", () => {
  const size = fitPreviewImage(1200, 2400, 900, 300);
  assert.ok(size);
  assert.equal(size.width, 150);
  assert.equal(size.height, 300);
  assert.equal(size.width / size.height, 0.5);
});

test("preview image sizing rejects invalid dimensions and viewports", () => {
  assert.equal(fitPreviewImage(0, 100, 100, 100), null);
  assert.equal(fitPreviewImage(100, 100, 0, 100), null);
  assert.equal(fitPreviewImage(Number.NaN, 100, 100, 100), null);
});

test("an explicitly enlarged preview may scale up to the image's intrinsic size", () => {
  assert.deepEqual(fitPreviewImage(600, 1200, 1000, 1000, 1), {
    width: 500,
    height: 1000,
  });
});

test("manual preview card dimensions stay inside the viewport bounds", () => {
  assert.deepEqual(clampPreviewCardSize(900, 700, 500, 300), {
    width: 500,
    height: 300,
  });
  assert.deepEqual(clampPreviewCardSize(10, 10, 500, 300), {
    width: 160,
    height: 120,
  });
  assert.deepEqual(clampPreviewCardSize(10, 10, 100, 80), {
    width: 100,
    height: 80,
  });
  assert.equal(clampPreviewCardSize(100, 100, 0, 80), null);
});
