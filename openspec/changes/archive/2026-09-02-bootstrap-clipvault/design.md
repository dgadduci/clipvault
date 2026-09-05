# Design: bootstrap-clipvault

## Context

`project.md` define la arquitectura objetivo de ClipVault: una aplicación
desktop Tauri 2 con frontend Svelte + TypeScript y un core Rust desacoplado
que comparte lógica entre la GUI y una futura CLI. La persistencia es
SQLite embebido en `~/.clipvault/clipvault.db` con migraciones explícitas,
y todas las integraciones con el sistema operativo viven detrás de
adaptadores testeables.

El spec `desktop-foundation` codifica los cuatro requisitos que la base
debe cumplir: aplicación Tauri funcional, separación del core, base
SQLite con migraciones, y operación local/offline sin telemetría.

El repositorio está vacío: no hay commits, ni `Cargo.toml`, ni
`app/`, ni `crates/`. Este cambio entrega el esqueleto mínimo que
satisface `desktop-foundation` sin salirse de su alcance.

## Goals / Non-Goals

**Goals:**

- Sentar la base estructural de crates y del shell Tauri 2 con frontend
  Svelte + TS.
- Persistir en SQLite embebido bajo `~/.clipvault/clipvault.db` con
  migraciones idempotentes y reversibles.
- Proveer un mecanismo de diagnóstico que verifique de punta a punta
  la conexión frontend ↔ Tauri ↔ core ↔ SQLite.
- Permitir que el core se pruebe en unit tests sin GUI, sin clipboard
  real y sin sesiones X11/Wayland.

**Non-Goals:**

- Implementar captura del portapapeles, historial, búsqueda fuzzy,
  favoritos, expiración, blacklist, system tray, hotkeys globales,
  quick-paste, CLI, snippets, transformaciones, detección de secretos o
  import/export. Esos specs tienen cambios dedicados.
- Empaquetar instaladores `.dmg`, `.AppImage` o `.deb`.
- Sincronización remota, cuentas, telemetría o cualquier integración de
  red.

## Decisions

### Workspace Rust multi-crate

Se adopta un workspace Cargo con crates separados
(`clipvault-core`, `clipvault-db`, `clipvault-platform`, `clipvault-search`).

- **Por qué**: respeta la separación definida en `project.md` y evita que
  el core arrastre dependencias de Tauri, GUI o sistema operativo.
- **Por qué no un solo crate con módulos**: dificulta imponer límites de
  dependencias (p. ej. que `clipvault-core` no use `tauri`) y complica
  la futura división entre `clipvault-cli` y el shell Tauri.

Dependencias nuevas justificadas (todas con alternativas evaluadas):

| Dependencia            | Uso                                             | Alternativa descartada                          |
|------------------------|-------------------------------------------------|--------------------------------------------------|
| `tauri` 2              | Shell desktop mínimo                            | `egui`, `iced`: no soportan webview ni tray aún |
| `rusqlite` con `bundled` | SQLite embebido sin runtime externo           | `sqlx`: requiere async runtime, overkill        |
| `serde`, `serde_json`  | Serializar respuestas Tauri / IPC               | manual: propenso a errores                       |
| `thiserror`            | Tipos de error tipados                          | `anyhow` en core: oculta información            |
| `tracing`              | Diagnóstico local estructurado                  | `log`: sin spans ni niveles jerárquicos         |
| `dirs`                 | Resolver `~/.clipvault/` cross-platform         | hardcodear `$HOME`: rompe sandbox macOS         |
| `time`                 | Timestamps UTC para migraciones y metadatos     | `chrono`: mayor superficie, más lento           |

### SQLite embebido y migraciones

`clipvault-db` expone `Database::open(path)` que:

1. Crea `~/.clipvault/` si no existe.
2. Abre SQLite en modo `OPEN_READ_WRITE | OPEN_CREATE`.
3. Activa `journal_mode = WAL` y `foreign_keys = ON` en una sola conexión
   de arranque.
4. Ejecuta todas las migraciones pendientes dentro de una transacción
   por migración, leyendo de un `Migration` struct con `version`,
   `description`, `up`, `down`.

La tabla `schema_migrations` registra las versiones aplicadas y la
migración inicial sienta únicamente las tablas/índices requeridos por
`desktop-foundation` (placeholder para futuras specs). El resto de las
tablas se agregan en sus respectivos cambios para mantener el alcance.

- **Reversibilidad**: cada migración implementa `down` para permitir
  rollback en tests.
- **Idempotencia**: el runner consulta `schema_migrations` antes de
  aplicar y aborta si la versión ya está registrada.

### Adaptadores y traits

`clipvault-core` define:

- `Clock` (inyectable para tests deterministas).
- `Clipboard` (trait que las plataformas implementan, con un `FakeClipboard`
  en el crate para tests).
- `PlatformInfo` (struct simple con `home_dir`, `data_dir`,
  `os_family`, `display_server`) provisto por `clipvault-platform`.

El `AppBootstrap` (`clipvault-core`) orquesta: arma la base de datos,
verifica la conexión y expone un `AppContext` que el shell Tauri
consume. Las pruebas del core instancian `AppBootstrap` con
`FakeClipboard` y un `Clock` fijo sin tocar el sistema.

### Shell Tauri 2 delgado

`app/tauri/src-tauri/src/main.rs`:

- Construye el `AppContext` en `setup`.
- Registra comandos delgados: `clipvault_diagnostics`,
  `clipvault_database_path`, `clipvault_migrations_applied`.
- No contiene reglas de negocio: cada comando es un wrapper de una o
  dos líneas sobre el core.

`app/tauri/tauri.conf.json` define la ventana principal (ancho/alto
seguros, sin plugins de red) y un `frontendDist` apuntando a
`app/tauri/frontend`.

### Frontend mínimo

`app/tauri/frontend` usa Svelte 5 + Vite + TypeScript. La pantalla
principal llama a `clipvault_diagnostics` al montar y muestra:

- Versión de la app.
- Ruta de la base de datos.
- Cantidad de migraciones aplicadas.
- Timestamp del arranque.

Esto verifica end-to-end que frontend, Tauri, Rust y SQLite están
conectados, satisfaciendo el requisito de diagnóstico.

### Operación estrictamente local

- Sin `tauri-plugin-http`, sin `tauri-plugin-fs-remote`.
- `tracing` inicializa un subscriber que escribe a
  `~/.clipvault/logs/` (rotación futura; por ahora sólo un archivo) y
  nunca se envía a la red.
- `Cargo.toml` y `package.json` no incluyen ningún cliente HTTP,
  paquete de analytics ni SDKs externos.

## Risks / Trade-offs

- **Rust 1.77 es antiguo**: Tauri 2 requiere Rust ≥ 1.77. Funciona, pero
  un eventual bump podría necesitar actualizar `rustup`. → Mitigación:
  declarar `rust-toolchain.toml` con la versión requerida.
- **WAL no se activa explícitamente en MVP**: la spec no lo exige, pero
  mejora concurrencia. → Se activa en `Database::open` porque es trivial
  y mejora la experiencia al usar la base desde el shell.
- **Sin `clipvault-rules`, `clipvault-cli` ni `clipvault-assets`**: los
  crates se introducen en sus respectivos cambios para mantener
  `bootstrap-clipvault` acotado a `desktop-foundation`.
- **Frontend sin store global**: cualquier estado se maneja con
  variables locales; Zustand/Redux se sumarán sólo cuando aparezca
  estado compartido real.

## Migration Plan

No aplica: este cambio es greenfield. La estrategia de rollback para
desarrolladores es simplemente borrar el directorio de trabajo y el
`~/.clipvault/` correspondiente.

## Open Questions

- ¿Se prefiere `tauri-plugin-log` para los logs o un subscriber propio?
  (por ahora se usa uno propio para evitar sumar dependencias hasta que
  la spec de privacidad lo pida).
- ¿Conviene activar `synchronous = FULL` para `rusqlite`? (por ahora
  `NORMAL`, suficiente para SQLite local en desktop).
