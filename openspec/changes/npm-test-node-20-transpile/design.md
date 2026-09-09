# Design: npm-test-node-20-transpile

## Context

The frontend declared Node 20 LTS + npm 10 as the canonical toolchain
in the archived `2026-09-09-repository-reproducibility` change.
`.nvmrc` (committed as `20`), `engines` (`node >=20 <21`, `npm >=10 <11`)
and the GitHub Actions workflow (`actions/setup-node` with
`node-version-file: app/tauri/frontend/.nvmrc`) all converge on that
baseline. The pre-change `npm test` script, however, was:

```text
"test": "node --experimental-strip-types --no-warnings --test tests/**/*.test.ts"
```

`--experimental-strip-types` only exists in Node 22.6+ (behind a flag)
and Node 23 (behind a flag). On Node 20 the flag is rejected and the
script aborts before any assertion runs. The CI runner pins Node 20
through `actions/setup-node@v4` + `node-version-file: .nvmrc`, so the
green run reported by the archived change was an artefact of the
implementer's host running Node 22 — not a Node 20 reproducible
behaviour.

## Goals / Non-Goals

**Goals**

- Make `npm test` run on Node 20 LTS with no new devDependency.
- Keep the existing test bodies, the polyfill, the Svelte source
  files and the protected drag-and-drop baseline byte-for-byte
  equivalent (the only test edits are the `import.meta.url` →
  `process.cwd()` swap forced by the cache layout).
- Keep `npm ci` deterministic and avoid generating files inside the
  source tree (the transpiled output lives under `node_modules/`).
- Keep the change cross-platform identical: macOS and Ubuntu use the
  same script, the same Node baseline and the same TypeScript
  invocation.
- Honour the `projects.md` versioning policy: bump the patch from
  `0.1.1` to `0.1.2` exactly once for this change and never again if
  the implementation is resumed.

**Non-Goals**

- Re-introduce Node 22 as a baseline.
- Add `tsx`, `ts-node`, `swc-node`, `@types/node` or any other
  TypeScript runner / type stub.
- Replace the test runner with a different framework.
- Reorganise the test layout or rewrite the protected drag-and-drop
  helpers.
- Touch the Rust toolchain, the Cargo manifests or the Tauri
  configuration beyond the patch-bump they already receive.

## Decisions

### Transpile with the TypeScript compiler already in `devDependencies`

The project lists `typescript: "^5.5.0"` in `devDependencies`. The
test runner we target (`node --test`) is a built-in Node 20 feature
that consumes JavaScript. The smallest possible bridge between the
two is to invoke `tsc` with a dedicated tsconfig that emits ESM
JavaScript into a temporary directory, then run the test runner
against the emitted output.

This option was preferred over:

- **`tsx` / `ts-node`**: would add a devDependency for a single
  script invocation and would still have to wrap `node --test`. The
  user explicitly asked to avoid unnecessary devDependencies.
- **A custom Node loader hook**: more invasive than `tsc`, harder to
  debug, and would still need TypeScript to do the transpilation.
- **Hand-rolled `.js` sources next to `.ts` sources**: duplicates the
  source tree and breaks the single-source-of-truth contract used by
  every other file in the project.

`tsc -p tsconfig.test.json` already in `node_modules/.bin/tsc` is the
smallest piece of glue that satisfies the goal.

### Dedicated `tsconfig.test.json`

The existing `tsconfig.json` sets `noEmit: true` (it is consumed by
`svelte-check` for type checking only). For the test build we need
emission, so the change adds a focused `tsconfig.test.json` that:

- Extends `tsconfig.json` so `target: ES2022`, `module: ESNext`,
  `verbatimModuleSyntax: true`, `allowImportingTsExtensions: true`,
  `strict: true` and the Svelte module conventions are inherited
  unchanged.
- Overrides the relevant options:
  - `noEmit: false`, `declaration: false`, `sourceMap: false`
    (tests run from JS, no type information or sourcemaps required).
  - `rewriteRelativeImportExtensions: true` so an `import ... from
    "../src/lib/foo.ts"` becomes `import ... from
    "../src/lib/foo.js"` in the emitted code. Without this flag Node
    ESM cannot resolve the rewritten imports.
  - `rootDir: "./"` and
    `outDir: "./node_modules/.cache/clipvault-test-build"` so the
    transpiled output mirrors the source layout (`src/`, `tests/`)
    inside a cache directory that the existing `.gitignore`
    (`**/node_modules`) already excludes.
  - `include` covers `src/**/*.ts` and `tests/**/*.ts`; Svelte
    components are intentionally excluded because tests read Svelte
    sources as plain text rather than importing them.

### `--noCheck` instead of `@types/node`

Tests import `node:test`, `node:assert/strict`, `node:fs`,
`node:path`, `node:url` and rely on the `process` global. Without
`@types/node` (which is not currently in `devDependencies`),
`tsc`'s type checker rejects every test file before emission.
Two paths exist:

1. Add `@types/node` as a devDependency and rely on real type
   checking for the test suite.
2. Skip type checking for the test pipeline with `tsc --noCheck`.

Path 2 was chosen because:

- The user-facing type contract for the application source is
  already covered by `npm run check` (`svelte-check`); adding
  `@types/node` would only add types for the test infrastructure.
- It avoids a new tracked dependency for a pipeline that already
  runs through `npm run check` for any meaningful coverage.
- `tsc --noCheck` still emits parse errors and emit errors, so a
  syntactically broken test cannot silently ship.

A future change may revisit path 1 if a regression requires test
type-checking to surface before emission; this is intentionally
left out of scope here.

### Anchor source-level reads to `process.cwd()`

Several tests use the pattern:

```ts
const FRONTEND_ROOT = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
```

After transpilation, `import.meta.url` points at
`node_modules/.cache/clipvault-test-build/tests/foo.test.js`, and
the helper that follows it (`loadSource("src/HistoryCard.svelte")`)
tries to open a `.svelte` file inside the cache directory — which
does not exist. Two options were considered:

1. Mirror the source tree inside the cache (copy `*.svelte`, copy
   the `src/` and `tests/` non-TS files alongside the emitted JS).
   This avoids touching any test but bloats the cache and complicates
   `tsc`.
2. Anchor `FRONTEND_ROOT` (and `REPO_ROOT` / `TAURI_ROOT` where they
   exist) to `process.cwd()`. `npm test` always runs from
   `app/tauri/frontend/`, so `process.cwd()` is stable and matches
   the convention the majority of tests already use (`process.cwd()`-
   based `resolvePath(...)`).

Option 2 was chosen: it is a one-line, behaviour-preserving swap in
the affected files (17 files, all using the same template) and keeps
the cache directory strictly JavaScript. The drag-and-drop tests and
the protected pointer controller helpers stay byte-for-byte identical
beyond the import + constant swap; the test bodies, the polyfill and
every assertion stay untouched.

### `engines` block and `.nvmrc`

`package.json` gains the explicit `engines` block:

```json
"engines": {
  "node": ">=20.0.0 <21.0.0",
  "npm": ">=10.0.0 <11.0.0"
}
```

`app/tauri/frontend/.nvmrc` is created with `20`. The archived
`2026-09-09-repository-reproducibility` change referenced this file
but the working tree did not have it; without it CI and contributors
cannot pin Node 20 from `.nvmrc`. Both declarations match the values
the archive documented.

### Canonical version bump: `0.1.1` → `0.1.2`

`projects.md` requires the canonical product version to live in:

- `Cargo.toml` (`[workspace.package].version`)
- `app/tauri/src-tauri/tauri.conf.json` (`version`)
- `app/tauri/frontend/package.json` (`version`)

The archived `2026-09-09-repository-reproducibility` change bumped
from `0.1.0` to `0.1.1` for the `linux-source-app-metadata` repair.
This change bumps to `0.1.2` and the `tasks.md` records the audit
trail. Resuming an interrupted implementation of the same fix MUST
NOT re-bump the patch — the canonical version is the one documented
in `projects.md`, not a local guess.

## Risks and mitigations

- **Risk**: an existing test that imports a Node built-in would now
  fail to type-check and silently emit broken JS.
  **Mitigation**: `tsc --noCheck` still surfaces parse and emit
  errors; every test file is exercised by `npm test` immediately
  after emission, so the regression window is one `npm test`
  invocation.

- **Risk**: contributors could re-introduce `--experimental-strip-types`
  in the future when running Node 22 on their machine.
  **Mitigation**: the spec `repository-reproducibility` is updated
  to pin the test pipeline. The OpenSpec change captures the
  decision so future contributors cannot silently regress it.

- **Risk**: the transpiled cache directory could be committed by
  accident.
  **Mitigation**: the directory lives under `node_modules/.cache/`,
  which is matched by the existing `**/node_modules` rule in
  `.gitignore`. `git status` after `npm test` shows no new entries.

- **Risk**: macOS and Ubuntu paths diverge because of case
  sensitivity, path separators or shell behaviour.
  **Mitigation**: the cache lives inside `node_modules/`, which is
  the same on both platforms. The `npm test` script uses POSIX path
  joins (`path.join` inside the tests, `/` in the shell glob) so the
  behaviour is identical.
