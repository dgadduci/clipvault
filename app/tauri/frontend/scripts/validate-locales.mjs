import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../src/locales");
const languages = ["en", "es", "pt", "de", "fr"];
const catalogs = Object.fromEntries(
  languages.map((language) => [
    language,
    JSON.parse(fs.readFileSync(path.join(root, `${language}.json`), "utf8")),
  ]),
);
const referenceKeys = Object.keys(catalogs.en).sort();
const referenceSet = new Set(referenceKeys);
const placeholderPattern = /\{([a-zA-Z0-9_]+)\}/g;
let failed = false;

for (const language of languages) {
  const catalog = catalogs[language];
  const keys = Object.keys(catalog).sort();
  const missing = referenceKeys.filter((key) => !(key in catalog));
  const extra = keys.filter((key) => !referenceSet.has(key));
  const empty = keys.filter((key) => typeof catalog[key] !== "string" || !catalog[key].trim());

  if (missing.length || extra.length || empty.length) {
    failed = true;
    console.error(`${language}: missing=[${missing.join(", ")}] extra=[${extra.join(", ")}] empty=[${empty.join(", ")}]`);
  }

  for (const key of referenceKeys) {
    if (!(key in catalog)) continue;
    const expected = [...catalogs.en[key].matchAll(placeholderPattern)].map((match) => match[1]).sort();
    const actual = [...catalog[key].matchAll(placeholderPattern)].map((match) => match[1]).sort();
    if (JSON.stringify(expected) !== JSON.stringify(actual)) {
      failed = true;
      console.error(`${language}: placeholder mismatch for ${key}`);
    }
  }
}

if (failed) process.exitCode = 1;
else console.log(`Validated ${referenceKeys.length} keys across ${languages.length} locale catalogs.`);
