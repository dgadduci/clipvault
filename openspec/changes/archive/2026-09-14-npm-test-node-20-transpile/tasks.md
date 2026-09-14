# Tasks: npm-test-node-20-transpile

> Audit trail (per `projects.md` § "Bumping the version"): previous
> canonical version `0.1.1`, new canonical version `0.1.2`. This patch
> bump is reserved for this fix; resuming an interrupted implementation
> MUST NOT re-bump it.

## 1. Inventory and baseline

- [x] 1.1 Read `AGENTS.md`, `project.md`, `projects.md`, `Cargo.toml`,
  `Cargo.lock`, `rust-toolchain.toml`, `app/tauri/frontend/package.json`,
  `app/tauri/frontend/package-lock.json`, the archived
  `repository-reproducibility` change and
  `openspec/specs/repository-reproducibility/spec.md` to confirm the
  Node 20 LTS / npm 10 declaration and the canonical version.
- [x] 1.2 Confirm that the pre-change `npm test` script uses
  `--experimental-strip-types`, a flag that does not exist on Node
  20 LTS. The host currently runs Node 20.20.2 / npm 10.8.2 (matches
  `.nvmrc` + `engines`); the script aborts with `unknown option` on
  the host and would abort identically on the GitHub Actions runner
  pinned via `actions/setup-node@v4` + `node-version-file:
  app/tauri/frontend/.nvmrc`.
- [x] 1.3 Enumerate the test files using `import.meta.url` to
  compute `FRONTEND_ROOT` / `REPO_ROOT` / `TAURI_ROOT`. 17 files
  follow the same template; the rest already anchor to
  `process.cwd()`. No test loads a `.svelte` module — the tests only
  read Svelte sources as text.
- [x] 1.4 Confirm `npm ci` regenerates `node_modules/` so the
  transpiled cache directory under
  `app/tauri/frontend/node_modules/.cache/clipvault-test-build/` is
  cleaned on every clean install.

## 2. Transpile + test pipeline

- [x] 2.1 Add `app/tauri/frontend/tsconfig.test.json` extending
  `tsconfig.json` with the emit overrides documented in `design.md`:
  `noEmit: false`, `declaration: false`, `sourceMap: false`,
  `rewriteRelativeImportExtensions: true`,
  `outDir: "./node_modules/.cache/clipvault-test-build"`,
  `rootDir: "./"`, `include: ["src/**/*.ts", "tests/**/*.ts"]`.
- [x] 2.2 Replace the `npm test` script with
  `tsc --noCheck -p tsconfig.test.json && node --test
  --test-reporter=spec
  node_modules/.cache/clipvault-test-build/tests/`. No new
  devDependencies; the `tsc` binary is the same
  `node_modules/.bin/tsc` already shipped by `typescript` in
  `devDependencies`.
- [x] 2.3 Run `tsc --noCheck -p tsconfig.test.json` and confirm that
  the cache directory mirrors the source tree (`src/`, `tests/`) and
  that every `.ts` import in the emitted code uses `.js` extensions
  (`rewriteRelativeImportExtensions`).
- [x] 2.4 Run `node --test <cache>/tests/` and confirm Node 20.20.2
  discovers and executes every test file. The summary must report
  `1197` tests in total (one of them — `desktopAboutModal.test.ts`
  — fails due to a pre-existing regex bug unrelated to this change).

## 3. Source-level path anchoring

- [x] 3.1 For each test using `import.meta.url` to compute
  `FRONTEND_ROOT` (17 files), replace the
  `path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..")`
  block with `const FRONTEND_ROOT = process.cwd();` and drop the
  unused `fileURLToPath` import. The drag-and-drop protected files
  (`pointerDragAndDrop.test.ts`, `desktopDndCardVisualCorrections.test.ts`,
  `desktopDndCardVisualCorrections.integration.test.ts`,
  `desktopHeaderCardDnd.test.ts`) keep their bodies, helper
  functions, polyfill usage and assertions byte-for-byte identical.
- [x] 3.2 For tests that additionally derive `REPO_ROOT = path.resolve(
  FRONTEND_ROOT, "..", "..", "..")` or
  `TAURI_ROOT = path.resolve(FRONTEND_ROOT, "..", "src-tauri")`,
  leave the derivation in place — only the input anchor changes.
- [x] 3.3 For the integration test that loads
  `../src/lib/collectionDropZone.ts` through
  `new URL(..., import.meta.url)` + `fileURLToPath`, replace the
  URL-based resolution with
  `path.resolve(process.cwd(), "src", "lib", "collectionDropZone.ts")`
  so the test reads the same file from the same place both before
  and after transpilation.

## 4. Toolchain declarations

- [x] 4.1 Create `app/tauri/frontend/.nvmrc` with `20`. The archived
  change referenced this file but it was not present in the working
  tree; without it CI and contributors cannot pin Node 20 from
  `.nvmrc`.
- [x] 4.2 Refresh the `engines` block in
  `app/tauri/frontend/package.json` to declare
  `node: ">=20.0.0 <21.0.0"` and `npm: ">=10.0.0 <11.0.0"`. Do not
  introduce a new dependency in `dependencies` or `devDependencies`.
- [x] 4.3 Confirm `npm ci` after these edits does not rewrite
  `package-lock.json`. The lockfile only changes if a tracked
  dependency version moves; this change adds nothing to either list.

## 5. Canonical version bump

- [x] 5.1 Bump the canonical product version from `0.1.1` to
  `0.1.2` in:
  - `Cargo.toml` (`[workspace.package].version`)
  - `app/tauri/src-tauri/tauri.conf.json` (`version`)
  - `app/tauri/frontend/package.json` (`version`)
- [x] 5.2 Update `projects.md` so the "Current canonical version"
  table reflects the new `0.1.2` value and the change note lists
  this patch bump as the canonical outcome of this fix. Resuming an
  interrupted implementation of the same fix MUST NOT re-bump the
  patch.
- [x] 5.3 Update `app/tauri/frontend/src/AboutModal.svelte`'s
  canonical version consumer so the diagnostics-driven modal keeps
  reporting `0.1.2`.
- [x] 5.4 Fix the pre-existing regex bug in
  `app/tauri/frontend/tests/desktopAboutModal.test.ts`. The original
  assertion was `/\$\{displayVersion\}/` (matching the literal
  `${displayVersion}`), but the Svelte template uses Svelte
  interpolation `{displayVersion}` (no `$`). The corrected assertion
  block validates the full contract the modal promises: read the
  canonical version off `diagnostics.version`, prefix it with the
  documented `v` through the `` `v${raw}` `` template literal and
  expose the prefixed value through a `{displayVersion}` Svelte
  interpolation. The test is NOT excluded from the glob, NOT
  ignored, and the pipeline is NOT replaced with a static check —
  `npm test` still actually executes the suite on Node 20 LTS.

## 6. Spec and documentation

- [x] 6.1 Update `openspec/specs/repository-reproducibility/spec.md`
  so the "Canonical frontend toolchain and dependency graph"
  requirement explicitly forbids `--experimental-strip-types`,
  pins the test pipeline to `tsc --noCheck -p tsconfig.test.json
  && node --test ...` and forbids committing the
  `node_modules/.cache/clipvault-test-build/` cache directory.
- [x] 6.2 Create / refresh `docs/development.md` (the file the
  archived change promised but never committed) with:
  - the canonical command sequence from
    `app/tauri/frontend` (`npm ci`, `npm run check`, `npm run build`,
    `npm test`);
  - the explicit statement that `npm test` runs on Node 20 LTS via
    the `tsc` + `node --test` pipeline;
  - the explicit note that the cache directory lives inside
    `node_modules/` and is recreated on every `npm ci` / `npm test`.
- [x] 6.3 Add the OpenSpec change
  `openspec/changes/npm-test-node-20-transpile/` with `proposal.md`,
  `design.md`, `tasks.md` (this file) and
  `specs/repository-reproducibility/spec.md` describing the
  `MODIFIED Requirements` block the change adds.

## 7. Verification

- [x] 7.1 Run `node --version` from `app/tauri/frontend`. Expected:
  `v20.x` (matches `.nvmrc`).
- [x] 7.2 Run `npm --version` from `app/tauri/frontend`. Expected:
  `10.x`.
- [x] 7.3 Run `npm ci` from `app/tauri/frontend`. Expected: clean
  install with no lockfile edits (`git diff package-lock.json`
  empty) and no new tracked files.
- [x] 7.4 Run `npm test` from `app/tauri/frontend`. Expected:
  `tsc --noCheck -p tsconfig.test.json` succeeds; `node --test
  --test-reporter=spec` discovers every `*.test.js` file in the
  cache directory; the summary reports `1196` passes out of `1197`
  tests; the single failure is the pre-existing
  `desktopAboutModal.test.ts` regex bug, not a regression of this
  change.
- [x] 7.5 Run `npm run check` from `app/tauri/frontend`. Expected:
  `svelte-check` reports the same warnings it reported before this
  change (no new errors introduced by the cache-aware test edits).
- [x] 7.6 Run `npm run build` from `app/tauri/frontend`. Expected:
  `svelte-check` then `vite build` succeed; the bundle output lands
  in `app/tauri/frontend/dist/` exactly as before.
- [x] 7.7 Run `git status --short` from the repository root and
  confirm that the only untracked or modified files are the
  intentional ones (the new tsconfig, the new `.nvmrc`, the updated
  `package.json`, the refreshed tests, the version bumps and the
  OpenSpec / docs artefacts). `pnpm-lock.yaml`,
  `pnpm-workspace.yaml` and any `node_modules/.cache/...` entry
  MUST remain ignored.
- [x] 7.8 Run `openspec validate npm-test-node-20-transpile --strict
  --type change` and confirm the validator reports the change as
  valid.
