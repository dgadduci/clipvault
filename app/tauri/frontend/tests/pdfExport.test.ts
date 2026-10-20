import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

import { selectedIdsInSelectionOrder } from "../src/lib/multiSelection.ts";

const FRONTEND_ROOT = process.cwd();

function loadSource(...segments: string[]): string {
  return readFileSync(path.join(FRONTEND_ROOT, ...segments), "utf8");
}

test("QuickPaste PDF export preserves selection order and drops stale rows", () => {
  assert.deepEqual(
    selectedIdsInSelectionOrder(new Set([8, 3, 5]), [3, 5, 8]),
    [8, 3, 5],
  );
  assert.deepEqual(
    selectedIdsInSelectionOrder(new Set([8, 3, 5]), [3, 8]),
    [8, 3],
  );
});

test("Desktop PDF action sits between the menu and delete controls", () => {
  const toolbar = loadSource("src", "DesktopToolbar.svelte");
  const menu = toolbar.indexOf('class="menu-wrapper"');
  const pdf = toolbar.indexOf('class="pdf-export-button"');
  const remove = toolbar.indexOf('class="trash"');

  assert.ok(menu >= 0 && menu < pdf, "PDF follows the overflow menu");
  assert.ok(pdf < remove, "PDF precedes the delete control");
  assert.match(toolbar, /disabled=\{selectedEntryIds\.length === 0 \|\| pdfExporting\}/);
});

test("Desktop and QuickPaste PDF actions show the translated red PDF label", () => {
  const desktop = loadSource("src", "DesktopToolbar.svelte");
  const quickPaste = loadSource("src", "QuickPaste.svelte");
  const label = '{$t("pdf_export.short_label")}';
  const buttonContent = (source: string, className: string): string => {
    const start = source.indexOf(`class="${className}"`);
    const end = source.indexOf("</button>", start);
    assert.ok(start >= 0 && end > start, `${className} button exists`);
    return source.slice(start, end);
  };

  const desktopButton = buttonContent(desktop, "pdf-export-button");
  const quickPasteButton = buttonContent(quickPaste, "qp-export-pdf");
  assert.ok(desktopButton.includes(label));
  assert.ok(quickPasteButton.includes(label));
  assert.doesNotMatch(desktopButton, /<svg/);
  assert.doesNotMatch(quickPasteButton, /<svg/);
  assert.match(desktop, /\.pdf-export-button\s*\{[^}]*color:\s*#f85149/i);
  assert.match(quickPaste, /\.qp-export-pdf\s*\{[^}]*color:\s*#f85149/i);
});

test("QuickPaste PDF action cannot confirm the selected paste stack", () => {
  const quickPaste = loadSource("src", "QuickPaste.svelte");
  assert.match(quickPaste, /class="qp-export-pdf"/);
  assert.match(
    quickPaste,
    /on:click\|stopPropagation=\{\(\) => void exportSelectedEntriesAsPdf\(\)\}/,
  );
  assert.match(quickPaste, /selectedIdsInSelectionOrder\(selectedEntryIds, resultIds\)/);
});

test("frontend bridge invokes one native PDF export command with ordered IDs", () => {
  const bridge = loadSource("src", "lib", "tauri.ts");
  const command = loadSource("..", "src-tauri", "src", "commands.rs");
  assert.match(bridge, /invoke<PdfExportSaveResult>\("clipvault_export_pdf"/);
  assert.match(command, /\.blocking_save_file\(\)/);
  assert.match(command, /std::fs::write\(path, pdf\)/);
});
