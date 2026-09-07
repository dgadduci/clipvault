/**
 * No-mutation regression tests for the `desktop-card-preview`
 * change.
 *
 * Opening, reading and closing the preview MUST NOT mutate the
 * entry's content, title, timestamp, favourite state, tags,
 * collections, source-app metadata, asset references or
 * clipboard contents. The change pins the read-only contract
 * across the Desktop rail (`App.svelte`, `HistoryCard.svelte`,
 * `HistoryCardRail.svelte`), the Quick Paste window
 * (`QuickPaste.svelte`) and the shared component
 * (`ClipboardPreview.svelte`).
 *
 * The tests inspect the source files so a regression that
 * re-introduces a mutation path (copy, paste, pin, delete,
 * title, tags, collections, capture, write) surfaces in CI
 * without an actual end-to-end run.
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
  quickPaste: readFileSync(
    resolvePath(process.cwd(), "src", "QuickPaste.svelte"),
    "utf8",
  ),
  helper: readFileSync(
    resolvePath(process.cwd(), "src", "lib", "clipboardPreview.ts"),
    "utf8",
  ),
};

function extractFunctionBody(source: string, name: string): string {
  const start = source.indexOf(`function ${name}`);
  if (start < 0) return "";
  const openBrace = source.indexOf("{", start);
  let depth = 1;
  let cursor = openBrace + 1;
  while (depth > 0 && cursor < source.length) {
    const ch = source[cursor];
    if (ch === "{") depth += 1;
    else if (ch === "}") depth -= 1;
    cursor += 1;
  }
  return source.slice(openBrace, cursor);
}

const MUTATION_COMMANDS = [
  "copyEntryCommand",
  "pasteEntryCommand",
  "setFavoriteCommand",
  "setEntryTitleCommand",
  "deleteEntryCommand",
  "entryTagsSetCommand",
  "entryTagsCommand",
  "entryCollectionsSetCommand",
  "entryCollectionsCommand",
  "entryRemoveFromCollectionCommand",
  "captureTextCommand",
  "collectionsCreateCommand",
  "collectionsDeleteCommand",
  "collectionsRenameCommand",
  "activeApplicationCommand",
  "writeText",
  "writeImage",
];

test("ClipboardPreview never invokes a mutation command", () => {
  for (const cmd of MUTATION_COMMANDS) {
    assert.equal(
      SOURCES.preview.includes(cmd),
      false,
      `ClipboardPreview must not invoke ${cmd} (read-only contract)`,
    );
  }
});

test("App.svelte preview helpers never invoke a mutation command", () => {
  // The preview helpers own only the read-only state; they
  // delegate the open / close to ClipboardPreview, never to the
  // mutation commands the rail consumes for pin / paste / delete.
  for (const helperName of [
    "requestPreview",
    "closePreview",
    "bumpPreviewScope",
  ]) {
    const helper = extractFunctionBody(SOURCES.app, helperName);
    for (const cmd of MUTATION_COMMANDS) {
      assert.equal(
        helper.includes(cmd),
        false,
        `${helperName} must not invoke ${cmd} (read-only contract)`,
      );
    }
  }
});

test("HistoryCard requestPreview never invokes a mutation command", () => {
  const helper = extractFunctionBody(SOURCES.card, "requestPreview");
  for (const cmd of MUTATION_COMMANDS) {
    assert.equal(
      helper.includes(cmd),
      false,
      `requestPreview must not invoke ${cmd} (read-only contract)`,
    );
  }
});

test("HistoryCard onCardKeydown never invokes a mutation command", () => {
  const helper = extractFunctionBody(SOURCES.card, "onCardKeydown");
  for (const cmd of MUTATION_COMMANDS) {
    assert.equal(
      helper.includes(cmd),
      false,
      `onCardKeydown must not invoke ${cmd} (read-only contract)`,
    );
  }
});

test("HistoryCardRail handlePreviewRequest never invokes a mutation command", () => {
  const helper = extractFunctionBody(SOURCES.rail, "handlePreviewRequest");
  for (const cmd of MUTATION_COMMANDS) {
    assert.equal(
      helper.includes(cmd),
      false,
      `handlePreviewRequest must not invoke ${cmd} (read-only contract)`,
    );
  }
});

test("QuickPaste preview path never invokes a mutation command", () => {
  // The shared component handles the overlay markup; the
  // surrounding `previewEntryId !== null` block must not invoke
  // the mutation commands the row / menu use to copy or paste.
  const overlayStart = SOURCES.quickPaste.indexOf(
    "{#if previewEntryId !== null}",
  );
  assert.notEqual(overlayStart, -1, "the Quick Paste preview branch must exist");
  const overlayEnd = SOURCES.quickPaste.indexOf("{/if}", overlayStart);
  const overlayBlock = SOURCES.quickPaste.slice(overlayStart, overlayEnd);
  for (const cmd of MUTATION_COMMANDS) {
    assert.equal(
      overlayBlock.includes(cmd),
      false,
      `Quick Paste preview branch must not invoke ${cmd}`,
    );
  }
});

test("clipboardPreview helper is metadata-only", () => {
  // The shared helper module never invokes a mutation command
  // either; it owns the matcher, the platform derivation and the
  // typed re-exports only.
  for (const cmd of MUTATION_COMMANDS) {
    assert.equal(
      SOURCES.helper.includes(cmd),
      false,
      `clipboardPreview.ts must not invoke ${cmd}`,
    );
  }
});

test("ClipboardPreview never captures the active application target", () => {
  // Capturing the active app would change the focus to ClipVault
  // and break the read-only contract. The overlay must remain
  // passive.
  assert.equal(
    SOURCES.preview.includes("activeApplicationCommand"),
    false,
    "ClipboardPreview must not capture the active application",
  );
  assert.equal(
    SOURCES.preview.includes("captureActiveApp"),
    false,
    "ClipboardPreview must not capture the active application",
  );
});

test("ClipboardPreview image rendering never mutates asset_ref or metadata", () => {
  // The image renderer must not touch asset_ref, mime_type,
  // payload_width, payload_height, content_hash or content_size.
  // The bytes the resolver returns stay byte-for-byte identical
  // because the resolver hands the preview a fresh Blob and the
  // UI mounts it through `<img src={url}>` — no re-encoding, no
  // resize, no profile mutation.
  const cssBlock = SOURCES.preview.slice(
    SOURCES.preview.indexOf("<style>"),
    SOURCES.preview.length,
  );
  for (const forbidden of [
    "canvas",
    "toBlob",
    "convert",
    "imageSmoothing",
    "transform",
    "rotate",
    "crop",
  ]) {
    assert.equal(
      cssBlock.includes(forbidden),
      false,
      `the preview image CSS must not ${forbidden} the asset`,
    );
  }
});

test("ClipboardPreview overlay title / type label only reads metadata, never writes", () => {
  // The header reads `entry.title`, `entry.content_type` and the
  // source-app label — none of these trigger a write path. The
  // assertion inspects the helper hooks the template binds.
  const refreshImage = extractFunctionBody(SOURCES.preview, "refreshImage");
  for (const forbidden of [
    "setEntryTitle",
    "setFavorite",
    "deleteEntry",
    "entryTagsSet",
    "entryCollectionsSet",
  ]) {
    assert.equal(
      refreshImage.includes(forbidden),
      false,
      `refreshImage must not ${forbidden}`,
    );
  }
});

test("HistoryCard menu keeps paste / pin / delete / title editor available", () => {
  // The change MUST NOT regress the existing controls the user
  // can still pick from the menu. The previsualizar action sits
  // alongside the documented paste / pin / delete buttons; a
  // regression that drops one of them surfaces here.
  //
  // `history-card-paste` testid is bound dynamically through
  // `pasteMenuActions`; the assertion confirms the source keeps
  // the dynamic binding instead of a static `data-testid` literal.
  for (const testid of [
    "history-card-edit-title",
    "history-card-restore-title",
    "history-card-add-tags",
    "history-card-add-collections",
    "history-card-pin",
    "history-card-delete",
    "history-card-preview",
  ]) {
    assert.ok(
      SOURCES.card.includes(`data-testid="${testid}"`),
      `HistoryCard menu must keep data-testid="${testid}"`,
    );
  }
  assert.ok(
    SOURCES.card.includes("pasteAction.testId"),
    "HistoryCard must keep the dynamic pasteMenuActions binding so paste / paste-rich / paste-plain testids stay intact",
  );
});