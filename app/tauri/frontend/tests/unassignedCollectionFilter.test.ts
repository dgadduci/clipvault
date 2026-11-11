import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

const readSource = (...segments: string[]) =>
  readFileSync(path.join(process.cwd(), ...segments), "utf8");

const appSource = readSource("src", "App.svelte");
const toolbarSource = readSource("src", "DesktopToolbar.svelte");
const filterSource = readSource("src", "CollectionFilter.svelte");

test("collection filter offers the unassigned History scope", () => {
  assert.match(filterSource, /\{ kind: "unassigned" \}/);
  assert.match(filterSource, /value="unassigned"[^\n]*filter\.collection_unassigned/);
  assert.match(filterSource, /onChange\(\{ kind: "unassigned" \}\)/);
  assert.match(filterSource, /data-filter-active=\{selected\.kind !== "all"\}/);
});

test("collection filter remains mounted and is disabled outside History", () => {
  const toolbarMarkup = toolbarSource.slice(toolbarSource.indexOf("</script>"));
  assert.match(toolbarMarkup, /<CollectionFilter[\s\S]*?disabled=\{collectionFilterDisabled\}/);
  assert.equal(toolbarMarkup.includes("{#if showCollectionFilter}"), false);
  assert.match(appSource, /collectionFilterDisabled=\{!activeCollectionIsHistory\}/);
  assert.match(filterSource, /disabled=\{disabled\}/);
  assert.match(filterSource, /data-filter-disabled=\{disabled\}/);
});

test("recents, search and available type options receive the unassigned scope", () => {
  assert.match(
    appSource,
    /function entriesUnassignedOnly\(collectionId: number \| null\): boolean[\s\S]*?collectionFilter\.kind !== "unassigned"/,
  );
  assert.equal(
    (appSource.match(/unassignedOnly: entriesUnassignedOnly\(selectedCollectionId\)/g) ?? []).length,
    3,
  );
  assert.match(appSource, /collectionFilter\.kind === "unassigned"/);
});
