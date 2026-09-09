## Why

`npm test` in `app/tauri/frontend` invokes Node with
`--experimental-strip-types` and runs `tests/**/*.test.ts` directly. That
flag does not exist on Node 20 LTS, the official frontend baseline pinned
by `.nvmrc` and `engines`, so `npm test` fails before any assertion runs.
A previous attempt re-ran the same command under Node 22 (or 23) where
the flag does exist; that path is not acceptable because:

- The project documents Node 20 LTS + npm 10 as the canonical frontend
  toolchain (`.nvmrc`, `engines`, CI `actions/setup-node`, the
  `repository-reproducibility` archived change).
- macOS hosts and Ubuntu hosts both install Node 20 from the same
  declaration; introducing a Node 22 dependency would split the
  toolchain between the two platforms and break the cross-platform
  reproducibility contract.
- The TypeScript files in `tests/` and `src/lib/` are pure ESM,
  TypeScript-only code that compiles cleanly to ES2022 without
  introducing runtime dependencies; we can keep using Node 20 by
  transpiling with the TypeScript compiler the project already
  depends on.

## What Changes

- Replace the `npm test` script with a two-step pipeline that runs on
  Node 20 LTS:
  1. `tsc --noCheck -p tsconfig.test.json` transpiles every test file
     and the `src/**/*.ts` files the tests import into
     `app/tauri/frontend/node_modules/.cache/clipvault-test-build/`.
     This directory is inside `node_modules/`, which the existing
     `.gitignore` already excludes and which `npm ci` recreates, so no
     new tracked artifacts are introduced.
  2. `node --test --test-reporter=spec <out-dir>/tests/` runs the Node
     test runner against the transpiled JavaScript output.
- Add a dedicated `app/tauri/frontend/tsconfig.test.json` that:
  - Extends the existing `tsconfig.json` so Svelte, ESM, strict,
    `verbatimModuleSyntax` and `allowImportingTsExtensions` stay
    consistent.
  - Enables emit (`noEmit: false`, `declaration: false`,
    `sourceMap: false`) with `rootDir: "./"` and
    `outDir: "./node_modules/.cache/clipvault-test-build"`.
  - Sets `rewriteRelativeImportExtensions: true` so the `import ... from
    "../src/lib/foo.ts"` specifiers tests already use are rewritten to
    `"./foo.js"` in the emitted code and resolve correctly under Node
    ESM.
  - Includes `src/**/*.ts` and `tests/**/*.ts` (no `.svelte` files —
    tests only read Svelte source as text, never import the runtime).
- Use `--noCheck` so the test pipeline does not depend on
  `@types/node` (which would be the only way to type-check Node
  built-in modules). The user-facing type contract for `src/` is
  covered by `npm run check` (`svelte-check`) as before; the test
  pipeline keeps Node 20 as a runtime dependency and ships no new
  devDependency.
- Update the source-level test helpers that compute the frontend root
  through `import.meta.url`. With the transpiled output living under
  `node_modules/.cache/`, `import.meta.url` would resolve inside the
  cache and the `loadSource("src/HistoryCard.svelte")` calls would
  miss. The fix is to anchor `FRONTEND_ROOT` (and `REPO_ROOT` /
  `TAURI_ROOT` where they exist) to `process.cwd()`, which is the
  directory `npm test` runs from and equals `app/tauri/frontend/`.
  This matches the convention the majority of the test files already
  follow (`process.cwd()`-based `resolvePath`).
- Add `app/tauri/frontend/.nvmrc` declaring `20` (it was referenced
  by the archived `2026-09-09-repository-reproducibility` change but
  was never committed to the working tree).
- Refresh the `engines` block in `package.json` to keep Node 20 LTS
  and npm 10 as the declared baseline. No new dependencies are added
  to `dependencies` or `devDependencies`.
- Bump the canonical product version from `0.1.1` to `0.1.2` in
  `Cargo.toml`, `app/tauri/src-tauri/tauri.conf.json` and
  `app/tauri/frontend/package.json`. This is the only patch bump
  this change introduces; resuming an interrupted implementation of
  the same fix MUST NOT re-bump the version.

## Non-Goals

- Do not migrate the frontend to pnpm, yarn or bun. The single
  tracked lockfile stays `app/tauri/frontend/package-lock.json`.
- Do not add `@types/node`, `tsx`, `ts-node`, `swc-node` or any other
  runtime devDependency. Node 20 plus the TypeScript compiler the
  project already lists are enough.
- Do not rewrite or re-implement the existing test bodies. The only
  edits to the `tests/` sources are the minimal `import.meta.url`
  → `process.cwd()` swaps that the cache layout forces, plus the
  already-correct `process.cwd()` consumers stay untouched.
- Do not commit transpiled JavaScript. The output directory lives
  inside `node_modules/` and is regenerated on every `npm ci` /
  `npm test` invocation.
- Do not change `npm run check`, `npm run build`, `npm run dev` or
  `npm run preview`. The build pipeline keeps `svelte-check` and
  `vite build` exactly as they are today.
- Do not modify the `drag-and-drop` protected baseline beyond the
  `import.meta.url` → `process.cwd()` swap. The pointer controller,
  ghost, pointer capture, text-selection lock and exclusion of
  interactive controls stay byte-for-byte identical.

## Capabilities affected

### Capabilities modified

- `repository-reproducibility`: the canonical frontend toolchain
  spec now requires `npm test` to run on Node 20 LTS through the
  `tsc` + `node --test` pipeline, not via the Node 22-only
  `--experimental-strip-types` flag.
