import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const aboutModal = readFileSync(resolve(process.cwd(), "src/AboutModal.svelte"), "utf8");

test("About keeps the update action visible when updates are disabled", () => {
  assert.match(aboutModal, /<section class="update-block" data-testid="about-update"/);
  assert.match(aboutModal, /\{#if !updaterEnabled\}[\s\S]*?about\.update\.unavailable/);
  assert.match(aboutModal, /\{#if !updaterEnabled\}[\s\S]*?<button[^>]*disabled[\s\S]*?about\.update\.check/);
  assert.doesNotMatch(aboutModal, /\{#if updaterEnabled\}[\s\S]*?data-testid="about-update"/);
});

test("About omits implementation details from the product modal and catalogs", () => {
  assert.doesNotMatch(aboutModal, /about\.version_details/);

  for (const language of ["en", "es", "pt", "de", "fr"]) {
    const catalog = JSON.parse(
      readFileSync(resolve(process.cwd(), `src/locales/${language}.json`), "utf8"),
    ) as Record<string, string>;
    assert.equal(catalog["about.version_details"], undefined, `${language} keeps the removed paragraph`);
    assert.ok(catalog["about.update.unavailable"], `${language} needs the disabled updater message`);
  }
});
