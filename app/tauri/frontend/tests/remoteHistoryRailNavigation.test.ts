/**
 * Behavioural coverage for the document-level keydown listener
 * the remote-history rail attaches. The previous regression
 * surfaced on QA when the listener intercepted `ArrowLeft` /
 * `ArrowRight` while the focus sat on any element in the
 * document — including the sidebar search field, the
 * collection selector and the import buttons in cards that
 * lived in a different panel. The pure helpers the rail calls
 * (`mapHorizontalArrowKey`, `isInteractiveControl`,
 * `isInsideRailSurface`, `shouldConsumeHorizontalRailKey`)
 * live in
 * `../src/lib/remoteHistoryRailNavigation.ts`; the suite below
 * pins the consume / fall-through decision byte-for-byte.
 *
 * Every test is a plain `node:test` assertion against the
 * pure helpers plus a small DOM-stub tree so the focus chain
 * the rail walks is exercised end-to-end. The polyfill lives
 * next to the file in `_domPolyfill.ts`.
 */

import { test } from "node:test";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import {
  mapHorizontalArrowKey,
  isInteractiveControl,
  isInsideRailSurface,
  shouldConsumeHorizontalRailKey,
} from "../src/lib/remoteHistoryRailNavigation.ts";

import {
  createRemoteRailSurface,
  type RailSurfaceFixture,
} from "./_remoteRailSurfaceFixture.ts";

function fixture(): RailSurfaceFixture {
  return createRemoteRailSurface();
}

const REMOTE_RAIL_SOURCE = readFileSync(
  resolve(process.cwd(), "src/RemoteHistoryRail.svelte"),
  "utf8",
);

test("mouse selection focuses the roving card and handled arrows focus the next card", () => {
  assert.match(
    REMOTE_RAIL_SOURCE,
    /function selectRemoteEntry\(remoteEntryId: string\): void \{[\s\S]*?selectedRemoteEntryId = remoteEntryId;[\s\S]*?cardEls\.get\(remoteEntryId\)\?\.focus\(\{ preventScroll: true \}\)/,
  );
  assert.match(
    REMOTE_RAIL_SOURCE,
    /event\.preventDefault\(\);[\s\S]*?selectedRemoteEntryId = navigation\.nextId;[\s\S]*?cardEls\.get\(navigation\.nextId\)\?\.focus\(\{ preventScroll: true \}\)/,
  );
});

test("mapHorizontalArrowKey translates the two horizontal arrows and rejects every other key", () => {
  assert.equal(mapHorizontalArrowKey("ArrowRight"), "right");
  assert.equal(mapHorizontalArrowKey("ArrowLeft"), "left");
  assert.equal(mapHorizontalArrowKey("ArrowUp"), null);
  assert.equal(mapHorizontalArrowKey("ArrowDown"), null);
  assert.equal(mapHorizontalArrowKey("Enter"), null);
  assert.equal(mapHorizontalArrowKey(" "), null);
});

test("isInteractiveControl rejects native HTML controls", () => {
  const surface = fixture();
  try {
    const input = surface.railSurface.ownerDocument.createElement("input");
    input.type = "text";
    surface.sidebar.appendChild(input);
    assert.equal(isInteractiveControl(input), true);

    const select = surface.railSurface.ownerDocument.createElement("select");
    surface.sidebar.appendChild(select);
    assert.equal(isInteractiveControl(select), true);

    const button = surface.railSurface.ownerDocument.createElement("button");
    surface.sidebar.appendChild(button);
    assert.equal(isInteractiveControl(button), true);

    const anchor = surface.railSurface.ownerDocument.createElement("a");
    anchor.setAttribute("href", "#");
    surface.sidebar.appendChild(anchor);
    assert.equal(isInteractiveControl(anchor), true);
  } finally {
    surface.dispose();
  }
});

test("isInteractiveControl rejects contentEditable and menu roles but allows rail navigation roles", () => {
  const surface = fixture();
  try {
    const editable = surface.railSurface.ownerDocument.createElement("div");
    editable.setAttribute("contenteditable", "true");
    surface.sidebar.appendChild(editable);
    assert.equal(isInteractiveControl(editable), true);

    const menu = surface.railSurface.ownerDocument.createElement("div");
    menu.setAttribute("role", "menu");
    surface.sidebar.appendChild(menu);
    assert.equal(isInteractiveControl(menu), true);

    const menuItem = surface.railSurface.ownerDocument.createElement("div");
    menuItem.setAttribute("role", "menuitem");
    surface.sidebar.appendChild(menuItem);
    assert.equal(isInteractiveControl(menuItem), true);

    const cardSurface = surface.railSurface.ownerDocument.createElement("article");
    cardSurface.setAttribute("role", "option");
    surface.railSurface.appendChild(cardSurface);
    assert.equal(isInteractiveControl(cardSurface), false);
    assert.equal(isInteractiveControl(surface.railSurface), false);
  } finally {
    surface.dispose();
  }
});

test("isInsideRailSurface returns true for descendants of the rail root and false elsewhere", () => {
  const surface = fixture();
  try {
    const cardSurface = surface.railSurface.ownerDocument.createElement("article");
    surface.railSurface.appendChild(cardSurface);
    assert.equal(isInsideRailSurface(cardSurface), true);
    assert.equal(isInsideRailSurface(surface.railSurface), true);
    assert.equal(isInsideRailSurface(surface.sidebar), false);
    assert.equal(isInsideRailSurface(surface.sidebarButton), false);
    assert.equal(isInsideRailSurface(null), false);
  } finally {
    surface.dispose();
  }
});

test("shouldConsumeHorizontalRailKey consumes arrows on listbox and option surfaces", () => {
  const surface = fixture();
  try {
    const cardSurface = surface.railSurface.ownerDocument.createElement("article");
    cardSurface.setAttribute("role", "option");
    surface.railSurface.appendChild(cardSurface);
    assert.equal(
      shouldConsumeHorizontalRailKey("ArrowRight", surface.railSurface),
      true,
    );
    assert.equal(
      shouldConsumeHorizontalRailKey("ArrowLeft", surface.railSurface),
      true,
    );
    assert.equal(
      shouldConsumeHorizontalRailKey("ArrowRight", cardSurface),
      true,
    );
    assert.equal(
      shouldConsumeHorizontalRailKey("ArrowLeft", cardSurface),
      true,
    );
  } finally {
    surface.dispose();
  }
});

test("shouldConsumeHorizontalRailKey refuses to consume the key when the focus sits on the rail menu/import buttons", () => {
  const surface = fixture();
  try {
    assert.equal(
      shouldConsumeHorizontalRailKey("ArrowRight", surface.menuButton),
      false,
    );
    assert.equal(
      shouldConsumeHorizontalRailKey("ArrowLeft", surface.importButton),
      false,
    );
  } finally {
    surface.dispose();
  }
});

test("shouldConsumeHorizontalRailKey refuses to consume the key when the focus sits outside the rail", () => {
  const surface = fixture();
  try {
    // Sidebar search field is the canonical regression: the
    // document-level listener must NOT consume horizontal
    // arrows typed into a control that lives in a different
    // panel. The previous baseline consumed the key and the
    // user could not move the caret with `ArrowLeft`.
    assert.equal(
      shouldConsumeHorizontalRailKey("ArrowRight", surface.sidebarSearch),
      false,
    );
    assert.equal(
      shouldConsumeHorizontalRailKey("ArrowLeft", surface.sidebarSearch),
      false,
    );
    assert.equal(
      shouldConsumeHorizontalRailKey("ArrowRight", surface.sidebarButton),
      false,
    );
    // A button inside a sibling panel that mimics the
    // collection selector must also be left alone.
    assert.equal(
      shouldConsumeHorizontalRailKey("ArrowRight", surface.siblingPanelButton),
      false,
    );
    // An anchor / link outside the rail is also untouched.
    assert.equal(
      shouldConsumeHorizontalRailKey("ArrowRight", surface.siblingLink),
      false,
    );
  } finally {
    surface.dispose();
  }
});

test("shouldConsumeHorizontalRailKey returns false for non-arrow keys regardless of focus", () => {
  const surface = fixture();
  try {
    const cardSurface = surface.railSurface.ownerDocument.createElement("article");
    surface.railSurface.appendChild(cardSurface);
    for (const key of ["Enter", " ", "Tab", "ArrowUp", "ArrowDown"]) {
      assert.equal(
        shouldConsumeHorizontalRailKey(key, cardSurface),
        false,
        `key ${key} must not be consumed`,
      );
    }
  } finally {
    surface.dispose();
  }
});

test("shouldConsumeHorizontalRailKey returns false when the target is null", () => {
  assert.equal(shouldConsumeHorizontalRailKey("ArrowRight", null), false);
  assert.equal(shouldConsumeHorizontalRailKey("ArrowLeft", null), false);
});
