import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

import { ACTIVE_FILTER_SURFACES } from "../src/lib/filterTokens.ts";
import { COLLECTION_ROW_TINT_OPACITY } from "../src/lib/collectionColor.ts";

const FRONTEND_ROOT = process.cwd();

function loadSource(file: string): string {
  return readFileSync(path.join(FRONTEND_ROOT, file), "utf8");
}

test("toolbar filters have distinct category colors with white foregrounds", () => {
  assert.deepEqual(ACTIVE_FILTER_SURFACES, {
    sourceApp: `rgba(37, 99, 235, ${COLLECTION_ROW_TINT_OPACITY})`,
    tag: `rgba(109, 40, 217, ${COLLECTION_ROW_TINT_OPACITY})`,
    collection: `rgba(15, 118, 110, ${COLLECTION_ROW_TINT_OPACITY})`,
    contentType: `rgba(180, 83, 9, ${COLLECTION_ROW_TINT_OPACITY})`,
  });
  for (const surface of Object.values(ACTIVE_FILTER_SURFACES)) {
    assert.equal(Number(surface.match(/, ([\d.]+)\)$/)?.[1]), COLLECTION_ROW_TINT_OPACITY);
  }
  assert.equal(new Set(Object.values(ACTIVE_FILTER_SURFACES)).size, 4);

  const filters = [
    {
      file: "src/SourceAppFilter.svelte",
      surface: "sourceApp",
      activeState: 'data-filter-active={selected.kind !== "all"}',
      activeSelector: '.trigger[data-filter-active="true"]',
    },
    {
      file: "src/TagFilter.svelte",
      surface: "tag",
      activeState: 'data-filter-active={selected.kind === "tag"}',
      activeSelector: '.trigger[data-filter-active="true"]',
    },
    {
      file: "src/CollectionFilter.svelte",
      surface: "collection",
      activeState: 'data-filter-active={selected.kind !== "all"}',
      activeSelector: '.collection-filter[data-filter-active="true"] select',
    },
    {
      file: "src/ContentTypeFilter.svelte",
      surface: "contentType",
      activeState: "data-filter-active={selected !== null}",
      activeSelector: '.trigger[data-filter-active="true"]',
    },
  ] as const;

  for (const filter of filters) {
    const source = loadSource(filter.file);
    assert.ok(
      source.includes(`ACTIVE_FILTER_SURFACES.${filter.surface}`),
      `${filter.file} uses its distinct surface`,
    );
    assert.ok(
      source.includes(filter.activeState),
      `${filter.file} derives active state from selection`,
    );
    const styleStart = source.indexOf(filter.activeSelector, source.indexOf("<style>"));
    assert.notEqual(styleStart, -1, `${filter.file} styles only its active control`);
    const styleEnd = source.indexOf("}", styleStart);
    const activeRule = source.slice(styleStart, styleEnd);
    assert.match(activeRule, /background:\s*var\(--filter-active-background\)/);
    assert.match(activeRule, /color:\s*#fff/);
  }
});

test("the all/unselected states remain neutral", () => {
  const sourceApp = loadSource("src/SourceAppFilter.svelte");
  const tag = loadSource("src/TagFilter.svelte");
  const collection = loadSource("src/CollectionFilter.svelte");
  const contentType = loadSource("src/ContentTypeFilter.svelte");

  assert.match(sourceApp, /data-filter-active=\{selected\.kind !== "all"\}/);
  assert.match(tag, /data-filter-active=\{selected\.kind === "tag"\}/);
  assert.match(collection, /data-filter-active=\{selected\.kind !== "all"\}/);
  assert.match(contentType, /data-filter-active=\{selected !== null\}/);
});
