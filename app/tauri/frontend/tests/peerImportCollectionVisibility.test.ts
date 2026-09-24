/**
 * Regression coverage for the `peer-import-collection-visibility`
 * change. The change projects a metadata-only `is_peer_bound` flag and
 * the current visible peer display name onto every collection row.
 * The sidebar must render an accessible remote-origin marker for
 * peer-bound user collections and MUST NOT leak the raw `peer_id`,
 * certificate fingerprint, network identity, content or any other
 * secret through the DOM, the accessible label, the tooltip or the
 * `data-*` attributes.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

const FRONTEND_ROOT = process.cwd();

function source(...segments: string[]): string {
  return readFileSync(path.join(FRONTEND_ROOT, ...segments), "utf8")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "");
}

const sidebarSource = source("src/OrganizationSidebar.svelte");
const typesSource = source("src/types.ts");

// ---------------------------------------------------------------------------
// Types: Collection carries is_peer_bound and peer_display_name.
// ---------------------------------------------------------------------------

test("Collection type declares is_peer_bound and peer_display_name", () => {
  assert.match(
    typesSource,
    /export interface Collection \{[\s\S]*?is_peer_bound\?: boolean/,
    "Collection must expose is_peer_bound?",
  );
  assert.match(
    typesSource,
    /export interface Collection \{[\s\S]*?peer_display_name\?: string \| null/,
    "Collection must expose peer_display_name? as a safe metadata-only field",
  );
});

// ---------------------------------------------------------------------------
// Helper: remoteOriginLabel produces the accessible label.
// ---------------------------------------------------------------------------

test("remoteOriginLabel returns an empty string for non peer-bound rows", () => {
  // The helper must early-return when is_peer_bound is false so the
  // template never produces a `data-testid="sidebar-collection-remote-origin"`
  // for system collections or unbound user collections.
  const helper = sidebarSource.match(
    /function remoteOriginLabel\([\s\S]*?\n  \}/,
  );
  assert.ok(helper, "remoteOriginLabel helper must exist");
  assert.match(
    helper[0],
    /if\s*\(!collection\.is_peer_bound\)\s*\{\s*return\s+["']["'];\s*\}/,
    "remoteOriginLabel must short-circuit for non peer-bound collections",
  );
});

test("remoteOriginLabel uses the peer_display_name when present", () => {
  // The accessible label must include the current visible peer name
  // exactly as the backend join produces it so screen readers and
  // tooltips describe the binding without exposing the raw `peer_id`.
  const helper = sidebarSource.match(
    /function remoteOriginLabel\([\s\S]*?\n  \}/,
  );
  assert.ok(helper, "remoteOriginLabel helper must exist");
  assert.match(
    helper[0],
    /collection\.peer_display_name\s*\?\?\s*["']["']/,
    "remoteOriginLabel must read peer_display_name with a nullish fallback",
  );
  assert.match(
    helper[0],
    /Importadas de \$\{peerName\}/,
    "remoteOriginLabel must format the visible peer name as 'Importadas de <peer>'",
  );
});

test("remoteOriginLabel falls back to a generic safe label when peer name is empty", () => {
  // When the peer is unknown, has no persisted display name, or the
  // display name is empty / whitespace-only, the sidebar must show a
  // generic safe label ("Importadas de equipo remoto") and MUST NOT
  // surface the raw `peer_id` or any other identifier.
  const helper = sidebarSource.match(
    /function remoteOriginLabel\([\s\S]*?\n  \}/,
  );
  assert.ok(helper, "remoteOriginLabel helper must exist");
  assert.match(
    helper[0],
    /peerName\.length\s*>\s*0/,
    "remoteOriginLabel must guard on a non-empty trimmed peer name",
  );
  assert.match(
    helper[0],
    /Importadas de equipo remoto/,
    "remoteOriginLabel must surface a generic safe fallback label",
  );
  assert.equal(
    /peer_id/.test(helper[0]),
    false,
    "remoteOriginLabel must never reference peer_id",
  );
});

// ---------------------------------------------------------------------------
// Template: the badge appears ONLY for peer-bound user collections.
// ---------------------------------------------------------------------------

test("OrganizationSidebar renders the remote-origin badge only for peer-bound user collections", () => {
  // The badge must branch on `is_peer_bound === true && kind === "user"`
  // so system collections and unbound user collections never render the
  // remote-origin surface (and a regression that flips the branch
  // surfaces here).
  assert.match(
    sidebarSource,
    /\{\s*#if isHistory\(collection\)\s*\}[\s\S]*?\{\s*:else if hasRemoteOrigin\(collection\)\s*\}/,
    "the template must host the remote-origin branch after the system branch",
  );
  assert.match(
    sidebarSource,
    /data-testid=["']sidebar-collection-remote-origin["']/,
    "the badge must expose data-testid=sidebar-collection-remote-origin",
  );
  assert.match(
    sidebarSource,
    /data-peer-bound=["']true["']/,
    "the badge must surface data-peer-bound=true so the binding is inspectable",
  );
});

test("hasRemoteOrigin requires both is_peer_bound and a user collection", () => {
  // The guard is the canonical "is this row bound to a peer" check
  // and must require both `is_peer_bound === true` and
  // `kind === "user"` so a system collection that happens to be
  // bound cannot surface the remote-origin marker.
  const helper = sidebarSource.match(
    /function hasRemoteOrigin\([\s\S]*?\n  \}/,
  );
  assert.ok(helper, "hasRemoteOrigin helper must exist");
  assert.match(
    helper[0],
    /collection\.is_peer_bound\s*===\s*true\s*&&\s*collection\.kind\s*===\s*["']user["']/,
    "hasRemoteOrigin must require both is_peer_bound === true AND kind === \"user\"",
  );
});

test("OrganizationSidebar never embeds the raw peer_id in the template", () => {
  // The wire payload never carries the `peer_id`, so the template
  // must not surface it through `data-*` attributes, accessible
  // labels, or tooltips. The tests below pin that invariant from
  // the source. A regression that forwards the binding key surfaces
  // here.
  const block = sidebarSource.match(
    /\{\s*#if isHistory\(collection\)\s*\}[\s\S]*?\{\/if\}\s*<\/button>/,
  );
  assert.ok(block, "the collection-row template block must exist");
  assert.equal(
    /peer_id/.test(block[0]),
    false,
    "the collection-row template must not reference peer_id",
  );
  assert.equal(
    /peer_id/.test(sidebarSource.match(
      /function remoteOriginLabel[\s\S]*?\n  \}/,
    )[0]),
    false,
    "remoteOriginLabel must not reference peer_id",
  );
});

test("OrganizationSidebar paints the badge with a remote-only style", () => {
  // The badge must own its own CSS class so a regression that hides
  // it through the `.system` rule cannot silently drop the marker.
  assert.match(
    sidebarSource,
    /\.badge\.remote\s*\{/,
    "the badge must own a dedicated .badge.remote CSS block",
  );
  assert.match(
    sidebarSource,
    /class=["']badge remote["']/,
    "the template must render the badge with .badge.remote",
  );
});