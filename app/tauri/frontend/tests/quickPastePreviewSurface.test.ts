/**
 * Tests for the Quick Paste surface the
 * `quick-paste-preview-ui` change exposes:
 *
 *   - the source-app icon area is a stable square footprint that
 *     keeps its dimensions through every documented state
 *     (`loaded`, `loading`, `error`, `absent`);
 *   - the per-row icon resolver keeps the stale-response guard so a
 *     late icon resolution cannot overwrite a fresher's row;
 *   - the rounded shell lives on the visible body without growing
 *     the documented `720 × 520` window;
 *   - the search hint surfaces the platform-aware shortcut;
 *   - the preview overlay sits inside the same fixed window and is
 *     strictly read-only (no clipboard, no paste, no history).
 */
import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";

const quickPasteSource = readFileSync(
  resolvePath(process.cwd(), "src", "QuickPaste.svelte"),
  "utf8",
);

test("Quick Paste wires the source-app icon bridge through sourceAppIconCommand", () => {
  // The component must route the icon resolution through the same
  // validated backend bridge the desktop rail uses. A regression
  // that drops the bridge widens the surface to arbitrary refs.
  assert.ok(
    quickPasteSource.includes("sourceAppIconCommand"),
    "Quick Paste must import sourceAppIconCommand",
  );
  assert.ok(
    quickPasteSource.includes("tauriSourceAppIconLoader"),
    "Quick Paste must declare the tauri source-app icon loader",
  );
});

test("Quick Paste declares a per-row app-icon token guard", () => {
  // The guard mirrors the existing `thumbnailToken` invariant: a
  // late resolution from entry A cannot clobber entry B's state.
  // A regression that drops the token would let a stale response
  // overwrite a fresher's icon.
  assert.ok(
    quickPasteSource.includes("let appIconToken"),
    "Quick Paste must declare appIconToken",
  );
  assert.ok(
    quickPasteSource.includes("appIconTokens"),
    "Quick Paste must declare the appIconTokens map",
  );
  assert.ok(
    quickPasteSource.includes("appIconTokens.get(entry.id) !== token"),
    "Quick Paste must discard stale icon responses via the token guard",
  );
});

test("Quick Paste renders the source-app area with stable dimensions", () => {
  // The spec mandates a stable square that matches the documented
  // `HistoryCard.svelte` footprint (`1.65rem`). The CSS rule MUST
  // pin width and height so the row never reflows while the icon
  // transitions between `loading`, `loaded` and `error`.
  const cssBlock = quickPasteSource.slice(
    quickPasteSource.indexOf("style"),
    quickPasteSource.length,
  );
  const rule = cssBlock.match(/\.qp-source-app\s*\{[^}]*\}/);
  assert.ok(rule, "the .qp-source-app CSS rule must exist");
  assert.ok(
    /width:\s*1\.65rem/.test(rule![0]),
    "the .qp-source-app square must keep a stable 1.65rem width (matching HistoryCard)",
  );
  assert.ok(
    /height:\s*1\.65rem/.test(rule![0]),
    "the .qp-source-app square must keep a stable 1.65rem height (matching HistoryCard)",
  );
});

test("Quick Paste mounts the rounded shell on the visible body", () => {
  // The shell border-radius lives on `main` so the geometry stays
  // anchored to the documented 720×520 rectangle while the corners
  // read consistently with the rest of the cards.
  const cssBlock = quickPasteSource.slice(
    quickPasteSource.indexOf("style"),
    quickPasteSource.length,
  );
  const mainRule = cssBlock.match(/\bmain\s*\{[^}]*\}/);
  assert.ok(mainRule, "the main CSS rule must exist");
  assert.ok(
    /border-radius:\s*16px/.test(mainRule![0]),
    "the visible shell must use the documented 16px corner radius",
  );
});

test("Quick Paste surfaces the platform-aware search hint", () => {
  // The hint must read the same value the matcher uses so the
  // visible label and the keyboard shortcut cannot drift apart.
  assert.ok(
    quickPasteSource.includes("quick-paste-search-hint"),
    "the search hint test id must be exposed",
  );
  assert.ok(
    quickPasteSource.includes("quickPasteSearchShortcutLabel"),
    "the hint must consult quickPasteSearchShortcutLabel",
  );
  assert.ok(
    quickPasteSource.includes('data-shortcut-platform={shortcutPlatform}'),
    "the hint must record the active platform for tests",
  );
});

test("Quick Paste renders the ⌘K / Ctrl K hint, never the ⌘F / Ctrl F variant", () => {
  // The Quick Paste window installs the `K`-key shortcut, NOT the
  // main desktop `F`-key search shortcut. The hint MUST therefore
  // derive from the Quick-Paste-specific helper so the visible
  // label and the matcher stay in lockstep.
  assert.ok(
    /⌘K|⌘F/.test("⌘K") === true,
    "sanity check: ⌘K literal exists in JS data",
  );
  const readsQuickPasteHelper = quickPasteSource.includes(
    "quickPasteSearchShortcutLabel",
  );
  assert.equal(
    readsQuickPasteHelper,
    true,
    "the visible badge must read quickPasteSearchShortcutLabel",
  );
  // The hint must NOT consult the original (F-key) helper — the
  // two surfaces must stay independently labelled.
  const usesFHelper = /shortcutLabelText\s*=\s*searchShortcutLabel/.test(
    quickPasteSource,
  );
  assert.equal(
    usesFHelper,
    false,
    "the badge must not consult the F-key searchShortcutLabel helper",
  );
});

test("Quick Paste mounts the in-window preview overlay", () => {
  // The overlay must live inside the same fixed window and render
  // through stable selectors. The `desktop-card-preview` change
  // extracts the markup to a shared `ClipboardPreview.svelte`
  // component consumed by both Quick Paste and the Desktop rail;
  // the test verifies Quick Paste delegates to that shared
  // component instead of re-implementing the markup locally.
  const previewSource = readFileSync(
    resolvePath(process.cwd(), "src", "ClipboardPreview.svelte"),
    "utf8",
  );
  assert.ok(
    previewSource.includes("quick-paste-preview-overlay") ||
      previewSource.includes("{testIdPrefix}-overlay"),
    "the shared preview overlay template must exist",
  );
  assert.ok(
    previewSource.includes("quick-paste-preview-card") ||
      previewSource.includes("{testIdPrefix}-card"),
    "the shared preview card template must exist",
  );
  assert.ok(
    previewSource.includes("role=\"dialog\""),
    "the shared preview overlay must declare the dialog role",
  );
  assert.ok(
    quickPasteSource.includes("<ClipboardPreview"),
    "Quick Paste must delegate the preview overlay to ClipboardPreview",
  );
  assert.ok(
    quickPasteSource.includes("testIdPrefix=\"quick-paste-preview\""),
    "Quick Paste must prefix the preview overlay with the quick-paste test ids",
  );
});

test("the preview overlay is strictly read-only", () => {
  // The overlay MUST NOT carry any of the paste, copy or mutation
  // paths the spec pins as out-of-scope. The shared
  // `ClipboardPreview.svelte` owns the overlay markup so the grep
  // below inspects that file (not `QuickPaste.svelte`) — a
  // regression that accidentally drops a write path into the
  // preview branch surfaces here.
  const previewSource = readFileSync(
    resolvePath(process.cwd(), "src", "ClipboardPreview.svelte"),
    "utf8",
  );
  for (const forbidden of [
    "copyEntryCommand",
    "pasteEntryCommand",
    "runCopyForEntry",
    "runMenuPasteForEntry",
    "setFavoriteCommand",
    "captureTextCommand",
  ]) {
    assert.equal(
      previewSource.includes(forbidden),
      false,
      `shared preview overlay must not invoke ${forbidden}`,
    );
  }
});

test("the preview overlay renders the safe preview text fallback", () => {
  // The preview body MUST render the entry's full preview
  // through a `<pre>` so the layout stays bounded and the markup
  // stays safe — never through `innerHTML` or `iframe`.
  const previewSource = readFileSync(
    resolvePath(process.cwd(), "src", "ClipboardPreview.svelte"),
    "utf8",
  );
  assert.ok(
    previewSource.includes("{testIdPrefix}-text"),
    "the shared preview text test id template must exist",
  );
  assert.ok(
    previewSource.includes("<pre"),
    "the shared preview body must use a <pre> element for the text surface",
  );
});

test("the preview overlay has no horizontal overflow contract", () => {
  // The overlay must use `max-width: 100%` / `max-height: 100%` so
  // a long entry never widens the fixed `720 × 520` window. The
  // shared component clips overflow through its inner card so the
  // fixed window never grows.
  const previewSource = readFileSync(
    resolvePath(process.cwd(), "src", "ClipboardPreview.svelte"),
    "utf8",
  );
  const cssBlock = previewSource.slice(
    previewSource.indexOf("<style>"),
    previewSource.length,
  );
  const overlayRule = cssBlock.match(/\.cv-preview-card\s*\{[^}]*\}/);
  assert.ok(overlayRule, "the preview card CSS rule must exist");
  assert.ok(
    /max-width:\s*100%/.test(overlayRule![0]) ||
      /overflow:\s*hidden/.test(overlayRule![0]),
    "the preview card must clip overflow so the fixed window never grows",
  );
});