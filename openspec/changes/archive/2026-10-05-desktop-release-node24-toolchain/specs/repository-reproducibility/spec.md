## REMOVED Requirements

### Requirement: Canonical frontend toolchain and dependency graph (REMOVED 2026-10-05)

This requirement fixed the frontend toolchain to Node.js 20 LTS and npm 10.
Node.js 20 is now end-of-life, and the locked `svelte-awesome-color-picker`
requires Node.js 24 or newer. The replacement contract below keeps one
reproducible frontend toolchain while aligning it with the dependency graph.

## ADDED Requirements

### Requirement: Canonical frontend toolchain and dependency graph on Node.js 24

The frontend SHALL use Node.js 24 LTS with npm 11 as its declared development
baseline, and `app/tauri/frontend/package-lock.json` SHALL remain the single
tracked npm dependency lockfile. The frontend SHALL ship its tests as a
TypeScript transpile + Node test runner pipeline that succeeds on Node.js 24
LTS without depending on experimental type-stripping flags or third-party
TypeScript runners. The pipeline SHALL place compiled tests outside
`node_modules` and SHALL execute the compiled test files instead of silently
accepting an empty test run.

#### Scenario: Clean frontend installation on the declared toolchain

- **WHEN** a developer selects the repository-declared toolchain and runs
  `npm ci` from `app/tauri/frontend`
- **THEN** Node.js reports `v24.x` and npm reports `11.x`
- **AND** `npm ci` consumes the tracked `package-lock.json` without emitting
  `EBADENGINE`
- **AND** no root, `app/` or `app/tauri/` npm/pnpm lockfile is generated

#### Scenario: Frontend verification uses one dependency graph

- **WHEN** macOS, Ubuntu and CI run frontend checks
- **THEN** they use the same tracked lockfile and declared Node.js/npm
  baseline
- **AND** `npm run check`, `npm run build` and `npm test` run from the
  frontend directory without an implicit package-manager migration

#### Scenario: Frontend test pipeline runs on Node.js 24 LTS

- **WHEN** a developer runs `npm test` from `app/tauri/frontend`
- **THEN** the script invokes `tsc --noCheck -p tsconfig.test.json` followed by
  `node --test` with the compiled `*.test.js` files under
  `app/tauri/frontend/test-build/tests/`
- **AND** the script does NOT depend on experimental type-stripping flags or
  any Node.js 22-only flag
- **AND** the compiled tests live outside `node_modules` and the test runner
  reports at least one test executed

#### Scenario: Frontend test assertions follow current localized behavior

- **WHEN** the complete compiled frontend test suite runs on Node.js 24 LTS
- **THEN** every test file is discovered and executed
- **AND** assertions for visible text resolve the current translation keys and
  locale catalogs instead of requiring obsolete hardcoded component strings
- **AND** interaction assertions verify the current handler and delegation
  behavior without weakening the tested contract
- **AND** a capture timestamp in the future uses the localized
  `time.just_captured` label in all supported locales
- **AND** no third-party TypeScript runner (`tsx`, `ts-node`, `swc-node`) or
  `@types/node` is added to `devDependencies`
- **AND** the generated `test-build/` directory remains ignored and is never
  committed
