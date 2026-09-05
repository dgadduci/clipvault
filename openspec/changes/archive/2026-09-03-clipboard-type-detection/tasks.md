## 1. Core detector

- [x] 1.1 New `clipvault_core::content_type` module exposing
  `detect_content_type(&str) -> ContentType`. Pure function: no Tauri,
  no SQLite, no clipboard, no logger, no filesystem, no network.
  Implemented at `crates/clipvault-core/src/content_type.rs`.
- [x] 1.2 Detection rules in stable precedence order: JWT → UUID →
  IPv6 → IPv4 → Email → URL → HexColor → JSON → HTML → SQL →
  ShellCommand → Code → FilePath → Text fallback. Documented in
  `crates/clipvault-core/src/content_type.rs` and `design.md`.
- [x] 1.3 Helper `ContentType::is_textual()` on the DB enum so the
  repository and the frontend agree on the set of textual types
  without duplicating the list. Implemented at
  `crates/clipvault-db/src/entry.rs`.

## 2. Core detector unit tests (table-driven per type)

- [x] 2.1 `cases::url` covers `https://example.com`, `http://foo`,
  `ftp://`, `mailto:`, `ssh://`, `tel:`, `file://`. Negative cases:
  `example.com`, `hello world`, `https://`.
- [x] 2.2 `cases::email` covers single address, subdomains,
  `+aliases`. Negative cases: `foo@`, `@bar`, `foo bar@example.com`,
  natural prose with `@`.
- [x] 2.3 `cases::json` covers `{ "k": 1 }`, `[1,2,3]`, nested
  objects. Negative cases: malformed JSON, `null`, `true`, `42`,
  `"string"`, `{}` and `[]` alone fall to `Text` (empty container).
- [x] 2.4 `cases::jwt` covers a real three-segment token with
  `eyJ…`. Negative cases: missing segment, non-base64url chars, only
  one segment.
- [x] 2.5 `cases::uuid` covers standard form, brace-wrapped,
  paren-wrapped, `urn:uuid:`, no-hyphens. Negative cases: random
  hyphenated strings, `00000000-0000-0000-0000-00000000000Z`.
- [x] 2.6 `cases::ipv4` covers `127.0.0.1`, `0.0.0.0`, `255.255.255.255`.
  Negative cases: `1.2.3`, `256.0.0.1`, `1.2.3.4.5`.
- [x] 2.7 `cases::ipv6` covers `::1`, `2001:db8::1`,
  `2001:0db8:85a3:0000:0000:8a2e:0370:7334`. Negative cases: invalid
  groups, missing colons, malformed brackets.
- [x] 2.8 `cases::hex_color` covers `#fff`, `#ffff`, `#ffffff`,
  `#ffffffff`. Negative cases: `#ff`, `#fffff`, `#gggggg`,
  `#1234567`.
- [x] 2.9 `cases::html` covers `<html>…</html>`,
  `<div>hello</div>`, `<p>x</p>`, `<a href="…">`, `<script>…</script>`,
  `<style>…</style>`. Negative cases: single `<` or `>` in prose,
  fragments without tags.
- [x] 2.10 `cases::file_path` covers macOS `/Users/foo/bar.txt`,
  `~/Documents`, `/Volumes/…`, Linux `/home/foo`, `/etc/hosts`,
  `/tmp/…`. Negative cases: bare words, prose with `/`, anything that
  is also a URL.
- [x] 2.11 `cases::sql` covers `SELECT … FROM …`,
  `INSERT INTO … VALUES …`, `UPDATE … SET … WHERE …`,
  `CREATE TABLE …`, `DROP TABLE …`, `WITH … AS … SELECT`. Negative
  cases: single `SELECT`, `select` lowercase, `from` alone, prose with
  the word `select`.
- [x] 2.12 `cases::shell_command` covers `#!/bin/bash`, shebang with
  `env`, `ls -la | grep foo`, `cd … && rm -rf …`, `cat file > /tmp/x`,
  `git status`, `npm run build`, `cargo build`. Negative cases: bare
  words, prose with the word `grep`, single `&&` in prose.
- [x] 2.13 `cases::code` covers fenced ` ```python\nprint("hi")\n``` `,
  ` ```rust\nfn main(){}\n``` `, shebang `#!/usr/bin/env python3`.
  Negative cases: single keywords, prose with `<` or `>`.
- [x] 2.14 `cases::text_fallback` covers empty string, whitespace,
  Unicode emoji + text, single letters, ambiguous fragments.
- [x] 2.15 `precedence::jwt_over_url` and `precedence::uuid_over_text`
  pin the order when a payload could match two categories.
- [x] 2.16 `precedence::ip_over_url` covers an IP-shaped URL host:
  detection wins for the IP, not the URL scheme.
- [x] 2.17 `precedence::json_over_html` covers a JSON-looking fragment
  inside HTML; the JSON shape wins because it's structurally
  determinate.
- [x] 2.18 `determinism::repeated_calls_return_same_type` runs each
  case in the table five times and asserts identical output.
- [x] 2.19 `unicode::non_classifiable_unicode_is_text` covers emoji,
  accented characters, RTL marks and CJK text that should not match
  any structural category.

## 3. DB enum and repository changes

- [x] 3.1 Extend `ContentType` with `Url`, `Email`, `Json`, `Jwt`,
  `Uuid`, `Ipv4`, `Ipv6`, `HexColor`, `Html`, `FilePath`,
  `ShellCommand`, `Sql`, `Code`; keep `Text`. All variants serialize
  as snake_case. Implemented at `crates/clipvault-db/src/entry.rs`.
- [x] 3.2 `as_str()` and `Display` for the new variants in
  `crates/clipvault-db/src/entry.rs::tests`.
- [x] 3.3 `parse_content_type` accepts every new variant and returns
  `None` for unknown values. The row mapper rejects unknown values
  via `FromSqlConversionFailure` so the contract from
  `clipboard-text-history` keeps holding. Implemented at
  `crates/clipvault-db/src/entry_repository.rs:303-325`.
- [x] 3.4 `EntryRepository::text_entries` filters by the canonical
  textual-types list instead of `'text'` alone. The list lives in a
  new constant `TEXTUAL_CONTENT_TYPES` so the SQL is generated from
  Rust and the test can assert it. Implemented at
  `crates/clipvault-db/src/entry_repository.rs::TEXTUAL_CONTENT_TYPES`
  and `text_entries`.
- [x] 3.5 DB unit tests covering: serialization of each variant,
  legacy `text` row parses, new variants parse, unknown value
  rejected, `text_entries` includes each textual type and excludes
  image / binary placeholders for future-proofing, deduplication
  preserves the original `content_type` on touch. Implemented at the
  bottom of `crates/clipvault-db/src/entry_repository.rs`.

## 4. Capture pipeline integration

- [x] 4.1 `TextHistoryService::record_payload` calls
  `detect_content_type` after the empty / blacklist checks and uses
  the result as the `content_type` field of `NewEntry`. Implemented
  at `crates/clipvault-core/src/history.rs:118-150`.
- [x] 4.2 Blacklisted payloads never reach the detector: the
  `PrivacyGate` returns before detection runs. Integration test in
  `tests/type_detection.rs` covers this.
- [x] 4.3 Empty / whitespace payloads keep being `HistoryOutcome::Ignored`
  with `content_type = Text` (the default in the DB layer when no
  row is created).
- [x] 4.4 Duplicate detection by hash is untouched: a re-captured
  payload updates `updated_at` / `last_seen_at` and keeps the
  original `content_type`.
- [x] 4.5 `content_type` is never logged: the integration test
  installs a `MakeWriter` capture and asserts the classified label
  is not in the buffered log.

## 5. Search and management integration

- [x] 5.1 `SearchService` already reads `text_entries`; widening the
  query keeps classified rows searchable. New integration test in
  `tests/type_detection.rs::search_finds_classified_entries`
  exercises JSON / URL / code / JWT / IPv4 hits.
- [x] 5.2 `HistoryManagementService::set_favorite` /
  `delete_entry` / `clear_non_favorites` /
  `delete_non_favorites_older_than` operate on `EntryRecord.id`,
  which is independent of `content_type`. Existing tests stay green.
- [x] 5.3 `clipvault_quick_paste_*` already uses `recent_entries`
  with no content-type filter; nothing changes.

## 6. Frontend UI

- [x] 6.1 New `app/tauri/frontend/src/lib/contentType.ts` with
  `contentTypeLabel(value: string): string` returning the localized
  label for every supported variant and `"Texto"` for unknown or
  empty values.
- [x] 6.2 `app/tauri/frontend/src/App.svelte` renders a small badge
  next to the entry snippet in the recent list and in the search
  results. The badge uses the helper output. `Text` and unknown
  values render as `"Texto"` (the safe fallback) per design.
- [x] 6.3 `app/tauri/frontend/src/QuickPaste.svelte` renders the
  same badge next to each row in the results list.
- [x] 6.4 No re-implementation of the detector in TypeScript: the
  helper only maps the backend string to a label.

## 7. Frontend tests

- [x] 7.1 New `app/tauri/frontend/tests/contentType.test.ts` covering
  every supported variant → expected label, unknown → `"Texto"`,
  empty → `"Texto"`, determinism on repeated calls, no payload in
  the label.
- [x] 7.2 Frontend regression: existing tests in `tests/*.test.ts`
  still pass with the badge added.

## 8. Verification

- [x] 8.1 `cargo fmt --all -- --check` clean.
- [x] 8.2 `cargo clippy --workspace --all-targets -- -D warnings`
  clean.
- [x] 8.3 `cargo test --workspace` green; the new
  `tests/type_detection.rs` covers integration scenarios:
  - `capture_persists_detected_type` for each textual type.
  - `blacklisted_capture_never_reaches_detector`.
  - `ambiguous_payload_persists_as_text`.
  - `duplicate_capture_does_not_create_second_row`.
  - `search_finds_classified_entries`.
  - `capture_logs_never_contain_payload_hash_or_content_type`.
- [x] 8.4 `cd app/tauri/frontend && npm run check` and `npm run
  build` clean; `npm test` reports the new content-type tests
  passing alongside the existing suite.
- [x] 8.5 `openspec validate clipboard-type-detection --strict
  --type change` exits with `Change 'clipboard-type-detection' is
  valid`.
