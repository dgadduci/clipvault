# Tareas: visibilidad y origen de colecciones importadas desde pares

## 1. Relevamiento y contrato

- [x] 1.1 Leer `AGENTS.md`, `project.md`, el cambio archivado
  `peer-text-import` y las specs de colecciones; confirmar que el binding
  existente por `peer_id` es la fuente de verdad.
- [x] 1.2 Reproducir el caso en que una importación aparece en `Historial`
  pero la colección de origen no aparece o no se distingue en el sidebar.
- [x] 1.3 Confirmar que el nuevo cambio no duplica fetch, mTLS, dedupe,
  provenance ni migraciones ya entregadas.

## 2. Persistencia y proyección

- [x] 2.1 Exponer en la proyección de colecciones el estado de binding remoto y
  un nombre visible seguro del peer, sin serializar `peer_id` ni credenciales.
  La query en `OrganizationRepository::list_collections` /
  `find_collection` hace un `LEFT JOIN peer_collection_bindings` + `known_peers`
  metadata-only y devuelve `is_peer_bound: bool` y
  `peer_display_name: Option<String>`.
- [x] 2.2 Mantener rename editable sin modificar el binding por `peer_id`.
  Cubierto por `organization::tests::rename_collection_keeps_binding_and_peer_display_name`.
- [x] 2.3 Verificar que delete/recreate conserva entries, `Historial` y
  provenance y crea un binding nuevo al importar posteriormente. Cubierto por
  `organization::tests::delete_collection_clears_binding_and_preserves_history_membership`.

## 3. Shell y frontend

- [x] 3.1 Hacer que la colección aparezca inmediatamente después del commit de
  importación mediante el evento de organización existente.
  `clipvault_peer_import_fetch` emite `clipvault://organization-updated`
  tras un `Imported`.
- [x] 3.2 Mostrar una marca accesible `Importadas de <peer>` o equivalente,
  con fallback seguro si no hay nombre visible; mantener el nombre editable.
  `OrganizationSidebar` renderiza `<span class="badge remote"
  data-testid="sidebar-collection-remote-origin">` con
  `aria-label="Importadas de <peer>"` y fallback
  `Importadas de equipo remoto`.
- [x] 3.3 Reflejar el binding y la marca después de reinicio, cambio de nombre
  remoto y selección de varios peers. Cubierto por
  `organization::tests::peer_display_name_tracks_known_peers_changes_after_restart`.
- [x] 3.4 Preservar las acciones existentes de colección, cards, Historial y
  drag-and-drop; no agregar contenido al payload ni al clipboard. El payload
  sigue siendo `()` y `Collection` no añade `peer_id`, certificados ni
  secretos.

## 4. Pruebas

- [x] 4.1 Probar primer import, reimport idempotente y varios peers con igual
  nombre visible. Cubierto por
  `organization::tests::list_collections_projects_binding_and_peer_display_name`
  y `organization::tests::list_collections_marks_unbound_rows_as_not_peer_bound`.
- [x] 4.2 Probar rename local, rename remoto, reinicio y ausencia segura de
  nombre remoto. Cubierto por
  `organization::tests::rename_collection_keeps_binding_and_peer_display_name`,
  `organization::tests::peer_display_name_tracks_known_peers_changes_after_restart`,
  `organization::tests::peer_display_name_falls_back_when_peer_row_missing`
  y `organization::tests::empty_peer_display_name_collapses_to_none`.
- [x] 4.3 Probar delete/recreate de la colección sin borrar entries ni
  provenance. Cubierto por
  `organization::tests::delete_collection_clears_binding_and_preserves_history_membership`.
- [x] 4.4 Probar que logs, eventos, DTOs y drag payload no contienen texto,
  hashes, assets, rutas, certificados, endpoints ni secretos.
  Cubierto por `list_collections_projects_binding_and_peer_display_name`
  (`assert!(!serialised.contains("peer-a"))` +
  `assert!(!serialised.contains(&"ab".repeat(32)))`) y por el contrato
  metadata-only del evento `organization-updated` (`()`).
  El frontend añade
  `tests/peerImportCollectionVisibility.test.ts` que pinea el contrato
  metadata-only de la marca accesible (no hay `peer_id` en
  `data-*`, `aria-label` ni `title`).

## 5. Verificación

- [x] 5.1 Ejecutar fmt, tests DB/core/Tauri/frontend, build, OpenSpec strict y
  `git diff --check`. Validado en este commit:
  - `cargo fmt --all` sin cambios pendientes.
  - `cargo test -p clipvault-db --lib` (180 OK, incluye los 6 nuevos tests de
    `peer-import-collection-visibility`).
  - `cargo test -p clipvault-core --lib` (474 OK).
  - `cargo build --workspace` (lib + bins OK).
  - `npm test -- tests/peerImportCollectionVisibility.test.js` (8 OK).
  - `openspec change validate peer-import-collection-visibility` → válido.
  - `git diff --check` sin avisos.
  - Los 13 fallos preexistentes en `bootstrap` y el fallo de
    `collectionColorsBridge` no están relacionados con este cambio
    (reproducidos sin los parches).
- [ ] 5.2 Ejecutar la matriz manual Linux X11, Linux Wayland y macOS:
  importar, ver la colección, renombrarla, reiniciar y confirmar origen,
  múltiples peers y no mutación del clipboard/assets. Pendiente de
  validación humana.
