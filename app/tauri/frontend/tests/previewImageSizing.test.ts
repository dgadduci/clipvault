import { test } from "node:test";
import assert from "node:assert/strict";
import {
  fitPreviewImage,
  initialImagePreviewWindowSize,
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

test("automatic preview fitting fills the viewport without exceeding intrinsic size", () => {
  assert.deepEqual(fitPreviewImage(600, 1200, 1000, 1000, 1), {
    width: 500,
    height: 1000,
  });
});

test("native preview fitting upscales beyond intrinsic size and preserves proportions", () => {
  assert.deepEqual(fitPreviewImage(600, 300, 1200, 1000, null), {
    width: 1200,
    height: 600,
  });
});

test("initial native preview window fits a portrait image to the monitor", () => {
  const size = initialImagePreviewWindowSize(600, 1200, 1440, 900);
  assert.ok(size.width <= 1440 * 0.94);
  assert.ok(size.height <= 900 * 0.92);
  assert.ok(size.width >= 420);
  assert.ok(size.height >= 320);
});

test("initial native preview window preserves room on small displays", () => {
  const size = initialImagePreviewWindowSize(4000, 2000, 800, 600);
  assert.ok(size.width <= 800 * 0.94);
  assert.ok(size.height <= 600 * 0.92);

  const fitted = fitPreviewImage(4000, 2000, size.width - 56, size.height - 160);
  assert.ok(fitted);
  assert.equal(fitted.width / fitted.height, 2);
});

test("initial native preview window uses safe defaults for missing dimensions", () => {
  assert.deepEqual(initialImagePreviewWindowSize(0, 0, 1280, 800), {
    width: 960,
    height: 720,
  });
  assert.deepEqual(initialImagePreviewWindowSize(900, 900, 0, 0), {
    width: 632,
    height: 736,
  });
});
