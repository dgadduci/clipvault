## Why

ClipVault necesita una base desktop local, modular y verificable antes de
incorporar captura de portapapeles, búsqueda, favoritos o cualquier otra
funcionalidad. El spec `desktop-foundation` ya define los requisitos
arquitectónicos (Tauri 2 + Svelte/TS + core Rust desacoplado, SQLite embebido
con migraciones, operación offline y privacidad local), pero todavía no existe
el repositorio de código ni la estructura modular descrita en `project.md`.
Este cambio entrega esa base mínima, dejando el resto de los specs
(`clipboard-text-history`, `clipboard-search`, `clipboard-management`,
`quick-paste`, `desktop-platform-integration`, `privacy-settings`) listos
para iterar encima sin tener que reorganizar el código más adelante.

## What Changes

- Crear un workspace Rust con crates `clipvault-core`, `clipvault-db`,
  `clipvault-platform` y `clipvault-search`, alineados con la estructura
  definida en `project.md`.
- Agregar una aplicación Tauri 2 en `app/tauri` que monta un frontend
  Svelte + TypeScript y expone comandos delgados que delegan en el core.
- Implementar la capa de persistencia SQLite embebida en `~/.clipvault/`
  con migraciones explícitas, idempotentes y reversibles aplicadas de forma
  transaccional antes de exponer la base al resto de la app.
- Definir traits y adaptadores fake en el core (`clipboard`, `platform`,
  `clock`) para que las pruebas unitarias no necesiten GUI ni clipboard
  real.
- Incorporar un comando o mecanismo de diagnóstico que verifique de un
  solo paso la conexión frontend ↔ Tauri ↔ Rust ↔ SQLite.
- Dejar stubs explícitos (`todo`) en las áreas que aún no se implementan
  (captura, búsqueda, favoritos, hotkeys, tray, etc.) para evitar que
  código prematuro contamine la base.

## Capabilities

### New Capabilities

- (ninguna nueva: el spec `desktop-foundation` ya existe y este cambio sólo
  entrega su implementación base).

### Modified Capabilities

- `desktop-foundation`: este cambio materializa la implementación de los
  cuatro requisitos ya definidos (aplicación Tauri funcional, separación
  del core, SQLite embebido con migraciones, operación local y offline).
  No se agregan ni se quitan requisitos; la delta confirma que la
  implementación satisface lo ya escrito.

## Impact

- Nuevo árbol de código bajo `crates/` y `app/`.
- Se introduce SQLite (vía `rusqlite` con `bundled`) y `tauri 2`,
  `serde`, `serde_json`, `thiserror`, `tracing`, `dirs`, `time` como
  dependencias mínimas justificadas; todas se documentan en
  `design.md`.
- Se crea el directorio `~/.clipvault/` en el primer arranque de la app.
- No se introducen servicios externos, telemetría, ni nuevas
  dependencias de runtime obligatorias para el usuario final.
