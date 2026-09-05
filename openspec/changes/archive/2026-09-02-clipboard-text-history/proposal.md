## Why

ClipVault necesita registrar el historial de texto del portapapeles como base
del MVP. El spec `clipboard-text-history` ya define qué debe capturar el
producto (sólo texto en v0.1, ignorando imágenes u otros formatos sin
cerrar la aplicación), qué metadatos persisten por entrada y cómo se
previenen duplicados consecutivos o repetidos mediante un hash
determinístico.

Hoy `bootstrap-clipvault` dejó lista la base técnica (SQLite embebido,
migraciones, traits, `FakeClipboard`), pero todavía no existe ninguna
tabla, repositorio ni servicio que materialice el historial de texto.
Este cambio implementa exactamente lo descrito en el spec, sin tocar
búsqueda, favoritos, hotkeys, snippets ni reglas, que viven en sus
respectivos specs.

## What Changes

- Agregar la migración `0002_clipboard_entries` con la tabla
  `clipboard_entries` y sus índices para hash, timestamp y source.
- Exponer un repositorio `EntryRepository` en `clipvault-db` con
  operaciones de inserción idempotente por hash, lectura por id,
  listado y conteo. La lógica vive en el crate de persistencia para
  mantener el core libre de SQL.
- Definir en `clipvault-core` el servicio `TextHistoryService` que
  consume `Clipboard` + `Clock`, calcula el hash, delega en el
  repositorio y devuelve un resultado tipado (`Stored`, `Duplicate`,
  `Ignored`, `Failed`) sin propagar pánicos.
- Conectar la pipeline de captura al shell Tauri: el `AppBootstrap`
  arma el servicio y lo expone a través de comandos delgados
  (`clipvault_capture_text`, `clipvault_recent_entries`,
  `clipvault_history_count`).
- Mantener `FakeClipboard` como adaptador primario: el ciclo de
  captura real se conectará en el cambio `desktop-platform-integration`;
  este cambio entrega la pipeline completa con fakes para que el core
  pueda probarse sin GUI.

## Capabilities

### New Capabilities

- `clipboard-text-history`: este cambio entrega la implementación de
  los cuatro requisitos del spec (`Capture text clipboard changes`,
  `Persist clipboard entry metadata`, `Prevent duplicate history
  entries`, `Maintain history across restarts`).

### Modified Capabilities

- (ninguna).

## Impact

- Nueva migración SQLite (`0002`) y nuevos archivos en
  `crates/clipvault-db` y `crates/clipvault-core`.
- Nuevos comandos Tauri delgados: `clipvault_capture_text`,
  `clipvault_recent_entries`, `clipvault_history_count`.
- `AppContext` expone el `TextHistoryService` para que el shell lo
  inyecte en los comandos.
- Sin dependencias nuevas (se reutilizan `rusqlite`, `time`, `sha2` no:
  usamos `DefaultHasher`/`sip` a través de la std para mantener el
  árbol ligero). No se agrega telemetría, red, ni dependencias de
  runtime para el usuario final.
