import { test } from "node:test";
import assert from "node:assert/strict";
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
