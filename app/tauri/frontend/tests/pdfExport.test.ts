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

test("PDF generation completion is emitted to the initiating window before save dialog", () => {
  const command = loadSource("..", "src-tauri", "src", "commands.rs");
  const generate = command.indexOf("PdfExportService::generate(context, &entry_ids)");
  const signal = command.indexOf("window.emit_to(", generate);
  const saveDialog = command.indexOf(".blocking_save_file()", signal);

  assert.ok(generate >= 0 && signal > generate && saveDialog > signal);
  assert.match(command, /window\.emit_to\(window\.label\(\),\s*PDF_EXPORT_GENERATION_COMPLETE_EVENT,\s*\(\)\)/);
  assert.match(command, /PDF_EXPORT_GENERATION_COMPLETE_EVENT:\s*&str\s*=\s*"clipvault:\/\/pdf-export-generation-complete"/);
});

test("Desktop and QuickPaste center a generation spinner and keep PDF actions busy through saving", () => {
  const desktop = loadSource("src", "App.svelte");
  const quickPaste = loadSource("src", "QuickPaste.svelte");
  const overlay = loadSource("src", "PdfExportProgressOverlay.svelte");
  const toolbar = loadSource("src", "DesktopToolbar.svelte");

  for (const source of [desktop, quickPaste]) {
    assert.match(source, /let pdfGenerationInFlight = false;/);
    assert.match(source, /PDF_EXPORT_GENERATION_COMPLETE_EVENT,[\s\S]*?pdfGenerationInFlight = false/);
    assert.match(source, /\{#if pdfGenerationInFlight\}[\s\S]{0,300}<PdfExportProgressOverlay/);
    assert.match(source, /finally\s*\{\s*unlistenGenerationComplete\?\.\(\);\s*pdfGenerationInFlight = false;\s*pdfExportInFlight = false;/);
  }
  assert.match(overlay, /position:\s*fixed;[\s\S]*inset:\s*0/);
  assert.match(overlay, /CircularLoadingIndicator/);
  assert.match(toolbar, /disabled=\{selectedEntryIds\.length === 0 \|\| pdfExporting\}/);
  assert.match(quickPaste, /disabled=\{selectedEntryIds\.size === 0 \|\| pdfExportInFlight\}/);
});

test("PDF page breaks are fixed and the setting is removed from all locales", () => {
  const settings = loadSource("src", "GeneralSettingsModal.svelte");
  const coreSettings = loadSource("..", "..", "..", "crates", "clipvault-core", "src", "settings.rs");
  const renderer = loadSource("..", "..", "..", "crates", "clipvault-core", "src", "pdf_export.rs");

  assert.doesNotMatch(settings, /pdf-page-break|settings\.pdf_export\.(title|description|page_break_between_captures)/);
  assert.doesNotMatch(coreSettings, /pdf_page_break_between_captures/);
  assert.match(renderer, /fn render_captures\(captures: &\[PdfCapture\]\)/);
  assert.match(renderer, /combined\.append_document\(render_document\(/);

  for (const locale of ["en", "es", "pt", "de", "fr"]) {
    const catalog = loadSource("src", "locales", `${locale}.json`);
    assert.match(catalog, /"pdf_export\.generating"/);
    assert.doesNotMatch(catalog, /"settings\.pdf_export\.(title|description|page_break_between_captures)"/);
  }
});
