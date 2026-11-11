import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";

const appSource = readFileSync(
  path.join(process.cwd(), "src", "App.svelte"),
  "utf8",
);

test("collection switch clears search and invalidates pending search results", () => {
  const resetMatch = appSource.match(
    /function resetSearchForCollectionSwitch\(\): void \{[\s\S]*?\n  \}/,
  );
  assert.ok(resetMatch, "collection switch must have a search reset helper");
  assert.match(resetMatch[0], /searchQuery\s*=\s*""/);
  assert.match(resetMatch[0], /searchToken\s*\+=\s*1/);
  assert.match(resetMatch[0], /searchController\?\.cancel\(\)/);
  assert.match(resetMatch[0], /searchController\s*=\s*null/);
  assert.match(resetMatch[0], /searchStatus\s*=\s*"idle"/);

  const selectMatch = appSource.match(
    /async function selectCollectionById\([\s\S]*?\n  \}/,
  );
  assert.ok(selectMatch, "sidebar collection selection handler must exist");
  assert.match(selectMatch[0], /resetSearchForCollectionSwitch\(\)/);
  assert.match(selectMatch[0], /sourceAppFilter\s*=\s*\{\s*kind:\s*"all"\s*\}/);
  assert.match(selectMatch[0], /tagFilter\s*=\s*\{\s*kind:\s*"all"\s*\}/);
  assert.match(selectMatch[0], /collectionFilter\s*=\s*\{\s*kind:\s*"all"\s*\}/);
  assert.match(selectMatch[0], /await refreshEntries\(\)/);
});
