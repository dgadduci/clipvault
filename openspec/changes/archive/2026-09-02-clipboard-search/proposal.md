## Why

Hoy la búsqueda en ClipVault existe solo como `StubSearchEngine` y el frontend
no expone ningún campo de búsqueda. El usuario debe abrir la ventana rápida y
recorrer el historial manualmente, igual que en cualquier historial plano sin
filtro.

El MVP v0.1 incluye explícitamente "búsqueda de texto con fuzzy search" como
funcionalidad de primera clase. La especificación `clipboard-search` define que
la búsqueda debe ser local, determinística y usable mientras la captura sigue
activa. Hasta ahora:

- `clipvault-search` sólo devuelve `note = "not_implemented"` y cero
  resultados.
- Tauri no expone ningún comando `clipvault_search_entries`.
- El frontend solo lista entradas recientes.
- No existe una operación de repositorio para alimentar el motor con los
  documentos buscables.

Sin búsqueda local usable, la experiencia cotidiana prometida
("atajo → escribir → Enter → pegar") no se cumple: el usuario no puede filtrar
el historial por contenido.

## What Changes

- Reemplazar `StubSearchEngine` por una implementación local, determinística y
  basada en datos en memoria sobre los registros textuales del repositorio.
- Exponer en `clipvault-db` una operación para obtener todos los registros
  buscables (texto) sin N+1 y dentro de la misma transacción de lectura que
  usa el resto del core.
- Agregar `SearchService` en `clipvault-core` que lea los registros, construya
  documentos para el motor, ejecute el ranking y devuelva los hits con los
  datos que el frontend necesita para renderizar y reusar el resultado en
  `quick-paste`.
- Agregar el comando Tauri `clipvault_search_entries(query, limit)` que
  delega al servicio.
- Agregar al frontend Svelte un campo de búsqueda, una lista de resultados con
  snippet y la posibilidad de reutilizar el `entry_id` desde la respuesta
  (sin pegado aún; sólo la integración de datos).
- Cubrir el motor y el servicio con tests determinísticos (orden estable,
  normalización, fuzzy con umbral, snippets Unicode-safe, consulta vacía,
  límite acotado).

## Capabilities

### New Capabilities

- clipboard-search: búsqueda local, full-text y fuzzy sobre el historial
  textual, con ranking determinístico e integración Core → Tauri → Svelte.

### Modified Capabilities

- clipboard-text-history: `EntryRepository` ahora expone una operación para
  enumerar los registros textuales sin filtrar, manteniendo las invariantes
  de transacción existentes. La tabla `clipboard_entries` no cambia en este
  cambio.
- desktop-foundation: el bootstrap construye un `SearchService` y lo expone
  en `AppContext`; los comandos Tauri delgados siguen siendo el único punto
  de entrada del frontend.

## Impact

- `crates/clipvault-search`: reemplazo del stub por un motor real con
  normalización, ranking por tiers, fuzzy acotado y snippets Unicode-safe.
  Sigue siendo GUI-agnóstico y sin dependencias de Tauri/Svelte/SQLite.
- `crates/clipvault-db`: nueva operación de lectura que devuelve los
  registros textuales para alimentar el motor, sin tocar la tabla ni agregar
  dependencias.
- `crates/clipvault-core`: nuevo módulo `search` con `SearchService`,
  `SearchServiceError` y un tipo de resultado enriquecido (`SearchEntryHit`)
  que combina el hit del motor con los metadatos del registro. `AppContext`
  expone el servicio.
- `app/tauri/src-tauri`: comando delgado `clipvault_search_entries` y su
  respuesta serializable, registrado en el handler.
- `app/tauri/frontend`: tipo TypeScript `SearchResponse`, helper de búsqueda
  con debounce, campo de entrada y lista de resultados en `App.svelte`.

No se introducen redes, telemetría, embeddings, LLMs ni dependencias nuevas.
La búsqueda no modifica el clipboard, los favoritos ni los timestamps; corre
exclusivamente sobre los datos locales.
