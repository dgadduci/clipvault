## Why

ClipVault ya captura texto del portapapeles, lo deduplica por hash y lo
persiste en SQLite. La captura actual sólo almacena `ContentType::Text`
para cualquier payload, así que el historial no distingue entre una URL,
un JSON, un JWT o un comando shell copiado accidentalmente. Esto bloquea
dos capacidades futuras documentadas en `project.md`: acciones
contextuales por tipo y filtros de búsqueda, y obliga al frontend a
adivinar el tipo a partir del contenido en cada render.

Este cambio introduce detección determinística de tipos textuales sin
romper el contrato existente: las filas con `content_type = 'text'`
siguen siendo válidas, el deduplicado por hash no cambia, el quick-paste
y el search siguen mostrando todas las entradas textuales, y la
clasificación se hace en el core sin red ni dependencias nuevas.

## What Changes

- Extender `clipvault_db::ContentType` con variantes para URL, email,
  JSON, JWT, UUID, IPv4, IPv6, hex color, HTML, file_path,
  shell_command, SQL y code; mantener `Text` como fallback estable para
  los datos existentes.
- Crear un detector puro en `clipvault-core` (`detect_content_type` /
  `ContentTypeDetector`) que clasifica el payload textual siguiendo una
  precedencia estable y conservadora; sin Tauri, sin SQLite, sin
  clipboard, sin red ni logs.
- Cablear el detector en el pipeline de captura
  (`TextHistoryService::record_payload`) para que toda nueva entrada
  textual guarde el tipo detectado en `content_type`.
- Ampliar `EntryRepository::text_entries()` para que la búsqueda local
  siga encontrando todos los tipos textuales (no sólo `text`).
- Mantener el deduplicado por hash y los timestamps existentes; un
  duplicado no crea fila nueva.
- Mostrar el `content_type` como etiqueta accesible en historial,
  resultados de búsqueda y quick-paste desde el frontend, con fallback
  seguro a `"Texto"` para valores desconocidos.
- Conservar la regla de privacidad: no se loguea payload, hash, snippet
  ni identificador de la app origen en ningún camino del detector ni de
  la captura.

## Capabilities

### New Capabilities

- `clipboard-type-detection`: detector determinístico, persistencia
  backward-compatible y etiquetas UI para tipos textuales.

### Modified Capabilities

- `clipboard-text-history`: la captura textual clasifica el payload en
  `content_type` antes de persistirlo.
- `clipboard-search`: la búsqueda local opera sobre todos los tipos
  textuales soportados.
- `clipboard-management`: favoritos, borrado, retención y deduplicación
  siguen funcionando sin cambios para todos los tipos textuales.

## Impact

- `crates/clipvault-db`: enum `ContentType` ampliado; serialización
  snake_case estable; `parse_content_type` rechaza valores desconocidos
  con error tipado; `EntryRepository::text_entries` filtra por la lista
  de tipos textuales.
- `crates/clipvault-core`: nuevo módulo `content_type` con detector
  puro; `TextHistoryService::record_payload` consulta el detector y
  guarda el `ContentType` resultante.
- `app/tauri/src-tauri`: comandos Tauri sin cambios (el contrato del
  `EntryRecord` ya exponía `content_type` como string).
- `app/tauri/frontend`: helper que mapea `content_type` a etiqueta
  legible, badge en `App.svelte` (historial reciente + búsqueda) y en
  `QuickPaste.svelte`; valores desconocidos se renderizan como "Texto".
- `tests/`: tabla de casos por tipo en el detector; precedencia;
  fallback; entradas vacías / whitespace / Unicode; malformed JSON;
  JWT / IP / UUID / colores inválidos; falsos positivos de SQL, shell,
  código, rutas y HTML; determinismo en repeticiones; integración con
  el pipeline de captura (persistencia, blacklist, ambigüedad,
  duplicados, búsqueda, no-leak de logs).

No se introducen migraciones nuevas: ampliar los valores de
`content_type` no requiere cambio de esquema y las filas existentes con
`'text'` siguen siendo válidas.
