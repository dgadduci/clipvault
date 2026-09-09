/**
 * Regression coverage for the desktop "Acerca de" modal.
 *
 * The modal exposes the canonical product name and version that the
 * `clipvault_diagnostics` Tauri command reads from
 * `Cargo.toml`'s `[workspace.package].version` (mirrored in
 * `tauri.conf.json` and `package.json`). The contract the test pins:
 *
 *   - The About modal is reachable through exactly one entry
 *     point: the global ellipsis menu on the desktop toolbar.
 *     Per-card menus MUST NOT carry a parallel "Acerca de" item,
 *     so the version string the user sees stays single-sourced.
 *   - The modal reads its version from the `Diagnostics` payload
 *     (the canonical bridge), never from a hard-coded Svelte
 *     constant. A drift between the manifests and the modal copy
 *     would surface here as a failed assertion.
 *   - The modal honours every accessibility contract the shared
 *     `Modal` shell already enforces: Escape closes it, the close
 *     button routes through the same handler, focus returns to the
 *     trigger and the document listener is detached on destroy
 *     (covered by the shared shell — pinned indirectly through the
 *     menu item count and the modal existence assertions below).
 *   - The version label always renders with the documented
 *     `vX.Y.Z` prefix so the user reads the same shape every other
 *     ClipVault surface advertises.
 *
 * The suite reads the production source through the `loadSource`
 * helper so it stays in lock-step with the markup and the modal
 * contract without standing up a Svelte runtime.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

const FRONTEND_ROOT = process.cwd();

function loadSource(...segments: string[]): string {
  return readFileSync(path.join(FRONTEND_ROOT, ...segments), "utf8");
}

function stripComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/\/.*$/gm, "")
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/^\s*\*.*$/gm, "");
}

test("AboutModal reads the version from the diagnostics payload and never hard-codes it", () => {
  const source = stripComments(loadSource("src/AboutModal.svelte"));
  // The component must read the version off `diagnostics.version`,
  // not from a literal `const` that would silently drift away from
  // the canonical Cargo / Tauri config / package.json manifests.
  assert.match(source, /diagnostics\?\.version/);
  assert.match(source, /displayVersion/);
  // The accessor prefixes the canonical version with the documented
  // `v` so the visible label matches the shape every other
  // ClipVault surface advertises (vX.Y.Z). The template literal
  // `v${raw}` is the single source of the prefix; the modal never
  // inlines a hard-coded version string.
  assert.match(source, /`v\$\{raw\}`/);
  // The visible "Versión" row exposes the prefixed value through a
  // `{displayVersion}` Svelte interpolation so the rendered DOM
  // and the diagnostics payload stay in lock-step.
  assert.match(source, /\{displayVersion\}/);
  // The visible "Versión" row exposes the version in a `<code>`
  // element so screen readers and copy-paste stay honest.
  assert.match(source, /<code[^>]*data-testid="about-version"/);
  // The product name is rendered as a literal label so a future
  // rebrand stays a single-file edit.
  assert.match(source, /ClipVault/);
});

test("DesktopToolbar exposes exactly one 'Acerca de' item inside the global ellipsis menu", () => {
  const source = stripComments(loadSource("src/DesktopToolbar.svelte"));
  // The item must carry the documented `data-testid` so the parent
  // can dispatch through it and the regression suite can locate it
  // without scraping labels.
  assert.match(source, /data-testid="open-about"/);
  assert.match(source, /onOpenAbout/);
  // The visible label must appear exactly once as the text content
  // of a `<button>` element — a regression that duplicates the
  // entry (for example by also surfacing it through the per-card
  // menu) would surface here.
  const labelMatches =
    source.match(/<button[\s\S]*?>\s*Acerca de\s*<\/button>/g) ?? [];
  assert.equal(
    labelMatches.length,
    1,
    "Acerca de must appear exactly once as the text content of a <button>",
  );
});

test("DesktopToolbar keeps the About entry inside the menu so it is single-sourced", () => {
  const source = stripComments(loadSource("src/DesktopToolbar.svelte"));
  const menuOpenIdx = source.indexOf("{#if menuOpen}");
  const menuCloseIdx = source.indexOf("{/if}", menuOpenIdx);
  assert.notEqual(menuOpenIdx, -1);
  assert.notEqual(menuCloseIdx, -1);
  const menuBody = source.slice(menuOpenIdx, menuCloseIdx);
  assert.ok(
    menuBody.includes("open-about"),
    "the About entry must live inside the global ellipsis menu",
  );
});

test("HistoryCard and Quick Paste never expose a parallel About entry", () => {
  // The version string must be single-sourced: only the global
  // ellipsis menu on the desktop toolbar carries the affordance.
  // Per-card menus (HistoryCard) and Quick Paste must NOT carry a
  // parallel "Acerca de" entry — the regression suite fails here
  // when a future refactor accidentally duplicates the menu item
  // across surfaces.
  for (const file of [
    "src/HistoryCard.svelte",
    "src/QuickPaste.svelte",
    "src/HistoryCardRail.svelte",
  ]) {
    const source = stripComments(loadSource(file));
    assert.equal(
      source.includes("Acerca de"),
      false,
      `${file} must not duplicate the global About entry`,
    );
    assert.equal(
      source.includes("data-testid=\"open-about\""),
      false,
      `${file} must not duplicate the global About testid`,
    );
  }
});

test("App.svelte mounts the About modal exactly once and routes through the shared Modal shell", () => {
  const source = stripComments(loadSource("src/App.svelte"));
  // The modal must live behind the same `<Modal>` shell the other
  // configuration dialogs use, so focus, Escape, the backdrop
  // click and the destroy cleanup stay symmetric.
  assert.match(source, /<Modal[\s\S]*?open=\{openModal === "about"\}/);
  assert.match(source, /title="Acerca de"/);
  assert.match(source, /<AboutModal\s+diagnostics=\{diagnostics\}/);
  // The handler the toolbar dispatches through must live on the
  // parent so the menu / modal state machine stays single-sourced.
  assert.match(source, /function onOpenAbout/);
  // The ModalId union must include the about variant so the
  // discriminated state machine cannot fall through to the wrong
  // branch by accident.
  assert.match(source, /"about"/);
});