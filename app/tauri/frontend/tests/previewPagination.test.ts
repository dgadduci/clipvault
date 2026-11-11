import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

const root = process.cwd();
const appSource = readFileSync(path.join(root, "src", "App.svelte"), "utf8");
const settingsSource = readFileSync(
  path.join(root, "src", "GeneralSettingsModal.svelte"),
  "utf8",
);
const remoteRailSource = readFileSync(
  path.join(root, "src", "RemoteHistoryRail.svelte"),
  "utf8",
);

test("desktop paginates filtered collection and history queries with an offset", () => {
  assert.match(appSource, /async function loadEntries\(\)/);
  assert.match(appSource, /limit:\s*previewPageSize\s*\+\s*1/);
  assert.match(
    appSource,
    /offset:\s*\(localPage\s*-\s*1\)\s*\*\s*previewPageSize/,
  );
  assert.match(appSource, /data-testid="local-preview-pager"/);
  assert.match(appSource, /goToLocalPage\(localPage\s*\+\s*1\)/);
  assert.match(appSource, /goToLocalPage\(localPage\s*-\s*1\)/);
  assert.match(
    appSource,
    /async function selectCollectionById[\s\S]{0,1200}selectedCollectionId\s*=\s*targetCollectionId;[\s\S]{0,180}localPage\s*=\s*1;/,
  );
});

test("general settings validates and saves the shared preview page size", () => {
  assert.match(settingsSource, /min="1"[\s\S]*max="50"/);
  assert.match(settingsSource, /settingsSetCommand\(\{\s*preview_page_size:\s*requested\s*\}\)/);
  assert.match(settingsSource, /dispatch\("previewPageSizeChanged"/);
});

test("linked-device import previews receive the configured page size", () => {
  assert.equal((remoteRailSource.match(/limit:\s*pageSize/g) ?? []).length, 2);
  assert.match(remoteRailSource, /pageSize\s*=\s*20/);
  assert.match(appSource, /pageSize=\{previewPageSize\}/);
});
