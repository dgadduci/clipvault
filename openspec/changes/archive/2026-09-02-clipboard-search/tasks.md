# Tasks: clipboard-search

## 1. Motor de búsqueda local en `clipvault-search`

- [x] 1.1 Extender `clipvault-search` con `SearchDocument`, `SearchHit` (con
  `score`), `SearchResults.note`, el trait `SearchEngine` actualizado y el
  método de normalización (lowercase Unicode, colapso de espacios, tokens).
- [x] 1.2 Implementar `LocalSearchEngine` con ranking por tiers
  (exacto > substring-todos > fuzzy-acotado) y desempate por `updated_at`
  descendente y `entry_id` descendente, totalmente determinístico.
- [x] 1.3 Implementar fuzzy de tokens con Levenshtein acotado (≤ 2),
  exigiendo coincidencia exacta cuando el token de la query tiene longitud
  ≤ 3 para evitar falsos positivos.
- [x] 1.4 Generar snippets Unicode-safe (corte en límites de `char`, "…"
  cuando hay truncado, ≤ 80 chars visibles).
- [x] 1.5 Devolver `note = "empty_query"` cuando la consulta normalizada
  queda vacía y nunca iterar los documentos en ese caso.
- [x] 1.6 Tests en `clipvault-search`: normalización, tiers, ranking
  estable, fuzzy con umbral, snippet multibyte, `empty_query`.

## 2. Repositorio de entradas buscables en `clipvault-db`

- [x] 2.1 Agregar `EntryRepository::text_entries` que devuelve sólo filas
  con `content_type = 'text'` en una sola consulta (sin N+1).
- [x] 2.2 Cubrir con tests en `clipvault-db` que el método respeta el
  tipo y es estable a inserciones/touch.

## 3. Servicio de búsqueda en `clipvault-core`

- [x] 3.1 Crear `clipvault-core/src/search.rs` con `SearchService`,
  `SearchServiceError`, `SearchEntryHit` y `SearchServiceOutcome`.
- [x] 3.2 `SearchService::search` lee los registros textuales del
  repositorio, construye los documentos, ejecuta el motor y devuelve los
  hits con los metadatos del registro para que el frontend los reuse en
  `quick-paste`.
- [x] 3.3 Registrar `SearchService` en `AppContext` con un getter
  `search()` y conectarlo desde `AppBootstrap::finish`.
- [x] 3.4 Tests de integración con DB temporal: ranking end-to-end,
  `empty_query` sin iterar, límite aplicado.

## 4. Comando Tauri delgado

- [x] 4.1 Agregar `clipvault_search_entries(query, limit?)` en
  `app/tauri/src-tauri/src/commands.rs` que delega al servicio y devuelve
  `SearchResponse { note, hits }`.
- [x] 4.2 Acotar el límite con un default razonable y un máximo de 500,
  replicando el patrón de `clipvault_recent_entries`.
- [x] 4.3 Registrar el comando en el `invoke_handler` de
  `app/tauri/src-tauri/src/main.rs`.

## 5. Frontend Svelte

- [x] 5.1 Agregar tipos `SearchHit` y `SearchResponse` en `types.ts` y el
  wrapper `searchEntriesCommand` en `lib/tauri.ts`.
- [x] 5.2 Crear `lib/search.ts` con `runSearch` (debounce y cancelación
  de resultados viejos) y cubrirlo con tests en `tests/search.test.ts`.
- [x] 5.3 Integrar el campo de búsqueda y la lista de resultados en
  `App.svelte` (snippet, `entry_id` visible, estado vacío, estado de
  carga). Sin pegado: sólo se exponen los datos para el futuro
  `quick-paste`.

## 6. Verificación

- [x] 6.1 Ejecutar `cargo fmt --all -- --check`.
- [x] 6.2 Ejecutar `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 6.3 Ejecutar `cargo test --workspace`.
- [x] 6.4 Ejecutar `npm run check` y `npm run build` en el frontend.
- [x] 6.5 Ejecutar `openspec validate clipboard-search --strict`.
- [x] 6.6 Confirmar que el diff queda acotado a `crates/`,
  `app/tauri/src-tauri/` y los artefactos del cambio; no se introducen
  secretos, telemetría, redes ni dependencias nuevas.
