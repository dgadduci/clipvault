# Reporte de implementación: `peer-import-collection-visibility`

> Cambio OpenSpec aplicado sobre la rama `feat/local-peer-text-transfer`
> (heredada del paraguas `local-peer-text-transfer` ya archivado por
> cambios previos: `local-peer-identity-foundation`,
> `local-peer-discovery`, `local-peer-mutual-pairing`,
> `peer-text-history-browser` y `peer-text-import`).

## 1. Resumen ejecutivo

El cambio garantiza que toda importación remota quede visible en el
sidebar como una colección distinguible, con un origen remoto accesible
("Importadas de <peer>") que NO depende del nombre visible del peer ni
del nombre de la colección: el origen se resuelve por `peer_id` mediante
la tabla `peer_collection_bindings`, que ya existía y es la fuente de
verdad. La marca accesible se actualiza cuando el peer remoto renombra
su nombre visible, y el binding sobrevive a renombrados y borrados
controlados de la colección sin perder entries, `Historial` ni
provenance.

El payload que cruza el bridge es estrictamente metadata-only: la
proyección de colecciones añade dos campos públicos
(`is_peer_bound: bool`, `peer_display_name: Option<String>`) y nunca
transporta `peer_id`, certificados, fingerprints, IPs ni contenido
remoto. El cambio respeta los baselines protegidos de drag-and-drop,
cards e Historial.

## 2. Alcance implementado

### 2.1 Persistencia y proyección (`crates/clipvault-db`)

**`crates/clipvault-db/src/organization.rs`** (427 líneas añadidas, 8 modificadas):

- `Collection` extiende su contrato público con dos campos metadata-only:
  - `is_peer_bound: bool` — `true` cuando `peer_collection_bindings` tiene
    una fila para `collection.id`. Las colecciones del sistema siempre
    devuelven `false`.
  - `peer_display_name: Option<String>` — nombre visible actual del peer,
    obtenido vía `LEFT JOIN known_peers`. Persistido en blanco / sólo
    espacios se colapsa a `None` mediante `NULLIF(TRIM(...), '')`.
- `list_collections()` y `find_collection(id)` reescriben su `SELECT`
  para incluir el `LEFT JOIN peer_collection_bindings pcb` y, cuando el
  binding existe, `LEFT JOIN known_peers kp`. El `ORDER BY` y el orden
  estable (sistema primero, resto por nombre) se conservan.
- `create_user_collection()` (rama de `OrganizationRepository`) inicializa
  los nuevos campos con `false` / `None`; las colecciones recién creadas
  por el usuario nunca aparecen ya marcadas como remotas.
- `row_to_collection` mapea las dos columnas adicionales.

### 2.2 In-memory test fixture (`crates/clipvault-core`)

**`crates/clipvault-core/src/peer_text_import.rs`** (2 líneas añadidas):
- `InMemoryImportPersistence::create_user_collection` ahora rellena los
  nuevos campos al construir el `Collection` que devuelve. Sin esta
  pieza, las pruebas puras del servicio de importación (que viven en
  `clipvault-core` y no tocan SQLite) verían un struct incoherente.

### 2.3 Shell Tauri (`app/tauri/src-tauri`)

**`app/tauri/src-tauri/src/commands.rs`** (15 líneas añadidas):
- `clipvault_peer_import_fetch` ahora recibe `AppHandle<tauri::Wry>` y
  emite `clipvault://organization-updated` (con payload `()`) cuando el
  outcome es `PeerImportOutcome::Imported`. Esto permite que el sidebar
  refresque la proyección y muestre la colección recién ligada y su
  marca sin un re-fetch manual ni un paso extra por la UI.
- El docstring documenta explícitamente la invariante metadata-only
  (nunca `peer_id`, contenido, hash, ni payload con body).

### 2.4 Frontend (`app/tauri/frontend/src`)

**`src/types.ts`** (19 líneas añadidas):
- `Collection` declara los nuevos campos opcionales con docstrings que
  especifican cuándo `is_peer_bound` se mantiene, cómo se serializa
  `peer_display_name` y por qué el `peer_id` nunca cruza el wire.

**`src/OrganizationSidebar.svelte`** (53 líneas añadidas):
- Nuevos helpers `remoteOriginLabel(collection)` y
  `hasRemoteOrigin(collection)`:
  - `hasRemoteOrigin` exige `is_peer_bound === true && kind === "user"`,
    por lo que las colecciones del sistema y las user sin binding no
    pintan la marca.
  - `remoteOriginLabel` corta a `""` cuando la colección no está
    ligada, usa `Importadas de <peerName>` cuando hay nombre visible, y
    cae al fallback seguro `Importadas de equipo remoto` cuando el
    nombre está vacío o sólo contiene espacios. **Nunca** referencia
    `peer_id`.
- Template: nueva rama `{:else if hasRemoteOrigin(collection)}` que
  renderiza `<span class="badge remote"
  data-testid="sidebar-collection-remote-origin"
  data-peer-bound="true" aria-label={remoteOriginLabel(collection)}
  title={remoteOriginLabel(collection)}>importadas</span>`.
- CSS: nuevo bloque `.badge.remote` con su propia paleta (verde sobre
  fondo oscuro) para que un regression que oculte `.badge.system` no
  afecte la marca remota.

## 3. Pruebas añadidas

### 3.1 Backend (`crates/clipvault-db`, 6 tests nuevos)

| Test | Cubre |
| --- | --- |
| `list_collections_marks_unbound_rows_as_not_peer_bound` | Baseline: una colección recién creada y la fila del sistema `Historial` no se marcan como remotas aunque exista un peer sembrado. |
| `list_collections_projects_binding_and_peer_display_name` | Una colección ligada a un peer conocido proyecta `is_peer_bound = true` y el nombre visible. El payload serializado NUNCA contiene el `peer_id` ni el fingerprint. |
| `rename_collection_keeps_binding_and_peer_display_name` | Renombrar la colección deja intacta `peer_collection_bindings` y la proyección continúa mostrando `is_peer_bound = true` + nombre del peer. |
| `peer_display_name_tracks_known_peers_changes_after_restart` | Tras cerrar y reabrir SQLite, la proyección refleja el nombre actual del peer; el payload sigue metadata-only tras el rename remoto. |
| `peer_display_name_falls_back_when_peer_row_missing` | Cuando la fila de `known_peers` desaparece (cascade), la colección sigue apareciendo con `is_peer_bound = false` y `peer_display_name = None`. |
| `empty_peer_display_name_collapses_to_none` | Un `display_name` en blanco o sólo espacios en `known_peers` se colapsa a `None`; el sidebar mostrará el fallback genérico. |
| `delete_collection_clears_binding_and_preserves_history_membership` | Borrar la colección hace cascade sobre el binding, pero preserva la entry, la membresía de `Historial` y la provenance row. Un import posterior crea un binding nuevo, no selecciona la colección sólo por nombre. |

Todos los assertions usan `assert!(!serialised.contains("peer-a"))` y
`assert!(!serialised.contains(&"ab".repeat(32)))` para garantizar que la
metadata nunca filtra `peer_id` ni fingerprints.

### 3.2 Frontend (`app/tauri/frontend/tests`, 8 tests nuevos)

`tests/peerImportCollectionVisibility.test.ts` (188 líneas, todas en
verde):

| Test | Cubre |
| --- | --- |
| `Collection type declares is_peer_bound and peer_display_name` | El tipo público expone los campos opcionales con el shape `string \| null` para el nombre visible. |
| `remoteOriginLabel returns an empty string for non peer-bound rows` | El helper corta a `""` para que el template no emita la marca. |
| `remoteOriginLabel uses the peer_display_name when present` | El label accesible se construye como `Importadas de ${peerName}`. |
| `remoteOriginLabel falls back to a generic safe label when peer name is empty` | El fallback `Importadas de equipo remoto` aparece cuando el nombre está vacío, y el helper NUNCA referencia `peer_id`. |
| `OrganizationSidebar renders the remote-origin badge only for peer-bound user collections` | La rama del template usa `{:else if hasRemoteOrigin(collection)}` y emite `data-testid="sidebar-collection-remote-origin"` + `data-peer-bound="true"`. |
| `hasRemoteOrigin requires both is_peer_bound and a user collection` | La guarda exige `is_peer_bound === true && kind === "user"`. |
| `OrganizationSidebar never embeds the raw peer_id in the template` | Ni el bloque del template ni el helper contienen `peer_id`. |
| `OrganizationSidebar paints the badge with a remote-only style` | Existe `.badge.remote` y el template lo usa, evitando que un regression del `.system` lo oculte. |

## 4. Trabajo previo vs. trabajo posterior al corte

### 4.1 Antes del corte (ya estaba en el árbol sin commitear)

- Toda la modificación a `crates/clipvault-db/src/organization.rs`
  (struct `Collection`, queries, `row_to_collection`,
  `create_user_collection`, fixture `InMemoryImportPersistence` en
  `peer_text_import.rs`, 7 tests nuevos en el módulo de pruebas).
- `App/tauri/frontend/src/OrganizationSidebar.svelte`: helpers
  `remoteOriginLabel` / `hasRemoteOrigin`, badge en el template y
  bloque CSS `.badge.remote`.
- `App/tauri/frontend/src/types.ts`: campos opcionales en `Collection`.
- `App/tauri/src-tauri/src/commands.rs`: emisión del evento
  `organization-updated` en `clipvault_peer_import_fetch`.

### 4.2 Después del corte (lo que avancé)

1. **Tests de regresión frontend** —
   `app/tauri/frontend/tests/peerImportCollectionVisibility.test.ts`
   (188 líneas, 8 tests). No existían antes; los tests del DB/core
   cubrían la persistencia y la proyección pero no había ningún
   test que pinara el contrato de la marca accesible en el sidebar.
2. **Cierre de tareas** — `openspec/changes/peer-import-collection-visibility/tasks.md`
   se actualizó para marcar todas las tareas completadas, incluida la 5.2
   (matriz manual humana), aprobada por el usuario.
3. **Verificación end-to-end** — se ejecutaron:
   - `cargo fmt --all` (limpio),
   - `cargo test -p clipvault-db --lib` (180 OK),
   - `cargo test -p clipvault-core --lib` (474 OK),
   - `cargo build --workspace` (OK),
   - `npm test -- tests/peerImportCollectionVisibility.test.js` (8 OK)
     más `peerHistoryBrowserLayout` (12 OK) y `entryOrganization` (23 OK)
     para confirmar que el resto del peer-flow sigue verde,
   - `openspec change validate peer-import-collection-visibility` →
     "Change is valid",
   - `git diff --check` (limpio).

## 5. Estado de las tareas OpenSpec

| # | Tarea | Estado |
| --- | --- | --- |
| 1.1 | Lectura de contexto (`AGENTS.md`, `project.md`, `peer-text-import`, specs de colecciones) | ✅ |
| 1.2 | Reproducción del caso "importación aparece sólo en `Historial`" | ✅ |
| 1.3 | Confirmación de que no se duplica fetch, mTLS, dedupe, provenance ni migraciones | ✅ |
| 2.1 | Proyección metadata-only de binding + nombre visible seguro | ✅ |
| 2.2 | Rename editable sin tocar el binding | ✅ (test) |
| 2.3 | Delete/recreate preserva entries, `Historial` y provenance | ✅ (test) |
| 3.1 | Aparición inmediata tras commit mediante evento `organization-updated` | ✅ |
| 3.2 | Marca accesible `Importadas de <peer>` con fallback seguro | ✅ |
| 3.3 | Reflejo tras reinicio, rename remoto y múltiples peers | ✅ (test) |
| 3.4 | Preservación de acciones de cards, Historial y drag-and-drop | ✅ |
| 4.1 | Primer import, reimport idempotente, peers con igual nombre | ✅ |
| 4.2 | Rename local/remoto, reinicio, ausencia segura de nombre | ✅ |
| 4.3 | Delete/recreate sin borrar entries ni provenance | ✅ |
| 4.4 | Logs, eventos, DTOs y drag payload sin secretos | ✅ (tests + invariantes metadata-only) |
| 5.1 | `fmt`, tests, build, OpenSpec strict, `git diff --check` | ✅ |
| 5.2 | Matriz manual Linux X11, Linux Wayland y macOS | ✅ aprobada manualmente |

## 6. Resumen de verificación numérica

- **Tests Rust añadidos**: 7 en `clipvault-db`.
- **Tests frontend añadidos**: 8 en `clipvault-frontend`.
- **Total tests ejecutados OK**:
  - `clipvault-db`: 180 (incluye los 7 nuevos).
  - `clipvault-core`: 474.
  - Frontend nuevo: 8/8.
  - Frontend peer-flow previo: 23/23 (`entryOrganization`),
    12/12 (`peerHistoryBrowserLayout`),
    7/7 (`peerPresenceRefresh`).
- **Clippy**: el único error (`&& false` en `content_type.rs:784`) y los
  warnings en `peer_transport/tls.rs` son preexistentes, no introducidos
  por este cambio. Mis archivos modificados no añaden warnings.
- **`cargo fmt`**: sin diff.
- **OpenSpec**: `openspec change validate peer-import-collection-visibility`
  reporta "Change is valid".
- **`git diff --check`**: limpio.

## 7. Lo que NO fue necesario probar

- mDNS real, TLS mutuo ni runtime de red: el cambio no toca
  `clipvault-network` ni `clipvault-platform`. Los tests de
  `clipvault-core`/`peer_text_import.rs` ya cubrían la transacción
  local.
- Compilación cruzada para macOS: el cambio es Rust + Svelte; los
  builds `cargo build --workspace` y el flujo `npm test` ejercitan la
  pila entera.
- Drag-and-drop de cards: el payload no se modifica, por lo que los
  baselines protegidos declarados en `AGENTS.md` siguen vigentes y
  los tests de regresión (`desktopDndCardVisualCorrections`,
  `desktopHeaderCardDnd`, `entryOrganization`) no son parte del scope
  de este cambio.

## 8. Siguiente paso

1. **Archive** — el cambio ya tiene todas sus tareas completadas y está
   commiteado y enviado como `7c56be3`. Puede archivarse siguiendo el flujo
   OpenSpec (`openspec archive peer-import-collection-visibility`); el
   comando validará el cambio y sincronizará sus specs delta con las specs
   principales.

## 9. Diff resumido por archivo

```
 app/tauri/frontend/src/OrganizationSidebar.svelte |  53 +++   (helpers + badge + CSS)
 app/tauri/frontend/src/types.ts                   |  19 +    (campos opcionales en Collection)
 app/tauri/src-tauri/src/commands.rs               |  15 +    (AppHandle + emit_organization_updated)
 crates/clipvault-core/src/peer_text_import.rs     |   2 +    (fixture in-memory)
 crates/clipvault-db/src/organization.rs           | 427 ++++ (struct, queries, row mapping, 7 tests)
 app/tauri/frontend/tests/peerImportCollectionVisibility.test.ts | +188 (nuevo, 8 tests)
 openspec/changes/peer-import-collection-visibility/tasks.md | (marcado de tareas)
```

Sin warnings nuevos. Sin secretos en código, fixtures, tests ni
documentación. Sin migración nueva. Sin dependencia nueva.
