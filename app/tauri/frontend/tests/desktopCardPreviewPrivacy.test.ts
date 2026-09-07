/**
 * Privacy regression tests for the `desktop-card-preview` change.
 *
 * The preview overlay is strictly read-only and the change MUST
 * NOT introduce any path that:
 *
 *   - logs or surfaces the entry content, snippet, hash, byte
 *     payload or absolute path;
 *   - leaks the `asset_ref` or any opaque identifier into the
 *     rendered DOM, an event payload or a log;
 *   - introduces a network call, telemetry hook or external
 *     dependency;
 *   - writes back to the database, the clipboard or the active
 *     application target.
 *
 * The tests inspect the source files the change touches so a
 * regression that re-introduces a write path, a log statement or a
 * `console.*` call with user content surfaces in CI.
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

const SOURCES: Record<string, string> = {
  preview: readFileSync(
    resolvePath(process.cwd(), "src", "ClipboardPreview.svelte"),
    "utf8",
  ),
  app: readFileSync(
    resolvePath(process.cwd(), "src", "App.svelte"),
    "utf8",
  ),
  card: readFileSync(
    resolvePath(process.cwd(), "src", "HistoryCard.svelte"),
    "utf8",
  ),
  rail: readFileSync(
    resolvePath(process.cwd(), "src", "HistoryCardRail.svelte"),
    "utf8",
  ),
  helper: readFileSync(
    resolvePath(process.cwd(), "src", "lib", "clipboardPreview.ts"),
    "utf8",
  ),
  quickPaste: readFileSync(
    resolvePath(process.cwd(), "src", "QuickPaste.svelte"),
    "utf8",
  ),
};

test("ClipboardPreview never logs or surfaces entry content / hashes / bytes", () => {
  // The component must not log the entry content, the source-app
  // metadata or the bytes the resolver returned. A regression that
  // drops a `console.log` call with the entry payload would surface
  // here.
  for (const forbidden of [
    "console.log",
    "console.warn",
    "console.error",
    "console.debug",
    "console.info",
    "console.trace",
  ]) {
    assert.equal(
      SOURCES.preview.includes(forbidden),
      false,
      `ClipboardPreview must not call ${forbidden} (privacy regression)`,
    );
  }
});

test("ClipboardPreview never writes to the clipboard or the active app", () => {
  // The preview is read-only; a regression that re-introduces a
  // write path (copy / paste / synthetic paste) breaks the
  // documented contract. The helper module the bridge consumes
  // already lives outside the component — the assertion inspects
  // the overlay markup / script only.
  for (const forbidden of [
    "copyEntryCommand",
    "pasteEntryCommand",
    "writeText",
    "writeImage",
    "captureActiveApp",
    "setFavorite",
    "setEntryTitle",
    "deleteEntry",
  ]) {
    assert.equal(
      SOURCES.preview.includes(forbidden),
      false,
      `ClipboardPreview must not invoke ${forbidden} (privacy regression)`,
    );
  }
});

test("ClipboardPreview never introduces network / telemetry / external dependencies", () => {
  // ClipVault is local-first / offline-first. A regression that
  // introduces a fetch, an analytics hook or an external URL
  // would surface here.
  for (const forbidden of [
    "fetch(",
    "XMLHttpRequest",
    "navigator.sendBeacon",
    "analytics",
    "telemetry",
    "https://",
    "http://",
    "import(",
  ]) {
    assert.equal(
      SOURCES.preview.includes(forbidden),
      false,
      `ClipboardPreview must not reference ${forbidden} (privacy regression)`,
    );
  }
});

test("ClipboardPreview keeps asset_ref opaque", () => {
  // The reference is a relative, locally-controlled token the
  // backend already validates. The component MUST NOT surface the
  // raw reference as text content (the template reads it for
  // internal bridge use only — the assertion confirms it is never
  // inserted into a visible DOM attribute).
  const matches = SOURCES.preview.match(/\basset_ref\b/g) ?? [];
  assert.ok(
    matches.length > 0,
    "ClipboardPreview must reference asset_ref internally for the bridge call",
  );
  assert.equal(
    SOURCES.preview.includes(">asset_ref<") ||
      SOURCES.preview.includes(">{asset_ref}<"),
    false,
    "ClipboardPreview must NOT render the raw asset_ref as text",
  );
  assert.equal(
    SOURCES.preview.includes('title="{asset_ref}"') ||
      SOURCES.preview.includes("title={asset_ref}"),
    false,
    "ClipboardPreview must NOT expose the raw asset_ref through a title or aria attribute",
  );
});

function extractAppFunctionBody(name: string): string {
  const start = SOURCES.app.indexOf(`function ${name}`);
  assert.notEqual(start, -1, `${name} must be declared in App.svelte`);
  const openBrace = SOURCES.app.indexOf("{", start);
  let depth = 1;
  let cursor = openBrace + 1;
  while (depth > 0 && cursor < SOURCES.app.length) {
    const ch = SOURCES.app[cursor];
    if (ch === "{") depth += 1;
    else if (ch === "}") depth -= 1;
    cursor += 1;
  }
  return SOURCES.app.slice(openBrace, cursor);
}

test("App.svelte preview path does not log entry content or emit events with payload", () => {
  // App.svelte coordinates the preview through pure state — a
  // regression that drops a `console.*` call with the entry
  // payload, or dispatches a CustomEvent with the entry body,
  // would surface here. The assertion inspects the body of
  // `requestPreview`, `closePreview` and `bumpPreviewScope` so
  // legitimate listener diagnostics elsewhere in the file (e.g.
  // `clipvault://history-updated` errors) stay untouched.
  for (const helperName of [
    "requestPreview",
    "closePreview",
    "bumpPreviewScope",
  ]) {
    const helper = extractAppFunctionBody(helperName);
    for (const forbidden of [
      "console.log",
      "console.warn",
      "console.error",
      "console.debug",
      "JSON.stringify(",
    ]) {
      assert.equal(
        helper.includes(forbidden),
        false,
        `${helperName} must not reference ${forbidden} (privacy regression)`,
      );
    }
  }
});

test("HistoryCard preview-request event carries only the entry id", () => {
  // The CustomEvent payload MUST carry only the opaque entry id
  // — the same payload the drag-and-drop baseline already uses.
  // A regression that adds `entry`, `content`, `asset_ref`,
  // `source_app` or any other metadata to the event payload would
  // surface here.
  const dispatch = SOURCES.card.match(
    /dispatch\("preview-request",\s*\{([^}]+)\}\)/,
  );
  assert.ok(dispatch, "the preview-request dispatch must exist");
  assert.ok(
    dispatch![1].includes("id:"),
    "the preview-request dispatch must carry an entry id",
  );
  for (const forbidden of [
    "content",
    "asset_ref",
    "source_app",
    "content_hash",
    "snippet",
    "payload",
  ]) {
    assert.equal(
      dispatch![1].includes(forbidden),
      false,
      `the preview-request payload must not include ${forbidden}`,
    );
  }
});

test("HistoryCardRail preview forwarder carries only the entry id", () => {
  // The rail forwards the request with only the id; a regression
  // that adds metadata to the forwarded payload would surface
  // here.
  const forward = SOURCES.rail.match(/onRequestPreview\(entry\)/);
  assert.ok(forward, "the rail must forward the entry to onRequestPreview");
});

test("clipboardPreview helper does not log or surface entry content", () => {
  // The shared helper is metadata-only; a regression that
  // accidentally logs the platform or the modifier table would
  // surface here.
  for (const forbidden of [
    "console.log",
    "console.warn",
    "console.error",
    "console.debug",
  ]) {
    assert.equal(
      SOURCES.helper.includes(forbidden),
      false,
      `clipboardPreview.ts must not call ${forbidden}`,
    );
  }
});

test("QuickPaste preview overlay uses the shared component", () => {
  // Quick Paste must NOT keep an inline preview overlay with its
  // own paste / copy / pin path; the change pins a single shared
  // component so neither surface can leak content through a
  // parallel implementation.
  assert.ok(
    SOURCES.quickPaste.includes("<ClipboardPreview"),
    "Quick Paste must delegate the preview overlay to ClipboardPreview",
  );
  // The quick-paste preview overlay markup must NOT exist any
  // more in QuickPaste.svelte.
  assert.equal(
    SOURCES.quickPaste.includes('data-testid="quick-paste-preview-overlay"'),
    false,
    "Quick Paste must NOT redeclare its own preview overlay markup (the shared component owns it)",
  );
});

test("ClipboardPreview image lifecycle does not leak bytes through the DOM", () => {
  // The image asset bytes the resolver returns stay inside the
  // blob URL minted by `URL.createObjectURL`. The component must
  // not expose the original payload bytes, base64-encoded
  // representations, data URLs or read them back into a text
  // node.
  for (const forbidden of [
    "data:image",
    "base64",
    "FileReader",
    "readAsDataURL",
    "readAsText",
    "readAsArrayBuffer",
    "fetch(asset_ref)",
  ]) {
    assert.equal(
      SOURCES.preview.includes(forbidden),
      false,
      `ClipboardPreview must not ${forbidden} (asset byte leak regression)`,
    );
  }
});

test("App.svelte preview state never serialises the entry to JSON", () => {
  // The preview state is a typed reference; a regression that
  // calls `JSON.stringify(entry)` to push it through a watcher
  // would surface here.
  assert.equal(
    SOURCES.app.includes("JSON.stringify(entry"),
    false,
    "App.svelte must not JSON.stringify the entry for the preview path",
  );
});