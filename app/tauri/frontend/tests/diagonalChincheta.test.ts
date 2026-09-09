/**
 * Regression coverage for the diagonal chincheta / pushpin
 * correction folded into `clipboard-legacy-image-assets`.
 *
 * The previous revision used a circle + line + triangle
 * composition that the user read as a magnifying glass / key. The
 * current revision traces the whole silhouette as a single
 * `<path>` inside a `<g transform="rotate(45 12 12)">` group, so
 * the wide rounded head, the short body and the pointed tip form
 * one continuous thumbtack glyph rather than three separate
 * primitives that read as a handle + lens.
 *
 * The contract this file pins for the diagonal glyph:
 *
 *   - Both states render the same tilted silhouette inside a
 *     consistent `viewBox="0 0 24 24"` viewport so the icon stays
 *     centred on the dark button background.
 *   - The icon is drawn as a single `<path>` element wrapped in a
 *     `<g transform="rotate(45 12 12)">` group, NOT as a circle
 *     + line + polygon composition (which looked like a key /
 *     magnifying glass and is what the user rejected).
 *   - After the rotation, the head sits in the upper-right
 *     quadrant of the viewport and the tip points to the
 *     lower-left at roughly 45°, matching the reference visual.
 *   - The unpinned state uses an outlined silhouette
 *     (`fill="none"` + `stroke="currentColor"`) in a gray/lavender
 *     tone so the affordance stays visible without competing with
 *     the filled variant.
 *   - The pinned state uses a fully filled silhouette
 *     (`fill="currentColor"`) in the documented yellow tone
 *     (`#F5C542`) so the chincheta reads as a classic push pin
 *     against the dark button surface.
 *   - The button background stays dark in both states so the
 *     yellow fill stays legible.
 *   - `aria-pressed`, `aria-label`, `title`, `focus-visible`, the
 *     `data-testid` hooks (`history-card-pin`, `history-card-pin
 *     -filled`, `history-card-pin-outline`), the `data-pinned`
 *     attribute and the `handlePinClick` wiring all survive the
 *     SVG redesign.
 *   - No star, heart, bookmark, emoji or textual fallback is
 *     rendered for the favorite affordance.
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
    .replace(/^\s*\*.*$/gm, "");
}

/**
 * Match the pin SVG by anchoring on the unique `data-testid`
 * attribute. The pin SVG shares its parent file with the
 * content-type icon, the metadata clock / size icons, the
 * thumbnail loading and fallback icons, the delete menu icon and
 * the title-edit confirm / cancel icons. The non-greedy
 * `[\s\S]*?` followed by the closing `</svg>` would otherwise
 * bleed into the wrong SVG when the same file lists many icons.
 */
function pinSvg(source: string, variant: string): string {
  const match = source.match(
    new RegExp(
      `<svg[^>]*\\bdata-testid="${variant}"[\\s\\S]*?<\\/svg>`,
    ),
  );
  if (!match) {
    throw new Error(`Pin SVG with data-testid="${variant}" not found`);
  }
  return match[0];
}

// ---------------------------------------------------------------------------
// Geometry: viewBox, single-path silhouette, diagonal rotation.
// ---------------------------------------------------------------------------

test("HistoryCard pin SVGs share the 24x24 viewport so the diagonal glyph stays centred", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  const filledSvg = pinSvg(source, "history-card-pin-filled");
  const outlineSvg = pinSvg(source, "history-card-pin-outline");
  assert.match(
    filledSvg,
    /viewBox="0 0 24 24"/,
    "filled pin must declare viewBox 0 0 24 24",
  );
  assert.match(
    outlineSvg,
    /viewBox="0 0 24 24"/,
    "outline pin must declare viewBox 0 0 24 24",
  );
  // Both states render the chincheta at the documented visual
  // size so the silhouette stays inside the dark button
  // background and the toggle does not visibly jump.
  assert.match(
    filledSvg,
    /width="18"/,
    "filled pin keeps the documented 18px width",
  );
  assert.match(
    outlineSvg,
    /width="18"/,
    "outline pin keeps the documented 18px width",
  );
  assert.match(
    filledSvg,
    /height="18"/,
    "filled pin keeps the documented 18px height",
  );
  assert.match(
    outlineSvg,
    /height="18"/,
    "outline pin keeps the documented 18px height",
  );
});

test("HistoryCard pin SVGs render a single diagonal <path> tilted 45° clockwise, no circle/line/polygon", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  for (const variant of [
    "history-card-pin-filled",
    "history-card-pin-outline",
  ]) {
    const svg = pinSvg(source, variant);
    // The chincheta is a single `<path>` inside a rotated group
    // so the wide rounded head, the short body and the pointed
    // tip form one continuous silhouette. The previous
    // `<circle>` + `<line>` + `<polygon>` composition read as a
    // magnifying glass / key and must never come back.
    assert.equal(
      /<circle\b/.test(svg),
      false,
      `${variant} must not declare a <circle> (circle + line + polygon = magnifying glass)`,
    );
    assert.equal(
      /<line\b/.test(svg),
      false,
      `${variant} must not declare a <line> (circle + line + polygon = magnifying glass)`,
    );
    assert.equal(
      /<polygon\b/.test(svg),
      false,
      `${variant} must not declare a <polygon> (circle + line + polygon = magnifying glass)`,
    );
    assert.equal(
      /<polyline\b/.test(svg),
      false,
      `${variant} must not declare a <polyline>`,
    );
    // The pin SVG must rotate the chincheta 45° around the
    // viewport centre so the head ends up in the upper-right
    // quadrant and the tip points to the lower-left.
    assert.match(
      svg,
      /<g[^>]*\btransform="rotate\(45 12 12\)"/,
      `${variant} must rotate the silhouette 45° around the viewport centre`,
    );
    // The whole silhouette is rendered as a single `<path>`.
    const pathMatch = svg.match(/<path\b[^>]*\/>/);
    assert.ok(pathMatch, `${variant} must declare a single <path>`);
    assert.match(
      pathMatch[0],
      /\bd="[^"]+"/,
      `${variant} <path> must carry a non-empty "d" attribute`,
    );
  }
});

test("HistoryCard pin path keeps the head dome in the upper half and the tip apex in the lower half of the 24x24 viewport", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  for (const variant of [
    "history-card-pin-filled",
    "history-card-pin-outline",
  ]) {
    const svg = pinSvg(source, variant);
    const pathMatch = svg.match(/<path\b[^>]*\bd="([^"]+)"/);
    assert.ok(pathMatch, `${variant} must declare a <path> with "d"`);
    const d = pathMatch[1] ?? "";
    // Extract every numeric coordinate from the path data so we
    // can sanity-check that the silhouette stays inside the
    // 24x24 viewport and that the head sits above the tip.
    const numbers = d.match(/-?\d+(?:\.\d+)?/g) ?? [];
    const coords = numbers.map((value) => Number(value));
    assert.ok(coords.length >= 4, `${variant} path must carry coordinates`);
    const xs: number[] = [];
    const ys: number[] = [];
    for (let i = 0; i + 1 < coords.length; i += 2) {
      xs.push(coords[i]);
      ys.push(coords[i + 1]);
    }
    assert.ok(xs.length > 0 && ys.length > 0);
    const minX = Math.min(...xs);
    const maxX = Math.max(...xs);
    const minY = Math.min(...ys);
    const maxY = Math.max(...ys);
    // The path itself is drawn vertically (head on top, tip on
    // bottom). The `rotate(45 12 12)` group then tilts it 45°
    // clockwise so the rotated silhouette has the head in the
    // upper-right and the tip in the lower-left.
    //
    // In the un-rotated frame: the head dome sits near the top
    // and the tip sits near the bottom.
    assert.ok(
      minY < 8,
      `${variant} head dome must sit in the upper half (saw minY=${minY})`,
    );
    assert.ok(
      maxY > 18,
      `${variant} tip apex must sit in the lower half (saw maxY=${maxY})`,
    );
    // After the rotate(45 12 12) the silhouette spans roughly
    // 16–18 units of the viewport, so all coordinates stay
    // between 0 and 24.
    assert.ok(
      minX >= 0 && maxX <= 24,
      `${variant} path x range must stay inside 0..24 (saw ${minX}..${maxX})`,
    );
    assert.ok(
      minY >= 0 && maxY <= 24,
      `${variant} path y range must stay inside 0..24 (saw ${minY}..${maxY})`,
    );
  }
});

// ---------------------------------------------------------------------------
// Fill / stroke contract: outlined unpinned, filled pinned.
// ---------------------------------------------------------------------------

test("HistoryCard pin outline variant renders the silhouette with stroke-only and no fill", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  const svg = pinSvg(source, "history-card-pin-outline");
  // The outline variant must paint an empty silhouette: the
  // path declares `fill="none"` and relies on the
  // `stroke="currentColor"` attribute so the gray/lavender
  // outline stays visible against the dark button surface.
  const pathMatch = svg.match(/<path\b[^>]*\/>/);
  assert.ok(pathMatch, "outline pin must declare a <path>");
  assert.match(
    pathMatch[0],
    /fill="none"/,
    'outline path must use fill="none"',
  );
  assert.match(
    pathMatch[0],
    /stroke="currentColor"/,
    'outline path must use stroke="currentColor"',
  );
});

test("HistoryCard pin filled variant renders the silhouette as a solid yellow glyph", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  const svg = pinSvg(source, "history-card-pin-filled");
  // The filled variant must NOT declare `fill="none"` anywhere:
  // the silhouette is solid so the yellow fill of the pinned
  // state reads as a single, recognisable chincheta.
  assert.equal(
    /fill="none"/.test(svg),
    false,
    'filled pin must not use fill="none" on any shape',
  );
  const pathMatch = svg.match(/<path\b[^>]*\/>/);
  assert.ok(pathMatch, "filled pin must declare a <path>");
  assert.match(
    pathMatch[0],
    /fill="currentColor"/,
    'filled path must be a solid silhouette (fill="currentColor")',
  );
  assert.match(
    pathMatch[0],
    /stroke="currentColor"/,
    'filled path must keep stroke="currentColor" so the silhouette stays unified',
  );
});

// ---------------------------------------------------------------------------
// CSS contract: dark button background, gray/lavender outline, yellow filled.
// ---------------------------------------------------------------------------

test("HistoryCard pin button keeps a dark background in both states so the yellow fill stays legible", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  // The pin button shares the base rule with the menu trigger,
  // pinning the background to `#1f2937`. The pinned override must
  // NOT change the background — only the icon colour.
  assert.match(
    source,
    /\.card-actions\s+:global\(\.pin\),\s*\.card-actions\s+:global\(\.menu-trigger\)\s*\{[^}]*background:\s*#1f2937/s,
  );
  // The pinned override must NOT override `background`; it only
  // changes the icon colour to the documented yellow.
  const pressedBlock = source.match(
    /\.card-actions\s+:global\(\.pin\[aria-pressed="true"\]\)\s*\{[^}]*\}/,
  );
  assert.ok(pressedBlock, "pinned state must override icon colour");
  assert.equal(
    /background:\s*#facc15/.test(pressedBlock[0]),
    false,
    "pinned state must not paint the button yellow (the dark background must stay)",
  );
  assert.match(
    pressedBlock[0],
    /color:\s*#f5c542/i,
    "pinned state must use the documented yellow `#f5c542` for the filled silhouette",
  );
});

test("HistoryCard pin button paints the unpinned outline with the documented gray/lavender tone", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  // The unpinned default uses `#aab4c8` (gray/lavender) so the
  // outlined silhouette stays visible on the dark button surface
  // without competing with the filled pinned variant.
  const baseBlock = source.match(
    /\.card-actions\s+:global\(\.pin\)\s*\{[^}]*\}/,
  );
  assert.ok(baseBlock, "pin button base rule must exist");
  assert.match(
    baseBlock[0],
    /color:\s*#aab4c8/i,
    "unpinned pin must use `#aab4c8` so the outlined chincheta is legible",
  );
});

test("HistoryCard pin button keeps focus-visible, tooltip and disabled affordances", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  assert.match(
    source,
    /\.card-actions\s+:global\(\.pin:focus-visible\)\s*\{[^}]*outline:/s,
  );
  // The disabled branch lowers opacity so the affordance is
  // visually muted while the pin stays keyboard-reachable.
  assert.match(
    source,
    /\.card-actions\s+:global\(\.pin:disabled\)\s*\{[^}]*opacity:/s,
  );
});

// ---------------------------------------------------------------------------
// Accessibility + data-testid preservation.
// ---------------------------------------------------------------------------

test("HistoryCard pin control keeps the documented data-testid, aria and click wiring", () => {
  const source = stripComments(loadSource("src/HistoryCard.svelte"));
  assert.match(source, /data-testid="history-card-pin"/);
  assert.match(source, /data-testid="history-card-pin-filled"/);
  assert.match(source, /data-testid="history-card-pin-outline"/);
  assert.match(source, /aria-pressed=\{entry\.is_pinned\}/);
  assert.match(source, /title=\{entry\.is_pinned \? "Desanclar" : "Anclar"\}/);
  assert.match(
    source,
    /aria-label=\{entry\.is_pinned \? `Desanclar entrada \$\{displayTitle\}` : `Anclar entrada \$\{displayTitle\}`\}/,
  );
  assert.match(source, /data-pinned=\{entry\.is_pinned \? "true" : "false"\}/);
  assert.match(source, /on:click=\{handlePinClick\}/);
});

test("HistoryCard pin control never paints a star, heart, bookmark or textual glyph anywhere", () => {
  const source = loadSource("src/HistoryCard.svelte");
  for (const forbidden of [
    "★",
    "☆",
    "⭐",
    "🌟",
    "✦",
    "✧",
    "❋",
    "✱",
    "♥",
    "❤",
    "♦",
    "♣",
    "♠",
    "►",
    "▾",
    "📌",
    "📍",
  ]) {
    assert.equal(
      source.includes(forbidden),
      false,
      `pin control must never render the glyph "${forbidden}"`,
    );
  }
  // The pin control must never render remote resources, no PNG, no
  // external sprite and no Unicode escape.
  assert.equal(/src=["']https?:/.test(source), false);
  assert.equal(/href=["']https?:/.test(source), false);
  assert.equal(/\\u\{/.test(source), false);
});
