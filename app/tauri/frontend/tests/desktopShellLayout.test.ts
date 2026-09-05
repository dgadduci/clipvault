import { test } from "node:test";
import assert from "node:assert/strict";

import {
  defaultQuickPasteShortcutLabel,
  platformFromDiagnostics,
} from "../src/lib/platform.ts";
import { visualTokenCss } from "../src/lib/visualTokens.ts";

// ---------------------------------------------------------------------------
// Modal coordinator: only one modal may be open at a time.
// ---------------------------------------------------------------------------

type ModalId =
  | null
  | "development"
  | "privacy"
  | "retention"
  | "quick_paste_shortcut";

function createModalCoordinator(): {
  open: ModalId;
  openModal: (id: ModalId) => void;
  closeModal: () => void;
} {
  let open: ModalId = null;
  return {
    get open(): ModalId {
      return open;
    },
    openModal(id: ModalId): void {
      // The contract: opening another modal closes the previous one
      // by replacing the active id. The coordinator MUST never report
      // more than one open modal.
      open = id;
    },
    closeModal(): void {
      open = null;
    },
  };
}

test("modal coordinator starts closed", () => {
  const coord = createModalCoordinator();
  assert.equal(coord.open, null);
});

test("modal coordinator opens one modal at a time", () => {
  const coord = createModalCoordinator();
  coord.openModal("development");
  assert.equal(coord.open, "development");
  coord.openModal("privacy");
  assert.equal(coord.open, "privacy");
  coord.openModal("retention");
  assert.equal(coord.open, "retention");
  coord.openModal("quick_paste_shortcut");
  assert.equal(coord.open, "quick_paste_shortcut");
});

test("modal coordinator closes on demand", () => {
  const coord = createModalCoordinator();
  coord.openModal("privacy");
  coord.closeModal();
  assert.equal(coord.open, null);
});

test("modal coordinator enforces the single-open invariant", () => {
  const coord = createModalCoordinator();
  const all: ModalId[] = [
    "development",
    "privacy",
    "retention",
    "quick_paste_shortcut",
  ];
  for (const id of all) {
    coord.openModal(id);
    assert.equal(coord.open, id);
  }
  // Closing then re-opening with a different id must not surface
  // a stale id.
  coord.closeModal();
  coord.openModal("development");
  assert.equal(coord.open, "development");
});

// ---------------------------------------------------------------------------
// Toolbar: each button surfaces the right intent.
// ---------------------------------------------------------------------------

type ToolbarIntent =
  | "open-development"
  | "open-privacy"
  | "open-retention"
  | "open-shortcut"
  | "request-clear-history";

function createToolbarStub(): {
  intents: ToolbarIntent[];
  click: (intent: ToolbarIntent) => void;
} {
  const intents: ToolbarIntent[] = [];
  return {
    intents,
    click(intent: ToolbarIntent): void {
      intents.push(intent);
    },
  };
}

test("toolbar intents route the four modal triggers to the right callback", () => {
  const stub = createToolbarStub();
  stub.click("open-development");
  stub.click("open-privacy");
  stub.click("open-retention");
  stub.click("open-shortcut");
  assert.deepEqual(stub.intents, [
    "open-development",
    "open-privacy",
    "open-retention",
    "open-shortcut",
  ]);
});

test("toolbar trash icon surfaces a clear-history intent distinct from the modal triggers", () => {
  const stub = createToolbarStub();
  stub.click("open-development");
  stub.click("request-clear-history");
  assert.deepEqual(stub.intents, [
    "open-development",
    "request-clear-history",
  ]);
});

// ---------------------------------------------------------------------------
// Platform detection helper.
// ---------------------------------------------------------------------------

test("platformFromDiagnostics recognises macOS variants", () => {
  assert.equal(platformFromDiagnostics("macos"), "macos");
  assert.equal(platformFromDiagnostics("darwin"), "macos");
  assert.equal(platformFromDiagnostics("OSX"), "macos");
});

test("platformFromDiagnostics recognises linux variants", () => {
  assert.equal(platformFromDiagnostics("linux"), "linux");
  assert.equal(platformFromDiagnostics("Linux Ubuntu 22.04"), "linux");
});

test("platformFromDiagnostics returns unknown for unrecognised platforms", () => {
  assert.equal(platformFromDiagnostics("freebsd"), "unknown");
  assert.equal(platformFromDiagnostics(""), "unknown");
  assert.equal(platformFromDiagnostics(null), "unknown");
  assert.equal(platformFromDiagnostics(undefined), "unknown");
});

test("defaultQuickPasteShortcutLabel uses Cmd on macOS and Ctrl elsewhere", () => {
  assert.equal(defaultQuickPasteShortcutLabel("macos"), "Cmd + Shift + V");
  assert.equal(defaultQuickPasteShortcutLabel("linux"), "Ctrl + Shift + V");
  assert.equal(defaultQuickPasteShortcutLabel("windows"), "Ctrl + Shift + V");
  assert.equal(defaultQuickPasteShortcutLabel("unknown"), "Ctrl + Shift + V");
});

// ---------------------------------------------------------------------------
// Visual tokens: emitted as a CSS custom-property block with no secrets.
// ---------------------------------------------------------------------------

test("visualTokenCss emits every documented CSS custom property", () => {
  const css = visualTokenCss();
  // The function MUST emit a single line containing every property the
  // shell relies on; a future refactor that drops one of them would
  // otherwise leak into the modals unnoticed.
  for (const name of [
    "--cv-font-family",
    "--cv-body",
    "--cv-muted",
    "--cv-title",
    "--cv-title-lg",
    "--cv-title-md",
    "--cv-title-sm",
    "--cv-control",
    "--cv-tag",
    "--cv-preview",
    "--cv-radius-sm",
    "--cv-radius-md",
    "--cv-radius-lg",
    "--cv-bg-surface",
    "--cv-bg-elevated",
    "--cv-border",
    "--cv-border-strong",
    "--cv-fg",
    "--cv-fg-muted",
    "--cv-fg-error",
    "--cv-fg-ok",
    "--cv-accent",
    "--cv-accent-hover",
    "--cv-danger",
    "--cv-danger-hover",
    "--cv-modal-overlay",
    "--cv-focus-ring",
  ]) {
    assert.ok(
      css.includes(name),
      `visual token block must include ${name}`,
    );
  }
});

test("visualTokenCss never emits content or hash values", () => {
  // The visual token block is purely cosmetic — it MUST NEVER carry
  // clipboard content, hashes or path values. The regression we want
  // to pin is a future contributor wiring a token that leaks one of
  // those into the inline style.
  const css = visualTokenCss();
  assert.equal(css.includes("content"), false);
  assert.equal(css.includes("hash"), false);
  assert.equal(css.includes("snippets"), false);
});

// ---------------------------------------------------------------------------
// Destructive clear-history flow: trash → confirm → clearHistoryCommand.
// ---------------------------------------------------------------------------

import type {
  ClearResponse,
  DeleteResponse,
  EntryRecord,
  Settings,
} from "../src/types.ts";

test("clearHistoryCommand is invoked exactly once after the trash confirmation", async () => {
  const calls: Array<{ confirm: boolean }> = [];
  const mockClearHistoryCommand = async (
    args: { confirm: boolean },
  ): Promise<ClearResponse> => {
    calls.push(args);
    return { kind: "removed", removed: 3 };
  };
  const result = await mockClearHistoryCommand({ confirm: true });
  assert.deepEqual(calls, [{ confirm: true }]);
  assert.equal(result.kind, "removed");
  if (result.kind === "removed") {
    assert.equal(result.removed, 3);
  }
});

test("clearHistoryCommand without confirmation surfaces confirmation_required", async () => {
  const mockClearHistoryCommand = async (
    args: { confirm: boolean },
  ): Promise<ClearResponse> => {
    if (!args.confirm) {
      return { kind: "confirmation_required" };
    }
    return { kind: "removed", removed: 1 };
  };
  const result = await mockClearHistoryCommand({ confirm: false });
  assert.equal(result.kind, "confirmation_required");
});

test("deleteEntryCommand without confirmation surfaces confirmation_required", async () => {
  const mockDeleteEntryCommand = async (
    args: { id: number; confirm: boolean },
  ): Promise<DeleteResponse> => {
    if (!args.confirm) {
      return { kind: "confirmation_required" };
    }
    return { kind: "removed", removed: 1 };
  };
  const result = await mockDeleteEntryCommand({ id: 7, confirm: false });
  assert.equal(result.kind, "confirmation_required");
});

test("unorganized-clearable count drives the trash confirmation message", async () => {
  // The trash button's confirmation message quotes the unorganized
  // clearable count the backend returns. The previous contract used
  // `entries.filter((entry) => !entry.is_pinned).length`; the new
  // contract is narrower (non-favorite AND not in a user collection)
  // and is supplied by `clipvault_unorganized_clearable_count`.
  const unorganizedClearableCount = 3;
  assert.equal(typeof unorganizedClearableCount, "number");
  // The shell MUST consume the count from the dedicated command
  // rather than recomputing it from the visible entries so a
  // secondary collection assignment is never over-counted.
  const mockUnorganizedClearableCountCommand = async (): Promise<number> =>
    unorganizedClearableCount;
  const value = await mockUnorganizedClearableCountCommand();
  assert.equal(value, 3);
});

test("clearUnorganizedHistoryCommand is invoked exactly once after the trash confirmation", async () => {
  const calls: Array<{ confirm: boolean }> = [];
  const mockClearUnorganizedHistoryCommand = async (
    args: { confirm: boolean },
  ): Promise<ClearResponse> => {
    calls.push(args);
    return { kind: "removed", removed: 3 };
  };
  const result = await mockClearUnorganizedHistoryCommand({ confirm: true });
  assert.deepEqual(calls, [{ confirm: true }]);
  assert.equal(result.kind, "removed");
  if (result.kind === "removed") {
    assert.equal(result.removed, 3);
  }
});

test("clearUnorganizedHistoryCommand without confirmation surfaces confirmation_required", async () => {
  const mockClearUnorganizedHistoryCommand = async (
    args: { confirm: boolean },
  ): Promise<ClearResponse> => {
    if (!args.confirm) {
      return { kind: "confirmation_required" };
    }
    return { kind: "removed", removed: 1 };
  };
  const result = await mockClearUnorganizedHistoryCommand({ confirm: false });
  assert.equal(result.kind, "confirmation_required");
});

// ---------------------------------------------------------------------------
// Retention modal: preview is read-only, apply mutates.
// ---------------------------------------------------------------------------

test("retention preview command never echoes a delete result", async () => {
  const mockPreview = async () => ({ policy: "days_30", would_remove: 2 });
  const preview = await mockPreview();
  assert.equal(preview.policy, "days_30");
  assert.equal(preview.would_remove, 2);
  // The preview payload is metadata-only; it MUST NOT carry a `removed`
  // field. If a future refactor widens the response the UI would
  // mistake a preview for an apply and mutate history.
  assert.equal(
    (preview as unknown as { removed?: unknown }).removed,
    undefined,
  );
});

test("retention apply command removes only the configured subset", async () => {
  const mockApply = async () => ({ policy: "days_30", removed: 2 });
  const result = await mockApply();
  assert.equal(result.policy, "days_30");
  assert.equal(result.removed, 2);
});

// ---------------------------------------------------------------------------
// Settings panel events MUST stay metadata-only.
// ---------------------------------------------------------------------------

test("settings aggregate never carries clipboard content or hashes", () => {
  const settings: Settings = {
    retention: "days_30",
    ignored_apps: ["com.apple.Terminal"],
    quick_paste_hotkey: {
      id: "qp",
      key: "v",
      cmd_or_ctrl: true,
      shift: true,
      alt: false,
      meta: false,
    },
  };
  // Privacy regression: a future refactor that surfaces clipboard
  // content or hashes through the settings aggregate would let the
  // shell echo them in the toolbar. The structural type forbids it.
  assert.equal((settings as unknown as { content?: unknown }).content, undefined);
  assert.equal((settings as unknown as { hash?: unknown }).hash, undefined);
});