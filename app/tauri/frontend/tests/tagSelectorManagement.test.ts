import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const source = (path: string): string =>
  readFileSync(resolve(process.cwd(), path), "utf8");

test("tag and collection filter options expose their complete labels on hover", () => {
  const tagFilter = source("src/TagFilter.svelte");
  const collectionFilter = source("src/CollectionFilter.svelte");

  assert.match(
    tagFilter,
    /title=\{option\.kind === "all" \? \$t\("filter\.all"\) : option\.option\.display_name\}/,
  );
  assert.match(
    collectionFilter,
    /<option value=\{String\(collection\.id\)\} title=\{collection\.name\}>\{collection\.name\}<\/option>/,
  );
});

test("a newly created tag stays selected across organization hydration", () => {
  const modal = source("src/TagSelectorModal.svelte");

  assert.match(
    modal,
    /const created = await tagsCreateCommand\(\{ name: trimmed \}\);[\s\S]*?selectTag\(created\.id\)/,
  );
  assert.match(modal, /checked=\{isSelected\(tag\.id\)\}/);
  assert.match(
    modal,
    /for \(const tagId of pendingAddedTagIds\) hydratedSelection\.add\(tagId\)/,
  );
  assert.match(modal, /if \(justOpened\)\s*\{\s*reset\(initialSelection\)/);
});

test("tag deletion requires confirmation and refreshes every loaded card", () => {
  const modal = source("src/TagSelectorModal.svelte");
  const app = source("src/App.svelte");

  assert.match(modal, /data-testid="tag-selector-delete"/);
  assert.match(modal, /data-testid="tag-selector-delete-confirmation"/);
  assert.match(modal, /data-testid="tag-selector-delete-cancel"/);
  assert.match(
    modal,
    /async function confirmDeleteTag\(tagId: number\)[\s\S]*?await tagsDeleteCommand\(\{ tagId \}\);[\s\S]*?nextSelection\.delete\(tagId\)/,
  );
  assert.match(modal, /catch\s*\{\s*deleteErrorKey = "tags\.selector\.delete_error"/);
  assert.match(modal, /function cancelDeleteTag\(\): void[\s\S]*?pendingDeleteTagId = null/);

  const refreshAll = app.match(
    /async function refreshOrganizationForAllEntries\(\)[\s\S]*?\n  \}/,
  );
  assert.ok(refreshAll);
  assert.match(refreshAll[0], /await refreshOrganization\(\)/);
  assert.match(refreshAll[0], /await hydrateEntryOrganization\(entries, \{ force: true \}\)/);
  assert.match(
    app,
    /async function handleOrganizationUpdated\(\)[\s\S]*?await refreshOrganizationForAllEntries\(\)/,
  );
});
