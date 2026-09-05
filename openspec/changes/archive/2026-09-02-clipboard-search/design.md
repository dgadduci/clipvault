# Design: clipboard-search

## Context

`clipvault-search` existe como stub que devuelve `note = "not_implemented"` y
`EntryRepository` no expone una operación para alimentar al motor con los
registros textuales. `clipvault-core` no tiene un servicio de búsqueda y
Tauri no expone `clipvault_search_entries`. El frontend sólo lista entradas
recientes.

El spec `clipboard-search` exige búsqueda local, full-text y fuzzy con
ranking determinístico sobre el historial textual. La implementación debe:

- correr exclusivamente contra SQLite/datos locales;
- no bloquear al watcher del portapapeles;
- no mutar entradas, timestamps, favoritos ni el clipboard;
- ser case-insensitive y Unicode-segura;
- acotar el límite (defecto razonable, tope 500);
- devolver cero resultados con `note = "empty_query"` para entradas vacías o
  sólo con espacios.

## Motor de búsqueda (`clipvault-search`)

El motor es GUI-agnóstico y no toca SQLite. Recibe documentos en memoria y
una consulta, y devuelve resultados rankeados de forma determinística.

### Tipos públicos

```rust
pub struct SearchQuery {
    pub text: String,
    pub limit: usize,
}

pub struct SearchHit {
    pub entry_id: i64,
    pub snippet: String,
    pub score: i32,
}

pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    pub note: String, // "ok" | "empty_query"
}

pub trait SearchEngine {
    fn search(&self, query: &SearchQuery, documents: &[SearchDocument])
        -> Result<SearchResults, SearchError>;
}

pub struct SearchDocument<'a> {
    pub entry_id: i64,
    pub content: &'a str,
    pub updated_at: &'a str,
}
```

`SearchDocument.updated_at` se usa sólo para el desempate; el motor lo lee
como string y delega el parseo al caller (no introduce dependencias nuevas).
El orden final se calcula con el `entry_id` como tiebreak explícito.

### Normalización

- Trim de espacios Unicode (`char::is_whitespace`) y colapso de espacios
  internos a un único `' '`.
- Lowercase con `str::to_lowercase` (Unicode-aware).
- Separación en tokens por espacio. Una consulta que queda con cero tokens
  después de normalizar devuelve `note = "empty_query"` sin error y sin
  iterar documentos.

### Ranking por tiers (de mayor a menor)

Para cada documento, el motor asigna un score entero en tiers disjuntos.
Todos los tokens deben "matchear" para que el documento entre al ranking;
los hits parciales no se devuelven (evita falsos positivos irrelevantes).

| Tier | Score base | Condición |
|------|-----------:|-----------|
| 1 | 3000 | La frase completa normalizada aparece como substring del contenido normalizado. |
| 2 | 2000 | Todos los tokens aparecen como substrings del contenido normalizado (no necesariamente contiguos). |
| 3 | 1000 | Todos los tokens emparejan vía fuzzy (Levenshtein ≤ 2 sobre el token normalizado; si el token de la query tiene longitud ≤ 3 se exige coincidencia exacta para evitar falsos positivos). |

Si ningún tier aplica, el documento se descarta. Los tiers son
estrictamente disjuntos y garantizan que un match exacto nunca pierda
contra uno parcial, y que un match por tokens nunca pierda contra uno
fuzzy.

### Desempate

A igualdad de `score`, se ordena por `updated_at` descendente. `updated_at`
se compara como string RFC 3339 (formato producido por la migración
existente); el orden lexicográfico coincide con el orden cronológico para
ese formato. Si persiste la igualdad, se usa `entry_id` descendente. La
estabilidad se valida en tests con datos y consultas fijas.

### Snippets

- Se busca la posición del primer token de la query dentro del contenido
  original (sin normalizar para preservar caracteres visibles).
- Se toman hasta 80 caracteres centrados en esa posición.
- El corte se hace en límites de `char` (no de bytes) para no romper
  caracteres multibyte.
- Si el snippet no empieza al inicio del contenido se antepone "…"; si no
  termina al final se agrega "…".
- El snippet no incluye el contenido completo; el motor nunca lo loguea.

### Límite

`SearchEngine::search` siempre devuelve como mucho `query.limit` hits. El
caller acota el valor antes de llamar al motor; el servicio de Core aplica
el tope 500 con un valor por defecto de 50 (mismo patrón que
`clipvault_recent_entries`).

## Repositorio (`clipvault-db`)

`EntryRepository` gana un método de lectura que devuelve los registros
textuales sin filtrar:

```rust
pub fn text_entries(&self) -> Result<Vec<EntryRecord>, EntryRepositoryError>;
```

- Sólo lee filas con `content_type = 'text'` (el único tipo vigente hoy,
  pero deja claro que la búsqueda textual no debe arrastrar tipos futuros).
- Devuelve los mismos `EntryRecord` que ya consume el resto del core, sin
  introducir tipos duplicados.
- Sin N+1: una sola consulta con `prepare` + `query_map`.

## Servicio de búsqueda (`clipvault-core`)

`clipvault-core/src/search.rs` introduce:

```rust
pub enum SearchServiceError {
    Repository(#[from] EntryRepositoryError),
}

pub struct SearchService;

pub struct SearchEntryHit {
    pub entry_id: i64,
    pub snippet: String,
    pub score: i32,
    pub record: EntryRecord,
}

pub struct SearchServiceOutcome {
    pub hits: Vec<SearchEntryHit>,
    pub note: String,
}

impl SearchService {
    pub fn new() -> Self;
    pub fn search(
        &self,
        context: &AppContext,
        query: &SearchQuery,
    ) -> Result<SearchServiceOutcome, SearchServiceError>;
}
```

`SearchService::search`:

1. Adquiere el `Mutex` de la base una sola vez.
2. Llama a `EntryRepository::text_entries` y obtiene todos los registros
   textuales.
3. Adapta cada `EntryRecord` a un `SearchDocument` con su `id`,
   `content` y `updated_at`.
4. Construye un `LocalSearchEngine` (implementación interna de
   `SearchEngine`) y le pasa los documentos.
5. Combina cada `SearchHit` del motor con el `EntryRecord` original y los
   serializa como `SearchEntryHit`.
6. Devuelve `SearchServiceOutcome` con la misma `note` que el motor
   (`"empty_query"` o `"ok"`).

`AppContext` gana `search: SearchService` y un getter `pub fn search(&self)
-> &SearchService`.

## Tauri (`app/tauri/src-tauri`)

Comando delgado:

```rust
#[tauri::command]
pub fn clipvault_search_entries(
    state: State<'_, SharedState>,
    query: String,
    limit: Option<usize>,
) -> Result<SearchResponse, CommandError>;
```

- Aplica el límite: `limit.unwrap_or(50).clamp(1, 500)`.
- Construye `SearchQuery { text: query, limit }`.
- Llama a `state.context().search().search(...)` y mapea el resultado.
- Devuelve `SearchResponse { note: String, hits: Vec<SearchEntryHitDto> }`.
- Convierte errores de repositorio en `CommandError { kind: "history_error",
  message }`.

El comando se registra en `invoke_handler` en `main.rs`. No agrega estado,
no toca la base fuera del lock del `AppContext`, no contiene lógica de
ranking ni de normalización.

## Frontend (`app/tauri/frontend`)

- `types.ts`: agrega `SearchHit`, `SearchResponse`.
- `lib/tauri.ts`: agrega `searchEntriesCommand({ query, limit? })`.
- `lib/search.ts`: helper puro `runSearch({ query, invoke, debounceMs })`
  que envuelve el comando y aplica un debounce para no saturar IPC
  mientras el usuario escribe. Se cubre con test TS (similar al patrón
  `guidance.test.ts`).
- `App.svelte`: campo de búsqueda con placeholder, lista de resultados que
  muestra snippet y `entry_id`, mensaje de estado para `"empty_query"` y
  para resultados vacíos, mensaje cuando la búsqueda está en curso. No
  realiza pegado desde aquí (queda para `quick-paste`); sólo expone el
  `entry_id` como atributo de dato.

## Tests

- `clipvault-search`:
  - normalización: lowercase, colapso de espacios, sólo-espacios → `empty_query`;
  - tiers: exacto > substring-tokens > fuzzy; fuzzy no aparece si los
    tokens son exactos;
  - ranking determinístico: misma entrada + misma consulta → mismo orden;
  - fuzzy con umbral: token distancia 1 → match; distancia 3 → no match;
  - fuzzy con tokens cortos: query de longitud ≤ 3 exige match exacto;
  - snippet: multibyte seguro, prefijos/sufijos "…" cuando hay truncado,
    límite de 80 chars aprox.
- `clipvault-db`:
  - `text_entries` devuelve sólo filas `text` y respeta el orden natural
    de la tabla;
  - el método es estable a inserciones y no muta nada.
- `clipvault-core`:
  - `SearchService::search` con DB temporal: ranking end-to-end, límite
    aplicado, `empty_query` sin tocar la base;
  - el resultado incluye los campos necesarios para que el frontend lo
    re-use en `quick-paste` (`record.id`, `record.content`, etc.).
- Frontend (TS, `node:test`):
  - `runSearch` debounce: invocaciones rápidas terminan en una sola
    llamada;
  - `runSearch` propaga `note` y `hits` tal cual vienen del backend;
  - `runSearch` cancela el resultado anterior si llega después de una
    nueva consulta (evita resultados viejos pisando a nuevos).

## Privacidad y rendimiento

- El motor y el servicio nunca loguean el contenido textual ni el snippet
  completo. Sólo se loguea `query.length` o `hits.len` si se requiere
  diagnóstico, mediante `tracing::debug!` con campos no sensibles.
- El watcher del portapapeles (`CaptureWatcher`) no comparte lock con
  `SearchService::search`: ambos toman el `Mutex<Database>`, pero como
  son `parking_lot::Mutex` las esperas son bounded. El motor trabaja en
  memoria, sin I/O adicional.
- La búsqueda es 100% local. No se introducen crates nuevos: la única
  dependencia transitiva añadida al workspace es ninguna (no se agrega
  dependencia).
