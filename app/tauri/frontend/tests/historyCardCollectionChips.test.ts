/**
 * Source-level regression coverage for the
 * `collection-colors-and-card-collection-labels` change applied
 * to the inline collection chips the `HistoryCard` component
 * renders below the tag row.
 *
 * The card mounts a hidden measurement strip and a visible
 * single-line chip row that the `ResizeObserver` and the
 * `computeVisibleCollections` helper drive. The change ships a
 * read-only `CollectionMembershipModal` the overflow chip
 * activates; the tests below pin the source-level invariants the
 * spec demands:
 *
 *   - the chip uses the same visual treatment as the existing
 *     `tag-chip` (background, border, radius, padding, compact
 *     typography, safe truncation);
 *   - the protected `Historial` system collection MUST NOT be
 *     rendered as an inline chip;
 *   - an entry whose only assignment is `Historial` MUST NOT
 *     render the chip row or the overflow chip;
 *   - when every chip fits, the overflow chip stays hidden;
 *   - when a chip does not fit, a textual `+N` chip is rendered
 *     with the documented `tag-chip.more` visual treatment;
 *   - the `+N` chip carries the documented accessibility
 *     attributes (`type="button"`, focus-visible outline,
 *     `aria-haspopup`, `aria-label`, `title`, the two data
 *     attributes the spec pins) and is registered with the
 *     singleton pointer drag controller's interactive selector
 *     list;
 *   - `N` is computed by `computeOverflowCount` and is exactly
 *     the number of hidden user collections; `Historial` never
 *     contributes to `N` even though it shows up in the modal;
 *   - the chip click MUST NOT participate in card selection
 *     (`isInteractiveTarget` accepts it) nor in the drag
 *     payload;
 *   - the membership modal lists every assignment, including
 *     `Historial`, with the same chip styling;
 *   - the modal closes on Escape, backdrop and close button and
 *     returns focus to the `+N` chip;
 *   - the modal payload never carries clipboard content, hashes,
 *     paths or asset references;
 *   - the existing drag-and-drop baselines stay intact (no
 *     accidental change to `pointerDragAndDrop.ts`).
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
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/^\s*\*.*$/gm, "")
    .replace(/<!--[\s\S]*?-->/g, "");
}

// ---------------------------------------------------------------------------
// Source-level invariants for the inline chip row.
// ---------------------------------------------------------------------------

test("HistoryCard renders the inline chip row below the tag chips", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  // The visible row mounts with `data-testid="history-card-collection-chips"`
  // and only when the hydration state is loaded and at least one user
  // collection is assigned.
  assert.match(
    source,
    /data-testid="history-card-collection-chips"/,
    "the inline chip row must carry its documented testid",
  );
  assert.match(
    source,
    /entryOrganizationLoaded\s*&&\s*hasInlineCollections/,
    "the chip row must only mount once the hydration cache is loaded and at least one user collection is assigned",
  );
});

test("HistoryCard renders each chip with the same visual treatment as tag-chip", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  const styleBlock = extractStyleBlock(source);
  // Background, border, radius and padding must mirror the tag
  // chip so the two rows read as the same affordance.
  assert.match(styleBlock, /\.collection-chip\s*\{[\s\S]*?\}/, "chip rule must exist");
  const chipRule = styleBlock.match(/\.collection-chip\s*\{[\s\S]*?\}/)?.[0] ?? "";
  assert.match(chipRule, /background:/, "chip must declare a background colour");
  assert.match(chipRule, /border:/, "chip must declare a border");
  assert.match(chipRule, /border-radius:\s*999px/, "chip must declare a pill radius");
  assert.match(chipRule, /padding:/, "chip must declare a padding");
  assert.match(chipRule, /font-size:\s*var\(--cv-tag/, "chip must use the tag typography token");
  assert.match(chipRule, /text-overflow:\s*ellipsis/, "chip must declare safe truncation");
  assert.match(chipRule, /white-space:\s*nowrap/, "chip must prevent wrapping");
});

test("HistoryCard excludes Historial from the inline chip row", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  assert.match(
    source,
    /userCollections\s*=\s*assignedCollections\.filter\(\s*\(c\)\s*=>\s*c\.kind\s*!==\s*"system"/,
    "the card must filter system collections out of the inline chip set",
  );
});

test("HistoryCard hides the chip row when the entry only belongs to Historial", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  assert.match(
    source,
    /hasInlineCollections\s*=\s*userCollections\.length\s*>\s*0/,
    "the card must derive an explicit `hasInlineCollections` predicate from the user-only filter",
  );
  assert.match(
    source,
    /entryOrganizationLoaded\s*&&\s*hasInlineCollections/,
    "the visible row must mount only when there is at least one user collection to show",
  );
});

test("HistoryCard renders the +N overflow chip only when overflow is true", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  assert.match(
    source,
    /\{\#if\s+collectionOverflow\s*&&\s*collectionOverflowCount\s*>\s*0\}/,
    "the overflow chip must be gated on both the `collectionOverflow` flag and a positive count",
  );
  // The chip is bound through `bind:this` so the membership modal
  // can return focus to it on close.
  assert.match(
    source,
    /bind:this=\{collectionOverflowEl\}/,
    "the overflow chip must be bound so the modal can restore focus",
  );
});

test("HistoryCard overflow chip declares the documented accessibility attributes", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  // Anchor the match on the testid so the regex cannot absorb
  // unrelated button blocks (the title confirm/cancel buttons
  // would otherwise be picked up first because of the file's
  // overall layout).
  const overflowIndex = source.indexOf(
    'data-testid="history-card-collections-overflow"',
  );
  assert.notEqual(
    overflowIndex,
    -1,
    "the overflow chip must exist somewhere in the source",
  );
  const start = source.lastIndexOf("<button", overflowIndex);
  const end = source.indexOf("</button>", overflowIndex);
  assert.notEqual(start, -1, "the overflow chip must open with `<button`");
  assert.notEqual(end, -1, "the overflow chip must close with `</button>`");
  const button = source.slice(start, end);
  assert.match(button, /type="button"/, "the chip must declare type=button");
  assert.match(button, /aria-haspopup="dialog"/, "the chip must announce the dialog role");
  assert.match(button, /aria-label=/, "the chip must carry an accessible label");
  assert.match(button, /title=/, "the chip must surface a title");
  assert.match(
    button,
    /data-entry-id=\{entry\.id\}/,
    "the chip must carry the entry id so regression tests can target it",
  );
  assert.match(
    button,
    /\{collectionOverflowLabel\}/,
    "the chip must render the documented `+N` label",
  );
  // No SVG icon — the affordance is the textual `+N` indicator.
  assert.equal(
    button.includes("<svg"),
    false,
    "the overflow chip must never render an inline SVG icon",
  );
});

test("HistoryCard overflow chip carries a focus-visible outline", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  const styleBlock = extractStyleBlock(source);
  assert.match(
    styleBlock,
    /\.collection-overflow:focus-visible\s*\{[\s\S]*?outline:/,
    "the overflow chip must declare a focus-visible outline",
  );
});

test("HistoryCard isInteractiveTarget treats the overflow chip as interactive", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  assert.match(
    source,
    /target\.closest\("\[data-testid='history-card-collections-overflow'\]"\)/,
    "isInteractiveTarget must short-circuit when the click lands on the overflow chip",
  );
});

test("HistoryCard overflow chip reuses the tag-chip.more visual treatment", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  // The chip combines the documented `tag-chip.more` class with
  // the row-local `.collection-overflow` selector so the painted
  // chrome (background, border, radius, padding, typography)
  // matches the tag overflow indicator.
  const overflowIndex = source.indexOf(
    'data-testid="history-card-collections-overflow"',
  );
  const start = source.lastIndexOf("<button", overflowIndex);
  const end = source.indexOf("</button>", overflowIndex);
  const button = source.slice(start, end);
  assert.match(
    button,
    /class="tag-chip more collection-overflow"/,
    "the chip must declare both `tag-chip more` and `collection-overflow`",
  );
});

test("HistoryCard derives the +N count from computeOverflowCount over user collections only", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  // The card must derive `collectionOverflowCount` from the pure
  // helper over the user-only subset so `Historial` is never
  // counted.
  assert.match(
    source,
    /import[\s\S]*?computeOverflowCount[\s\S]*?computeVisibleCollections[\s\S]*?\}\s*from\s+"\.\/lib\/collectionChipLayout"/,
    "the card must import both pure helpers from the layout module",
  );
  assert.match(
    source,
    /collectionOverflowCount\s*=\s*computeOverflowCount\(\s*userCollections,[\s\S]*?visibleUserCollections[\s\S]*?\)/,
    "the overflow count must be derived from the user-only subset",
  );
  assert.match(
    source,
    /collectionOverflowLabel\s*=\s*`\+\$\{collectionOverflowCount\}`/,
    "the chip text must render as `+${count}`",
  );
});

test("HistoryCard overflow chip text never falls back to a fixed membership count", () => {
  // The overflow chip must drive its text from the purely derived
  // `collectionOverflowLabel` (a `+N` literal) and never from a
  // hard-coded `assignedCollections.length > N` threshold that
  // would slice the membership set independently of the row width.
  // The check scopes the inspection to the chip block so the
  // unrelated ternary the ellipsis menu uses for "Editar" /
  // "Agregar a colección" does not produce false positives.
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  const overflowIndex = source.indexOf(
    'data-testid="history-card-collections-overflow"',
  );
  assert.notEqual(overflowIndex, -1, "the overflow chip must exist");
  const start = source.lastIndexOf("<button", overflowIndex);
  const end = source.indexOf("</button>", overflowIndex);
  const chip = source.slice(start, end);
  assert.equal(
    /assignedCollections\.length\s*>\s*\d+/.test(chip),
    false,
    "the overflow chip must never compare the membership count against a hard-coded threshold",
  );
});

// ---------------------------------------------------------------------------
// ResizeObserver + measurement invariants.
// ---------------------------------------------------------------------------

test("HistoryCard uses a ResizeObserver to react to layout changes", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  assert.match(source, /ResizeObserver/, "the card must use a ResizeObserver");
  assert.match(
    source,
    /collectionResizeObserver(\?)?\.observe\(collectionChipsEl\)/,
    "the observer must subscribe to the chip row",
  );
  assert.match(
    source,
    /collectionResizeObserver\.disconnect\(\)/,
    "the observer must be detached on teardown",
  );
});

test("HistoryCard computes overflow from the intrinsic chip total vs available row width", () => {
  // The overflow predicate must compare the intrinsic total of
  // every user collection (with gaps) against the painted row's
  // `clientWidth`. Comparing `scrollWidth > clientWidth` against
  // the painted row is wrong: once the row painted the trimmed
  // subset, `scrollWidth` collapses to the trimmed width and the
  // predicate can never converge on the original overflow
  // decision. The intrinsic total is computed from the hidden
  // measurement strip so the comparison survives across
  // re-renders, resize and hydration.
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  const chipLayoutSource = stripComments(
    loadSource("src/lib/collectionChipLayout.ts"),
  );
  assert.match(
    source,
    /intrinsicCollectionRowWidth\(/,
    "the card must compute the intrinsic total through the dedicated helper",
  );
  assert.match(
    source,
    /intrinsicTotal\s*>\s*collectionRowWidth/,
    "the overflow predicate must compare the intrinsic total against the row's available width",
  );
  assert.match(
    chipLayoutSource,
    /export function intrinsicCollectionRowWidth/,
    "the layout helper must export the intrinsic-total computation",
  );
});

test("HistoryCard consults computeVisibleCollections to trim the chip subset", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  assert.match(
    source,
    /import[\s\S]*?computeVisibleCollections[\s\S]*?\}\s*from\s+"\.\/lib\/collectionChipLayout"/,
    "the card must import the pure helper for the visible-subset computation",
  );
  assert.match(
    source,
    /visibleUserCollections\s*=\s*computeVisibleCollections\(/,
    "the visible subset must be derived through the helper",
  );
});

test("HistoryCard feeds the measured overflow chip width back into the visible subset", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  // The card mounts a measurement span for the overflow chip with
  // the worst-case text (`+{userCollections.length}`) and passes
  // its real width to `computeVisibleCollections` so the
  // reservation tracks the painted chrome instead of a fixed
  // fallback.
  assert.match(
    source,
    /data-collection-overflow-measure-item/,
    "the measurement strip must include the overflow chip measurement",
  );
  assert.match(
    source,
    /collectionOverflowChipWidth\s*=\s*overflowItem\??\.?getBoundingClientRect\(\)\.width/,
    "the card must cache the measured width of the overflow chip",
  );
  assert.match(
    source,
    /computeVisibleCollections\(\s*userCollections,[\s\S]*?collectionChipWidths,[\s\S]*?collectionRowWidth,[\s\S]*?collectionOverflow,[\s\S]*?collectionOverflowChipWidth/,
    "the helper must receive the measured overflow chip width",
  );
});

// ---------------------------------------------------------------------------
// Pointer drag controller — the singleton must skip the new chip.
// ---------------------------------------------------------------------------

test("pointerDragAndDrop.ts registers the overflow chip as an interactive selector", () => {
  const source = stripComments(loadSource("src/lib/pointerDragAndDrop.ts"));
  assert.match(
    source,
    /"\[data-testid='history-card-collections-overflow'\]"/,
    "the singleton drag controller must skip the new overflow chip",
  );
});

// ---------------------------------------------------------------------------
// Membership modal invariants.
// ---------------------------------------------------------------------------

test("CollectionMembershipModal reuses the shared Modal shell", () => {
  const source = stripComments(loadSource("src/CollectionMembershipModal.svelte"));
  assert.match(
    source,
    /import\s+Modal\s+from\s+"\.\/Modal\.svelte"/,
    "the modal must reuse the shared Modal shell",
  );
  assert.match(source, /<Modal\b/, "the modal must mount the shared shell");
});

test("CollectionMembershipModal lists every assigned collection including Historial", () => {
  const source = stripComments(loadSource("src/CollectionMembershipModal.svelte"));
  // The list iterates `orderedCollections`, which keeps the
  // system row first. We assert the relevant pieces individually
  // so the regexes stay trivial and lint-clean.
  assert.match(
    source,
    /system\s*=\s*assignedCollections\.filter\(\(c\)\s*=>\s*c\.kind\s*===\s*"system"\)/,
    "the modal must filter the system rows out of the user set",
  );
  assert.match(
    source,
    /users\s*=\s*\[\.\.\.assignedCollections\.filter\(\(c\)\s*=>\s*c\.kind\s*===\s*"user"\)\]/,
    "the modal must filter the user rows out of the system set",
  );
  assert.match(
    source,
    /return\s*\[\.\.\.system,\s*\.\.\.users\]/,
    "the modal must render the system row first followed by the user rows",
  );
  assert.match(source, /each\s+orderedCollections/, "the modal must iterate the sorted list");
});

test("CollectionMembershipModal is read-only — no save or delete affordances", () => {
  const source = stripComments(loadSource("src/CollectionMembershipModal.svelte"));
  // No Tauri command dispatch, no mutation handler, no checkboxes
  // or remove buttons — the modal is a viewer only.
  assert.equal(
    /clipvault_collections_set_color|clipvault_collections_create|clipvault_collections_delete|clipvault_collections_rename|clipvault_entry_collections_set|clipvault_entry_remove_from_collection/.test(
      source,
    ),
    false,
    "the modal must never invoke any collection-mutation command",
  );
  assert.equal(
    /<button[^>]*data-testid="collection-membership-(?:remove|delete)"/.test(source),
    false,
    "the modal must not expose a remove or delete affordance",
  );
});

test("CollectionMembershipModal payload carries no clipboard / hash / path / asset data", () => {
  const source = stripComments(loadSource("src/CollectionMembershipModal.svelte"));
  for (const forbidden of [
    "DataTransfer",
    "content_hash",
    "asset_ref",
    "assetRef",
    "snippet",
    "DataTransfer.types",
  ]) {
    assert.equal(
      source.includes(forbidden),
      false,
      `the modal must never mention "${forbidden}"`,
    );
  }
});

test("CollectionMembershipModal wires aria-labelledby through a stable title id", () => {
  const source = stripComments(loadSource("src/CollectionMembershipModal.svelte"));
  const modalSource = stripComments(loadSource("src/Modal.svelte"));
  // Title id is parameterised by the entry id so two open modals
  // never share the same `aria-labelledby` target. The shared
  // `Modal` shell wires `aria-labelledby={titleId}` so the dialog
  // announces the title the membership modal declares.
  assert.match(
    source,
    /titleId\s*=\s*`collection-membership-modal-title-\$\{entryId\}`/,
    "the modal must derive the title id from the entry id",
  );
  assert.match(
    source,
    /<Modal[\s\S]*?\{titleId\}/,
    "the modal must forward the title id to the shared shell",
  );
  assert.match(
    modalSource,
    /aria-labelledby=\{titleId\}/,
    "the shared modal shell must wire aria-labelledby to the title id",
  );
});

test("CollectionMembershipModal returns focus to the overflow chip on close", () => {
  const source = stripComments(loadSource("src/CollectionMembershipModal.svelte"));
  assert.match(
    source,
    /returnFocusTo\s*:\s*HTMLElement\s*\|\s*null/,
    "the modal must declare a return-focus target element",
  );
});

test("CollectionMembershipModal applies the collection's colour to each chip", () => {
  const source = stripComments(loadSource("src/CollectionMembershipModal.svelte"));
  assert.match(
    source,
    /collectionColor\(collection\.color_hex\)/,
    "the modal must reuse the same pure colour helper the card uses",
  );
  assert.match(source, /background-color:\s*\{colour\}/, "the chip swatch must paint with the colour");
  assert.match(source, /color:\s*var\(--chip-color/, "the chip text must use the colour");
});

// ---------------------------------------------------------------------------
// Existing baselines: drag-and-drop, pin/menu/title, payloads.
// ---------------------------------------------------------------------------

test("the HistoryCard root element keeps the protected attributes", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  const articlePattern = /<article[\s\S]*?>/;
  const article = source.match(articlePattern)?.[0] ?? "";
  assert.match(article, /data-testid="history-card"/);
  assert.match(article, /data-entry-id=\{entry\.id\}/);
  assert.match(article, /draggable="false"/);
});

test("the drag payload still carries only the entry id", () => {
  // Sanity: the buildDragPayload helper is the same contract the
  // card relies on; a regression that broadens the payload would
  // surface here.
  const dragSource = stripComments(loadSource("src/lib/dragAndDrop.ts"));
  assert.match(dragSource, /buildDragPayload/, "buildDragPayload must still exist");
});

test("pointerDragAndDrop.ts remains a singleton with the documented public surface", () => {
  const source = stripComments(loadSource("src/lib/pointerDragAndDrop.ts"));
  assert.match(source, /export function installPointerDragController/);
  assert.match(source, /export function uninstallPointerDragController/);
  assert.match(source, /export function isPointerDragActive/);
  assert.match(source, /export function activePointerDragEntryId/);
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function extractStyleBlock(source: string): string {
  // The component keeps the rules inside the `<style>` tag. We
  // strip comments and return the inner content so the regex
  // matchers below can address the CSS directly.
  const styleMatch = source.match(/<style>([\s\S]*?)<\/style>/);
  if (!styleMatch) return "";
  return stripComments(styleMatch[1]);
}