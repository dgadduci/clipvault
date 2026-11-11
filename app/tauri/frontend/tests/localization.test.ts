import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { get } from "svelte/store";

import {
  isLocale,
  localeStore,
  setLocale,
  translate,
  translatePlural,
} from "../src/lib/localization.ts";

test("localization defaults to English and rejects unsupported locale values", () => {
  assert.equal(get(localeStore), "en");
  assert.equal(isLocale("fr"), true);
  assert.equal(isLocale("it"), false);
  assert.equal(setLocale("it"), "en");
  assert.equal(get(localeStore), "en");
});

test("translations resolve locale-aware placeholders and plural forms", () => {
  assert.equal(translate("common.cancel", {}, "fr"), "Annuler");
  assert.equal(
    translatePlural("app.search.results", 2, { query: "clip" }, "fr"),
    "2 résultats pour « clip ».",
  );
});

test("preview pagination labels are present in every supported locale", () => {
  const keys = [
    "settings.pagination.title",
    "settings.pagination.description",
    "settings.pagination.label",
    "settings.pagination.range_error",
    "settings.pagination.page_indicator",
  ];
  const catalogs = ["en", "es", "pt", "de", "fr"].map((locale) =>
    JSON.parse(
      readFileSync(
        path.join(process.cwd(), "src", "locales", `${locale}.json`),
        "utf8",
      ),
    ) as Record<string, string>
  );

  for (const catalog of catalogs) {
    for (const key of keys) {
      assert.equal(typeof catalog[key], "string", `${key} must be translated`);
      assert.notEqual(catalog[key].trim(), "", `${key} must not be empty`);
    }
    assert.match(catalog["settings.pagination.page_indicator"], /\{page\}/);
  }
});
