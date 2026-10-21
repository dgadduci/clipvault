import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

const loadSource = (...segments: string[]) =>
  readFileSync(path.join(process.cwd(), ...segments), "utf8");

const filterSource = loadSource("src/ContentTypeFilter.svelte");
const toolbarSource = loadSource("src/DesktopToolbar.svelte");
const appSource = loadSource("src/App.svelte");
const bridgeSource = loadSource("src/lib/tauri.ts");

test("content type filter exposes an accessible, keyboard-operated listbox", () => {
  for (const contract of [
    'aria-haspopup="listbox"',
    "aria-expanded={triggerAriaExpanded as \"true\" | \"false\"}",
    'role="listbox"',
    'role="option"',
    'data-testid={`${testId}-trigger`}',
  ]) {
    assert.ok(filterSource.includes(contract), `missing ${contract}`);
  }
  for (const key of ["ArrowDown", "ArrowUp", "Home", "End", "Enter", "Escape"]) {
    assert.ok(filterSource.includes(`"${key}"`), `keyboard handling misses ${key}`);
  }
  assert.match(
    filterSource,
    /function onTriggerKeydown\(event: KeyboardEvent\): void \{\s*if \(open\) \{\s*onListboxKeydown\(event\);/,
    "an open combobox must route trigger key events to listbox navigation",
  );
  assert.match(
    filterSource,
    /event\.key === "Enter" \|\| event\.key === " "[\s\S]*?selectKey\(optionKeys\[activeIndex\]\)/,
    "Enter and Space must select the active option",
  );
});

test("type options use the card icon sprite and localized labels", () => {
  assert.ok(filterSource.includes("CONTENT_TYPE_ICON_SPRITE"));
  assert.ok(filterSource.includes("contentTypeIconId("));
  assert.ok(filterSource.includes("contentTypeTranslationKey("));
  assert.ok(filterSource.includes('{ kind: "all", key: "all" }'));
});

test("DesktopToolbar renders one type filter before Text and the overflow menu", () => {
  const markup = toolbarSource.slice(toolbarSource.indexOf("</script>"));
  const typeFilterIndex = markup.indexOf("<ContentTypeFilter");
  const textActionIndex = markup.indexOf('data-testid="create-manual-text-entry"');
  const overflowIndex = markup.indexOf('data-testid="open-overflow-menu"');
  assert.ok(typeFilterIndex >= 0, "the toolbar must mount the content type filter");
  assert.ok(typeFilterIndex < textActionIndex, "the type filter precedes the Text action");
  assert.ok(textActionIndex < overflowIndex, "the Text action precedes the overflow menu");
  assert.equal(markup.split("<ContentTypeFilter").length - 1, 1);
});

test("App loads complete scoped type options and composes the selection with recents/search", () => {
  assert.match(appSource, /contentTypesCommand\(\{/);
  assert.match(appSource, /collectionId:\s*requestCollectionId/);
  assert.match(
    appSource,
    /unassignedOnly:\s*entriesUnassignedOnly\(selectedCollectionId\)/,
  );
  assert.match(appSource, /orderAvailableContentTypes\(availableTypes\s*\?\?\s*\[\]\)/);
  assert.equal((appSource.match(/contentType:\s*selectedContentType/g) ?? []).length, 2);
  assert.match(appSource, /function\s+handleContentTypeFilterChange/);
  assert.match(appSource, /selectedContentType\s*=\s*null/);
});

test("Tauri bridge requests only scoped content type metadata", () => {
  assert.match(bridgeSource, /"clipvault_content_types"/);
  assert.match(bridgeSource, /collectionId:\s*args\.collectionId\s*\?\?\s*null/);
});
