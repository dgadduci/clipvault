## 1. Workspace and crate scaffolding

- [x] 1.1 Crear `Cargo.toml` raíz con workspace que incluya `clipvault-core`, `clipvault-db`, `clipvault-platform`, `clipvault-search` y la aplicación Tauri.
- [x] 1.2 Crear `rust-toolchain.toml` declarando la toolchain estable y `rustfmt.toml` para formato consistente.
- [x] 1.3 Crear `.gitignore` que ignore `target/`, `node_modules/` y artefactos locales de Tauri.

## 2. clipvault-db: SQLite embebido y migraciones

- [x] 2.1 Implementar `Database::open` que crea `~/.clipvault/`, abre SQLite con `OPEN_READ_WRITE | OPEN_CREATE`, activa WAL y `foreign_keys`.
- [x] 2.2 Definir `Migration` (version, description, up_sql, down_sql) y un runner idempotente que registre versiones aplicadas en `schema_migrations`.
- [x] 2.3 Exponer una migración inicial vacía (placeholder) que reserve la estructura mínima para futuras specs.
- [x] 2.4 Escribir tests de creación de la base, aplicación idempotente de migraciones y persistencia básica.

## 3. clipvault-platform: traits y stubs

- [x] 3.1 Definir el trait `PlatformInfo` y un stub `DefaultPlatform` que devuelva `home_dir`, `data_dir`, `os_family` y `display_server` sin lógica de portapapeles.
- [x] 3.2 Documentar en el módulo que las APIs de clipboard/hotkeys/tray se agregarán en cambios posteriores.

## 4. clipvault-search: stub

- [x] 4.1 Crear el crate `clipvault-search` con un módulo público `engine` que devuelva `SearchResults` vacío y un mensaje `not_implemented` hasta que se implemente `clipboard-search`.

## 5. clipvault-core: AppContext y adaptadores fake

- [x] 5.1 Definir los traits `Clock` y `Clipboard` con implementaciones `SystemClock` y `FakeClipboard` (esta última para tests).
- [x] 5.2 Implementar `AppBootstrap` que arma la base, aplica migraciones y devuelve un `AppContext` con `database`, `clock`, `platform`, `started_at`.
- [x] 5.3 Implementar `DiagnosticsService` que devuelve versión, ruta de DB, cantidad de migraciones y timestamp UTC de arranque.
- [x] 5.4 Escribir tests del core usando `FakeClipboard` y un `Clock` fijo para validar la separación del GUI/clipboard real.

## 6. Shell Tauri 2

- [x] 6.1 Crear `app/tauri/Cargo.toml` con Tauri 2, `clipvault-core`, `clipvault-db` y `tauri-build`.
- [x] 6.2 Implementar `main.rs` con `setup` que invoca `AppBootstrap`, registra estado y expone comandos delgados (`clipvault_diagnostics`, `clipvault_database_path`, `clipvault_migrations_applied`).
- [x] 6.3 Crear `tauri.conf.json` mínimo apuntando al frontend, ventana principal y sin plugins de red.
- [x] 6.4 Crear `build.rs` con `tauri_build::build()`.

## 7. Frontend Svelte + TypeScript

- [x] 7.1 Crear `app/tauri/frontend` con Vite + Svelte + TypeScript usando `npm`.
- [x] 7.2 Implementar `App.svelte` que llame a `clipvault_diagnostics` al montar y muestre versión, ruta de DB, migraciones aplicadas y timestamp.
- [x] 7.3 Configurar `package.json` con scripts `dev`/`build`/`check` y `tsconfig.json` con `checkJs`/`svelte-check`.

## 8. Verificación end-to-end y entrega

- [x] 8.1 Ejecutar `cargo fmt --check`, `cargo check` y los tests de los crates nuevos.
- [x] 8.2 Ejecutar `npm install`, `npm run check` y `npm run build` en el frontend.
- [x] 8.3 Validar OpenSpec con `openspec validate bootstrap-clipvault --strict`.
- [x] 8.4 Verificar que la app arranca en modo desarrollo (sin pantalla por entorno headless) y que `~/.clipvault/clipvault.db` se crea con las migraciones aplicadas.
