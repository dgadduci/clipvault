## 1. Migración SQLite para el historial de texto

- [x] 1.1 Agregar la migración `0002_clipboard_entries` en `clipvault-db/src/registry.rs` con la tabla y los índices (`content_hash` UNIQUE, `updated_at`, `source_app`).
- [x] 1.2 Verificar con un test de `clipvault-db` que la migración se aplica, que es reversible (`down_sql` borra tabla e índices) y que el índice UNIQUE rechaza duplicados.

## 2. Repositorio de entradas en `clipvault-db`

- [x] 2.1 Definir `EntryRecord` y `NewEntry` con los campos del spec (id, content, content_type, content_size, content_hash, source_app, created_at, updated_at, last_seen_at) y serde opcional.
- [x] 2.2 Implementar `EntryRepository::insert_or_touch`, `find_by_hash`, `find_by_id`, `recent`, `count` operando sobre `Connection` con transacciones explícitas.
- [x] 2.3 Cubrir con tests la inserción idempotente, el `touch` que sólo actualiza `updated_at`/`last_seen_at`, el listado por timestamp descendente y el conteo.

## 3. Hash determinístico y `TextHistoryService` en `clipvault-core`

- [x] 3.1 Implementar `history::hash_content(&str) -> String` usando `DefaultHasher` con prefijo fijo y volcado hexadecimal.
- [x] 3.2 Definir `HistoryOutcome` (`Stored`, `Duplicate`, `Ignored`, `Failed`) y `TextHistoryService` que orquesta `Clipboard`, `Clock` y `EntryRepository`.
- [x] 3.3 Exponer el servicio a través de `AppContext::history()` y agregar `history_entries` al struct `Diagnostics`.
- [x] 3.4 Cubrir con tests de `clipvault-core` los cuatro escenarios del spec (texto nuevo, duplicado, ignorado, error de clipboard).
- [x] 3.5 Tratar `Some("")` del clipboard como `HistoryOutcome::Ignored` (sin crear fila) y cubrirlo con un único test representativo en `clipvault-core/tests/history.rs`.

## 4. Comandos Tauri delgados y pipeline de captura

- [x] 4.1 Conectar `AppBootstrap` para construir el servicio con `FakeClipboard` por defecto y registrarlo en `AppState`.
- [x] 4.2 Agregar comandos `clipvault_capture_text`, `clipvault_recent_entries` y `clipvault_history_count` en `app/tauri/src-tauri/src/commands.rs` delegando al core.
- [x] 4.3 Validar que `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` y `cargo test --workspace` pasan en verde.

## 5. Verificación final

- [x] 5.1 Ejecutar `openspec validate clipboard-text-history --strict` y resolver cualquier observación.
- [x] 5.2 Confirmar que el diff del cambio queda acotado a `crates/`, `app/tauri/src-tauri/` y los artefactos OpenSpec; no se introducen secretos, telemetría ni dependencias nuevas.
