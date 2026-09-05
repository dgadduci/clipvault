# Design: clipboard-text-history

## Context

`bootstrap-clipvault` dejó lista la base para ClipVault: workspace Rust
multi-crate, `clipvault-db` con SQLite embebido y migraciones
idempotentes, `clipvault-core` con `AppContext`, `Clipboard`, `Clock` y
`FakeClipboard`, y un shell Tauri 2 con frontend Svelte mínimo.
`clipboard-text-history` debe entregar la captura local de texto del
portapapeles, su persistencia en SQLite y la prevención de duplicados
sin tocar búsqueda, favoritos, hotkeys ni reglas.

El spec exige cuatro requisitos:

1. Capturar texto y tolerar fallos del clipboard o formatos no texto.
2. Persistir id, content, content_type `text`, created_at, updated_at,
   source_app (nullable), content_size y content_hash.
3. Evitar duplicados consecutivos o repetidos actualizando el campo
   `updated_at` de la entrada existente en lugar de crear una nueva fila.
4. Mantener el historial tras reinicios.

## Goals / Non-Goals

**Goals:**

- Materializar la tabla `clipboard_entries` y los índices necesarios
  para soportar hash único, búsqueda por timestamp y consultas por
  source.
- Exponer un repositorio pequeño y testeable que el core consume sin
  filtrar SQL al resto de la app.
- Implementar `TextHistoryService` con un resultado tipado que cubra
  los cuatro escenarios del spec (texto nuevo, duplicado, ignorado,
  error).
- Exponer tres comandos Tauri delgados (`clipvault_capture_text`,
  `clipvault_recent_entries`, `clipvault_history_count`) y reflejar el
  conteo en `Diagnostics`.
- Mantener `FakeClipboard` como adaptador por defecto en el shell: el
  cambio `desktop-platform-integration` conectará el backend real;
  este cambio garantiza que la pipeline funciona end-to-end con fakes.

**Non-Goals:**

- Búsqueda, favoritos, hotkeys globales, quick-paste, snippets,
  reglas, transformaciones, detección de secretos, import/export,
  expiración automática y system tray. Esos specs tienen cambios
  dedicados.
- Soporte para imágenes, archivos u otros formatos de clipboard: el
  spec exige que se ignoren en v0.1.
- Detección del nombre de la aplicación origen: el spec admite un
  valor nulo o "unknown"; este cambio captura el campo pero lo deja en
  `None` hasta que llegue la integración de plataforma real.
- Reemplazar `FakeClipboard` en el shell Tauri: queda como adaptador
  inyectado por `AppBootstrap` para mantener `clipvault-core`
  autocontenido y testeable.

## Decisions

### Migración `0002_clipboard_entries`

`crates/clipvault-db/src/registry.rs` agrega la migración
`MIGRATION_0002_CLIPBOARD_ENTRIES` con `up_sql` que crea:

```sql
CREATE TABLE IF NOT EXISTS clipboard_entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    content TEXT NOT NULL,
    content_type TEXT NOT NULL,
    content_size INTEGER NOT NULL,
    content_hash TEXT NOT NULL,
    source_app TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_clipboard_entries_hash
    ON clipboard_entries (content_hash);
CREATE INDEX IF NOT EXISTS idx_clipboard_entries_updated_at
    ON clipboard_entries (updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_clipboard_entries_source_app
    ON clipboard_entries (source_app);
```

`down_sql` elimina los índices y la tabla en orden inverso para que
los tests de reversibilidad sigan pasando.

**Por qué `content_hash` UNIQUE** y no deduplicación "consecutiva":
el spec exige que "texto con un hash ya presente en el historial" no
genere una nueva fila; usar UNIQUE garantiza la invariante a nivel de
base de datos y elimina cualquier duplicación entre sesiones.

**Por qué `content_hash` como TEXT**: un índice UNIQUE sobre `BLOB`
también funcionaría, pero guardar la representación hexadecimal en
TEXT simplifica la depuración con `sqlite3` y los logs sin filtrar
contenido. El hash se calcula en memoria (no se loggea nunca).

### Hash determinístico

Se usa `std::collections::hash_map::DefaultHasher` con una semilla
fija (`"clipvault-history:v1"` como `Hash` inicial) y luego se
serializa el contenido en UTF-8. El resultado se vuelca como `u64`
hexadecimal en minúsculas.

- **Por qué no `sha2`**: introduce una dependencia criptográfica
  innecesaria para un identificador local; el spec habla de "hash
  determinístico", no de un hash criptográfico.
- **Por qué no `blake3`/`xxhash`**: mismo motivo; mantener el árbol
  ligero hasta que aparezca una necesidad concreta.

La función vive en `clipvault_core::history::hash_content` para que
pueda ser reutilizada por futuras reglas sin filtrarse a la base.

### Repositorio en `clipvault-db`

`clipvault-db` agrega:

- `EntryRecord`: struct con los campos del spec, serializable.
- `EntryRepository::new(&mut Connection)`: ofrece `insert_or_touch`,
  `find_by_hash`, `find_by_id`, `recent`, `count`.
- `EntryRepositoryError`: errores tipados que envuelven
  `rusqlite::Error`.

`insert_or_touch` inserta una fila nueva o, si ya existe una con el
mismo `content_hash`, refresca `updated_at` y `last_seen_at` de la fila
existente; toda la operación corre dentro de una sola transacción en
`clipvault-db`, así el core sólo necesita una llamada y no combina dos
operaciones por su cuenta.

**Por qué en `clipvault-db` y no en `clipvault-core`**: el spec exige
que la lógica de persistencia viva detrás de la capa de base; el core
orquesta pero no escribe SQL. Mantener la separación coincide con
`bootstrap-clipvault` y con el spec `desktop-foundation`.

### `TextHistoryService` en `clipvault-core`

Nuevo módulo `clipvault_core::history` con:

```text
TextHistoryService::record_text(
    &self,
    context: &AppContext,
    source_app: Option<&str>,
) -> HistoryOutcome
```

`HistoryOutcome` es un enum `Stored { id }`, `Duplicate { id }`,
`Ignored` (cuando el clipboard no devuelve texto utilizable: `Ok(None)`
o `Ok(Some(""))`) y `Failed { error }`.

El servicio:

1. Lee texto del `Clipboard` adaptador.
2. Si la lectura devuelve `Err`, devuelve `Failed`. Si devuelve
   `Ok(None)` o `Ok(Some(""))`, devuelve `Ignored` (sin abortar el
   ciclo de captura).
3. Calcula `content_hash` con `hash_content` y `content_size` desde
   la longitud del texto.
4. Llama a `EntryRepository::insert_or_touch`, que ya corre la
   inserción o el refresh en una sola transacción, y mapea el
   `EntryOutcome` resultante a `Stored` o `Duplicate`.
5. Devuelve el resultado para que la UI decida qué hacer.

`AppContext` expone `history()` para que el shell Tauri pueda
inyectarlo. El método `DiagnosticsService::snapshot` agrega el
conteo de entradas para verificar visualmente que la captura está
funcionando.

### Comandos Tauri delgados

`app/tauri/src-tauri/src/commands.rs` agrega:

- `clipvault_capture_text(state, source_app: Option<String>) -> Result<CaptureResponse, CommandError>`
- `clipvault_recent_entries(state, limit: Option<usize>) -> Result<Vec<EntryRecord>, CommandError>`
- `clipvault_history_count(state) -> Result<HistoryCount, CommandError>`

`CaptureResponse` serializa el `HistoryOutcome` como
`{ kind: "stored" | "duplicate" | "ignored" | "failed", id?: i64,
message?: string }` para que el frontend pueda reflejarlo sin
acceder a SQLite. El límite por defecto es 50 entradas; el shell lo
valida para mantener el contrato.

**Por qué tres comandos y no uno solo**: el spec exige que la captura
sea idempotente y tolerante a fallos. Separar `capture_text` (acción)
de `recent_entries`/`history_count` (consultas) mantiene la pipeline
limpia y permite que el frontend refresque el historial bajo demanda.

### Diagnóstico enriquecido

`Diagnostics` agrega `history_entries` para reflejar cuántas entradas
se persistieron. Esto convierte al comando `clipvault_diagnostics` en
una verificación end-to-end del cambio: si la captura funciona, el
campo aumenta.

## Risks / Trade-offs

- **`DefaultHasher` no es estable entre versiones de Rust**: si en el
  futuro cambia el algoritmo, los hashes previos dejarían de
  coincidir y aparecerían "duplicados falsos". → Mitigación: usamos
  `Hasher::write_u64` con un prefijo fijo (`clipvault-history:v1`); si
  el cambio ocurre, detectaremos el bump al ejecutar los tests de
  regresión y migraremos al algoritmo `sip` correspondiente.
- **UNIQUE sobre hash puede impedir dos entradas idénticas legítimas
  si el usuario quiere guardar dos copias intencionalmente**: el spec
  explícitamente pide evitar esta duplicación, así que la decisión
  coincide con el comportamiento deseado.
- **`source_app` queda `None` hasta el cambio de plataforma**: el spec
  admite "null or explicit unknown"; el servicio acepta el valor para
  que la integración futura no requiera cambios estructurales.

## Migration Plan

- Aplicar la migración 0002 sobre la base existente; los registros
  previos (si los hubiera) no se ven afectados porque la tabla es
  nueva.
- Mantener reversibilidad: el runner ya soporta `down_sql`, y los
  tests de `clipvault-db` cubren el rollback de la nueva migración.
- Si fuera necesario en el futuro rehacer la captura tras un cambio
  de algoritmo de hash, una migración nueva regenerará el campo
  `content_hash`. No está contemplado en este cambio.

## Open Questions

- ¿Conviene exponer `content_hash` al frontend? (por ahora no, porque
  el spec sólo exige que el core lo use para deduplicar).
- ¿Se prefiere `last_seen_at` o sólo `updated_at`? El spec menciona
  "latest-seen metadata"; ambos campos existen por separado para que
  reglas futuras puedan distinguirlos.
