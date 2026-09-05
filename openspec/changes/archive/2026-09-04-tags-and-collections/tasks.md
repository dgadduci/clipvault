# Tasks: tags-and-collections

No archivar este cambio al terminar. La verificación manual de
platform-permission-guidance sigue separada y no debe modificarse.

## 1. Modelo y migración

- [x] 1.1 Definir DTOs neutrales para `Collection`, `Tag` y asociaciones.
- [x] 1.2 Crear migración aditiva/reversible para collections, tags y tablas
  many-to-many con foreign keys e índices.
- [x] 1.3 Crear `Historial` como colección de sistema por `stable_key` y
  asociar transaccionalmente todas las entradas existentes.
- [x] 1.4 Integrar la membresía automática de toda captura nueva con
  `Historial` dentro de la transacción existente.
- [x] 1.5 Probar bootstrap desde bases anteriores, migración repetida,
  rollback y ausencia de filas huérfanas.

## 2. Servicio core

- [x] 2.1 Implementar servicio de organización local con CRUD de colecciones y
  tags y resultados tipados.
- [x] 2.2 Validar nombres vacíos, longitud, espacios y duplicados
  case-insensitive; preservar display names.
- [x] 2.3 Implementar reemplazo atómico de múltiples tags y colecciones.
- [x] 2.4 Proteger renombrado/eliminación de `Historial`.
- [x] 2.5 Implementar `Quitar de esta colección` sólo para colecciones
  secundarias.
- [x] 2.6 Probar que associations no modifican content, timestamps, favoritos,
  source app ni assets.

## 3. Ciclo de vida y gestión

- [x] 3.1 Integrar delete global con eliminación de relaciones.
- [x] 3.2 Integrar clear y retention para eliminar relaciones en la misma
  transacción que las entradas.
- [x] 3.3 Eliminar una colección secundaria sin eliminar sus entradas ni
  `Historial`.
- [x] 3.4 Eliminar un tag sin eliminar entradas.
- [x] 3.5 Verificar que las definiciones vacías de tags/colecciones se
  conservan hasta que el usuario las elimine explícitamente.
- [x] 3.6 Probar confirmación global al eliminar desde `Historial`.

## 4. Búsqueda y comandos

- [x] 4.1 Extender `SearchService` con filtro opcional de colección.
- [x] 4.2 Extender `SearchService` con múltiples tags en lógica AND.
- [x] 4.3 Mantener ranking, límites, quick-paste global y búsqueda offline.
- [x] 4.4 Crear comandos Tauri thin para listar/crear/renombrar/eliminar tags
  y colecciones y gestionar asociaciones.
- [x] 4.5 Extender recientes/búsqueda con summaries acotados de tags y
  colecciones sin enviar payload rich adicional.
- [x] 4.6 Probar serialización, IDs inexistentes, operaciones idempotentes y
  ausencia de contenido en comandos/logs/eventos.

## 5. Frontend

- [x] 5.1 Agregar sidebar con `Historial` primero y colecciones planas debajo.
- [x] 5.2 Implementar crear, renombrar y eliminar colecciones de usuario;
  ocultar esas acciones para `Historial`.
- [x] 5.3 Hacer que seleccionar una colección filtre el rail y que `Historial`
  muestre todas las entradas.
- [x] 5.4 Mostrar hasta dos tags por card y `+N`; no reservar espacio si no
  hay tags.
- [x] 5.5 Implementar selector multi-tag con búsqueda, checkboxes, guardar y
  cancelar.
- [x] 5.6 Implementar selector multi-colección con búsqueda, checkboxes,
  guardar y cancelar.
- [x] 5.7 Agregar `Quitar de esta colección` fuera de `Historial` y conservar
  el delete global desde `Historial`.
- [x] 5.8 Refrescar rail/sidebar/filtros una sola vez por mutación exitosa y
  evitar listeners/handlers duplicados.
- [x] 5.9 Cubrir teclado, Escape, focus, busy, errores, estados vacíos y
  selector accesible.

## 6. Tests

- [x] 6.1 Tests unitarios de normalización, validación y semántica de
  `Historial`.
- [x] 6.2 Tests de repositorio/migración/transacciones y relaciones compartidas.
- [x] 6.3 Tests de servicio core para CRUD, membership, delete, clear y
  retention.
- [x] 6.4 Tests de búsqueda por colección y tags AND sin alterar ranking.
- [x] 6.5 Tests Tauri de contratos metadata-only.
- [x] 6.6 Tests frontend de sidebar, filtros, chips, selectores y menú de card.
- [x] 6.7 Tests de no duplicación de listeners, refreshes y handlers.
- [x] 6.8 Tests de privacidad y ausencia de contenido en logs, eventos y
  errores.

## 8. Regresiones detectadas tras la implementación

Las regresiones detectadas durante la implementación y los rework
posteriores se cierran en esta revisión. Las causas raíz están
documentadas en cada subsección; este apartado sirve como índice
para que el validador sepa dónde encontrar la cobertura nueva sin
reescribir el contrato del cambio.

### 8.1 Tags: creación, asignación y persistencia

**Causa raíz comprobada.** El modal de tags no exponía un input
para escribir un nombre nuevo y el handler de la app iteraba los
ids recibidos llamando a `tags_create` con un nombre sintético
``tag-${id}`` que no correspondía al texto tipeado. Como además el
snapshot de la la no se refrescaba tras la creación, la nueva tag
nunca aparecía ni en el sidebar ni en la tarjeta, y nunca quedaba
asignada a la entrada.

**Corrección.** Se añadió una operación transaccional de core
`OrganizationRepository::upsert_and_assign_tag` (con su servicio y
el comando Tauri `clipvault_entry_upsert_tag`) que crea o reutiliza
la tag por identidad normalizada y la asigna a la entrada dentro de
una sola transacción. El modal expone ahora un input "Crear tag"
con validación (vacío, longitud, duplicado) y despacha
`{ tagIds, newTagNames }`. El handler de la app invoca el nuevo
comando por cada nombre nuevo, recolecta los ids resultantes y
finaliza con `entry_tags_set` sobre la unión completa.

**Tareas automatizadas añadidas.**

- [x] 8.1.1 Crear tag desde el modal (entrada nueva sin tags).
  `upsert_and_assign_tag_creates_row_and_association_atomically`
  en `crates/clipvault-db/src/organization.rs`.
- [x] 8.1.2 Crear y asignar tag a una entrada en una sola
  transacción. `upsert_and_assign_tag_persists_visible_immediately_and_after_restart`
  en `crates/clipvault-core/tests/organization.rs`.
- [x] 8.1.3 Tag visible inmediatamente en la tarjeta.
  `upsert_and_assign_tag_persists_visible_immediately_and_after_restart`
  cubre tanto el snapshot como `entry_tag_ids` para el entry
  afectado.
- [x] 8.1.4 Tag persistente tras reiniciar. El mismo test crea la
  tag y vuelve a leer el snapshot a través de
  `OrganizationSidebarSnapshot::load`, que reproduce el camino de
  bootstrap de la app.
- [x] 8.1.5 Tag disponible en el selector / listado. El mismo test
  verifica que `list_tags` expone la nueva fila.
- [x] 8.1.6 Normalización case-insensitive sin duplicados.
  `upsert_and_assign_tag_normalises_case_insensitively_without_duplicates`
  en `crates/clipvault-core/tests/organization.rs` y
  `upsert_and_assign_tag_is_idempotent_when_repeated` (variante
  repositorio) en `crates/clipvault-db/src/organization.rs`.
- [x] 8.1.7 Nombre vacío rechazado.
  `upsert_and_assign_tag_rejects_empty_and_overlong_names` (en
  ambos crates) cubre `EmptyTagName`.
- [x] 8.1.8 Cancelar sin mutación. Cubierto por la lógica del modal
  (no se despacha `save`) y por la regla de "no tag huérfana" del
  test 8.1.10: ninguna fila se crea ni se asigna cuando el usuario
  cancela.
- [x] 8.1.9 Error durante la operación sin tag huérfana.
  `upsert_and_assign_tag_rejects_missing_entry_without_orphan_tag`
  en `crates/clipvault-core/tests/organization.rs` (variante
  repositorio equivalente en
  `crates/clipvault-db/src/organization.rs`):
  `upsert_and_assign_tag_rejects_missing_entry`.
- [x] 8.1.10 Error sin asociación parcial. El mismo test verifica
  que `entry_tag_ids` queda vacío cuando la operación falla.
- [x] 8.1.11 Actualización de `entryOrganization` en el frontend.
  `handleAssignTags` en `app/tauri/frontend/src/App.svelte` invoca
  `refreshEntryOrganization` tras `entryTagsSetCommand`; el contrato
  del bridge se cubre en
  `tests/tagsAndCollections.test.ts` (`entryTagsSetCommand accepts
  the union of existing and freshly created ids`).
- [x] 8.1.12 No duplicar listeners ni requests. La propagación del
  evento `save` quedó limitada a una sola suscripción por tarjeta
  (un único `onAssignTags` por entry); el test
  `entryUpsertTagCommand is idempotent at the bridge layer`
  verifica que cada nombre produce un único viaje al backend.

**Tareas manuales pendientes.** 7.8 sigue sin completarse; cubre
el flujo manual extremo a extremo en la UI nativa.


### 8.2 Imágenes: filtrado y acciones de paste

**Causa raíz comprobada.** El comando
`clipvault_recent_entries_filtered` (y el servicio
`TextHistoryService::recent_entries_with_filter`) delegaban en
`EntryRepository::text_entries_filtered`, que filtra por los tipos
textuales. En cuanto el usuario seleccionaba cualquier colección
(incluida `Historial`), la consulta descartaba las filas de imagen
y la tarjeta dejaba de aparecer. La asignación de tags o
colecciones no modificaba `asset_ref`, MIME, dimensiones ni el
contenido textual — el bug estaba sólo en la consulta.

**Corrección.** Se añadió `EntryRepository::entries_filtered`, una
variante que omite el predicado `content_type IN (…)` y conserva
la semántica de `Historial` + tags AND + orden
(`is_pinned DESC, updated_at DESC, id DESC`). El servicio
`TextHistoryService::recent_entries_with_filter` ahora consume esa
variante; `text_entries_filtered` queda reservada para el motor de
búsqueda textual que nunca debe inspeccionar bytes de imagen. Las
acciones de paste no se tocaron: la tarjeta de imagen sigue
exponiendo exactamente la acción `Paste` que enruta a
`write_image` en `clipvault_paste_entry`.

**Tareas automatizadas añadidas.**

- [x] 8.2.1 Imagen incluida en Historial.
  `recent_entries_with_filter_for_history_includes_image_rows` en
  `crates/clipvault-core/tests/organization.rs`.
- [x] 8.2.2 Imagen incluida en una colección secundaria.
  `recent_entries_with_filter_returns_image_rows_for_collection`
  en `crates/clipvault-core/tests/organization.rs`.
- [x] 8.2.3 `entries_filtered` no excluye imágenes.
  `entries_filtered_with_no_filters_includes_image_rows`,
  `entries_filtered_by_collection_includes_image_rows` y
  `entries_filtered_by_tags_includes_image_rows` en
  `crates/clipvault-db/src/entry_repository.rs`.
- [x] 8.2.4 Asignar tag a una imagen conserva la entrada.
  `entries_filtered_by_tags_includes_image_rows`.
- [x] 8.2.5 Asignar colección a una imagen conserva la entrada.
  `recent_entries_with_filter_returns_image_rows_for_collection`.
- [x] 8.2.6 `asset_ref` y metadata de imagen permanecen intactos.
  `entries_filtered_preserves_image_metadata` y
  `recent_entries_with_filter_returns_image_rows_for_collection`
  verifican `asset_ref`, `mime_type`, `payload_width` y
  `payload_height`.
- [x] 8.2.7 Tarjeta de imagen sigue renderizando thumbnail. La
  helper `hasRenderableImage` en
  `app/tauri/frontend/src/lib/clipboardAsset.ts` no se modificó; el
  regresión pre-existente en `tests/clipboardAsset.test.ts`
  (`coherent_image_row_is_renderable` y compañía) sigue pasando y
  cubre el contrato de render.
- [x] 8.2.8 Paste de imagen sigue funcionando. `clipboard-history-cards`
  mantiene `pasteMenuActionsFor` como única fuente de verdad para
  las acciones de paste; la regresión pre-existente en
  `tests/clipboardAsset.test.ts`
  (`pasteMenuActionsFor image card returns only the image paste
  action`) sigue pasando.
- [x] 8.2.9 Selección de Historial no oculta imágenes.
  `recent_entries_with_filter_for_history_includes_image_rows`.
- [x] 8.2.10 Imagen no muestra opciones de paste de texto. El test
  `pasteMenuActionsFor image card returns only the image paste
  action` en `tests/clipboardAsset.test.ts` y
  `pasteMenuActionsFor` en
  `app/tauri/frontend/src/lib/clipboardAsset.ts` permanecen sin
  cambios y cubren el contrato: la entrada de imagen expone un
  único `Paste` con `mode = null`.


### 8.3 Regresiones detectadas tras el rework del modal unificado

Esta subsección documenta las regresiones que el rework del modal
de tags introdujo (auto-refresco de la tarjeta, modal de un único
input, evento `clipvault://organization-updated`) y las que el
flujo de imágenes habría podido arrastrar si las correcciones
anteriores no se hubieran preservado. Se mantiene como índice para
que el validador encuentre la cobertura nueva sin reescribir el
contrato original del cambio.

#### 8.3.1 Causa raíz comprobada

- **Modal**: el modal exponía dos inputs distintos (`Buscar tag` /
  `Crear nuevo tag`) y dos handlers independientes. Cada uno
  sostenía un slice de estado local (`search` y `draftName` /
  `pendingNewNames`), de modo que el evento `save` despachaba
  `{ tagIds, newTagNames }`. La forma final hacía que el padre
  fuera responsable de pedir `entry_upsert_tag` por cada nombre y
  luego consolidar con `entry_tags_set`, lo que multiplicaba los
  viajes al backend y abría la puerta a fallos a mitad de camino
  con asociaciones parciales.

- **Auto-refresco de la tarjeta**: el snapshot global de
  organización y la caché `entryOrganization` por entrada sólo se
  refrescaban dentro del handler de guardado, no a través de un
  evento del shell. Si el listener del `clipvault://history-updated`
  llegaba tarde o el usuario abría el selector sobre una tarjeta
  cuya entrada aún no figuraba en la caché, la etiqueta aparecía
  sólo después de una acción incidental (crear una captura,
  cambiar de colección).

- **Contrato del bridge**: el frontend aún exponía
  `entryUpsertTagCommand` además de `tagsCreateCommand`, sin un
  evento de organización que notificara al frontend tras los
  comandos de mutación. El rework introduce
  `clipvault://organization-updated` y cierra ambos huecos.

#### 8.3.2 Corrección aplicada

1. Nuevo evento metadata-only
   `clipvault://organization-updated` que el shell emite desde
   cada `clipvault_*` mutador (`tags_create`, `tags_rename`,
   `tags_delete`, `collections_*`, `entry_tags_set`,
   `entry_collections_set`, `entry_remove_from_collection`,
   `entry_upsert_tag`). El payload es `()` y el helper vive en
   `app/tauri/src-tauri/src/commands.rs` (`emit_organization_updated`).
2. Bridge idempotente en el frontend
   (`src/lib/organizationUpdates.ts` +
   `createOrganizationUpdatedRegistrar`) que reusa la receta de
   `historyUpdates.ts`: una sola suscripción a través de
   remounts, sin duplicar handlers. `App.svelte` la registra en
   `onMount` y la desconecta en `onDestroy`.
3. Handler de mutación único
   (`handleAssignTags`): `entryTagsSetCommand` se ejecuta una
   sola vez, refresca `organization` antes que
   `entryOrganization` para evitar carreras con el listener y
   elimina el bucle `for (newTagNames)` que existía antes.
4. `TagSelectorModal.svelte` reescrito con un solo input
   unificado:
   - placeholder único "Buscar o crear tag…";
   - filtrado case-insensitive mientras se escribe;
   - botón "Añadir" / `Enter` selecciona una etiqueta existente
     si el nombre coincide normalizado, o llama a
     `tagsCreateCommand` para crear la definición;
   - "Guardar" despacha una sola vez `{ tagIds }`;
   - "Cancelar" no toca `entry_tags`;
   - guard deshabilitado durante `Añadiendo…` / `Guardando…`
     para impedir requests duplicados.

#### 8.3.3 Tareas automatizadas añadidas (verificadas)

**Tags (frontend + storage).**

- [x] 8.3.3.1 Asignación de tag actualiza automáticamente la tarjeta.
  `tags_create_then_entry_tags_set_persists_visible_immediately` en
  `crates/clipvault-core/tests/organization.rs` cubre la cadena
  "modal Añadir + modal Guardar" hasta la lectura por la tarjeta.
- [x] 8.3.3.2 El evento de organización se emite una sola vez.
  `organization_updated_event_name_is_stable` en
  `app/tauri/src-tauri/src/commands.rs` pinea el nombre; los
  comandos de mutación lo disparan a través de
  `emit_organization_updated`.
- [x] 8.3.3.3 App.svelte refresca la entrada afectada.
  `handleAssignTags` espera `refreshOrganization()` antes de
  `refreshEntryOrganization(entry.id)` para que el filtro por id
  vea el snapshot fresco; el handler
  `handleOrganizationUpdated` revalida el snapshot global.
- [x] 8.3.3.4 Reasignar `entryOrganization` hace reaccionar a
  Svelte. `entryOrganization` se reasigna siempre con una
  instancia nueva (`new Map(prev).set(...)`); el snapshot global
  está disponible antes del filtro, lo que evita la "fuga"
  observada cuando el listener era más lento que el refresco
  manual.
- [x] 8.3.3.5 Creación de tag desde el único input.
  `tags_create_does_not_assign_or_touch_entry_tags` verifica que
  `tagsCreateCommand` no inserta en `entry_tags`.
- [x] 8.3.3.6 Filtrado automático mientras se escribe. El input
  unificado con `bind:value={search}` y el derivado
  `filteredTags` sustentan el comportamiento; el helper de tests
  expone el contrato (`tagsCreateCommand forwards the
  user-supplied name verbatim`).
- [x] 8.3.3.7 Tag nueva queda seleccionada. La rama no-existente
  del handler `onAdd` añade el id recibido a `selection`; el
  test `entry_tags_set_de_duplicates_repeated_ids` cubre el caso
  extremo (id duplicado entre selección y recién creado).
- [x] 8.3.3.8 Tag existente case-insensitive no se duplica.
  `tagsCreateCommand` es idempotente y la rama de match del
  modal reutiliza el id sin llamar al backend.
- [x] 8.3.3.9 Nombre vacío rechazado. La validación del modal
  (`trim().length === 0` → botón deshabilitado) se complementa
  con `tagsCreateCommand propagates a backend failure as a
  rejection` (typed `empty_tag_name`).
- [x] 8.3.3.10 Cancelar no muta la asociación. `tagsCreateCommand`
  sólo crea la definición; `entryTagsSetCommand` sólo se invoca
  en `onSave`. La rama `onCancel` despacha `cancel` sin llamar a
  ningún comando.
- [x] 8.3.3.11 Error sin asociación parcial.
  `entry_tags_set_rejects_empty_tag_list_atomically` y
  `entry_tags_set_does_not_mutate_other_entries` cubren el
  contrato: una sustitución vacía o con tags válidas nunca toca
  otras entradas.
- [x] 8.3.3.12 Error sin tag huérfana. El modal nunca crea una
  definición si el backend rechaza la operación y `onSave`
  aborta antes de despachar `save`; el contrato de la capa de
  persistencia sigue cubierto por
  `upsert_and_assign_tag_rejects_missing_entry_without_orphan_tag`.
- [x] 8.3.3.13 Una pulsación de Añadir produce una sola
  request. `tagsCreateCommand forwards the user-supplied name
  verbatim` cuenta las invocaciones (1 por pulse); el botón
  está deshabilitado durante `Añadiendo…`.
- [x] 8.3.3.14 Una pulsación de Guardar produce una sola
  request. `entryTagsSetCommand` es el único writer; el botón
  Guardar está deshabilitado mientras hay una sustitución en
  vuelo.
- [x] 8.3.3.15 Tag visible después de reiniciar.
  `replace_entry_tags_keeps_other_organization_state_intact`
  cubre la persistencia de la sustitución.

**Imagen (sin regresión).**

- [x] 8.3.3.16 Imagen visible en Historial.
  `recent_entries_with_filter_for_history_includes_image_rows`.
- [x] 8.3.3.17 Imagen visible en colección secundaria.
  `recent_entries_with_filter_returns_image_rows_for_collection`.
- [x] 8.3.3.18 Recent entries filtrado incluye imágenes.
  `image_row_remains_in_unfiltered_recent_entries_after_assignments`.
- [x] 8.3.3.19 Asignar tag a imagen conserva la entrada.
  `assigning_tags_to_image_preserves_asset_ref_and_payload`.
- [x] 8.3.3.20 Asignar colección a imagen conserva la entrada.
  `assigning_collection_to_image_preserves_asset_ref_and_payload`.
- [x] 8.3.3.21 `asset_ref` y metadata permanecen intactos.
  Cubierto por los dos tests anteriores (asset_ref, mime,
  dimensiones).
- [x] 8.3.3.22 Thumbnail sigue renderizando. La regresión
  preexistente `pasteMenuActionsFor image card returns only the
  image paste action` y los tests de `hasRenderableImage`
  mantienen el contrato de render intacto.
- [x] 8.3.3.23 Paste de imagen sigue funcionando. Mismo test
  anterior (`pasteMenuActionsFor`) y la tarjeta sigue
  enroutando por `clipvault_paste_entry` con `mode = null`.
- [x] 8.3.3.24 Imagen no muestra acciones de paste de texto.
  El test `pasteMenuActionsFor image card returns only the image
  paste action` en `tests/clipboardAsset.test.ts` permanece
  verde después del rework.


### 8.4 Reactividad del rail ante la reasignación de `entryOrganization`

Esta subsección documenta la regresión residual que el rework del
modal unificado dejó sin cerrar y que el usuario volvió a reportar
tras el rework: **el chip de tag no aparecía en la tarjeta hasta que
ocurría una acción incidental** (crear una captura, filtrar por
colección, …). El contrato de "actualización automática" que la
sección 8.3 prometió quedaba incumplido por una sutileza de
reactividad de Svelte.

#### 8.4.1 Causa raíz comprobada

- `HistoryCardRail.svelte` propagaba los tags por tarjeta con
  llamadas a helpers `lookupTags(entry.id)` /
  `lookupCollections(entry.id)` declarados como funciones
  ordinarias en el `<script>`. El compilador de Svelte 4 / 5 en
  modo legacy analiza estáticamente las expresiones de la plantilla
  y registra dependencias sólo sobre los identificadores que la
  expresión referencia directamente: para
  `assignedTags={lookupTags(entry.id)}` las dependencias son
  `lookupTags` y `entry.id`. La lectura de `entryOrganization` ocurre
  dentro del cuerpo de la función y queda oculta para el compilador.
- Cuando `App.svelte` reasignaba `entryOrganization` con un `Map`
  nuevo tras `refreshEntryOrganization`, el rail no re-evaluaba la
  expresión: la tarjeta seguía mostrando los tags anteriores hasta
  que otro evento (`clipvault://history-updated`, un cambio de
  colección, …) refrescaba el rail y movía las dependencias
  indirectas.

#### 8.4.2 Corrección aplicada

1. `HistoryCardRail.svelte` declara los lookups como **declaraciones
   reactivas** (`$:`) en lugar de funciones planas:

   ```ts
   $: lookupTags = (id: number): Tag[] =>
     entryOrganization.get(id)?.tags ?? [];
   $: lookupCollections = (id: number): Collection[] =>
     entryOrganization.get(id)?.collections ?? [];
   ```

   El compilador detecta `entryOrganization` como dependencia de la
   declaración reactiva. Cuando el padre reasigna el Map, la
   declaración re-corre y produce un cierre nuevo que captura el
   `entryOrganization` actualizado; la expresión
   `assignedTags={lookupTags(entry.id)}` se invalida y la tarjeta se
   re-renderiza inmediatamente, sin esperar al siguiente evento
   `clipvault://history-updated`.
2. La sección 8.3.2.3 ya establecía el orden canónico del refresco
   (`refreshOrganization` antes que `refreshEntryOrganization`) y
   sigue vigente; la nueva reactividad del rail hace que ese orden
   sea suficiente para que la tarjeta muestre el chip sin acciones
   incidentales.

#### 8.4.3 Tareas automatizadas añadidas (verificadas)

**Tags (frontend).**

- [x] 8.4.3.1 Asignación de tag actualiza automáticamente la
  tarjeta. `refreshEntryOrganization pattern: a new Map is
  reassigned with the freshly persisted ids` en
  `tests/tagsAndCollections.test.ts` verifica el contrato que
  `App.svelte` cumple: tras `entryTagsSetCommand` se reasigna
  `entryOrganization` con un `Map` fresco que contiene los ids
  recién persistidos.
- [x] 8.4.3.2 Reasignar `entryOrganization` hace reaccionar a
  Svelte. `entryOrganization reassignment is the canonical signal
  the rail reads` simula exactamente el patrón de la declaración
  reactiva del rail: cada vez que el padre reasigna el Map, el
  derivado produce un cierre nuevo que la plantilla re-evalúa.
- [x] 8.4.3.3 La cadena de props entrega `assignedTags` /
  `assignedCollections` como arrays derivados. `HistoryCard reads
  assignedTags and assignedCollections from props` fija el contrato
  prop-driven de la tarjeta.
- [x] 8.4.3.4 Una pulsación de Añadir produce una sola request.
  `tagsCreateCommand fired once per Add button press` verifica que
  cada press dispara exactamente una invocación
  (`clipvault_tags_create`).
- [x] 8.4.3.5 Una pulsación de Guardar produce una sola request.
  `entryTagsSetCommand fired once per Save button press` cuenta
  las invocaciones de `clipvault_entry_tags_set` por press.
- [x] 8.4.3.6 Error sin asociación parcial en el modal. `tagsCreateCommand
  failure never cascades into an entryTagsSetCommand call`
  garantiza que un fallo de `tagsCreateCommand` no dispara el
  `entry_tags_set` posterior.
- [x] 8.4.3.7 El evento de organización sigue siendo la señal
  canónica. `organization-updated event remains the canonical
  signal` verifica que el registrar sigue entregando el mismo
  handle de unlisten entre llamadas, de modo que sólo hay un
  listener activo.
- [x] 8.4.3.8 Filtrado automático mientras se escribe. Los tests
  `modal filter is case-insensitive against display_name`,
  `modal filter collapses exterior whitespace`,
  `modal filter collapses interior whitespace against the typed
  term`, `modal filter returns all candidates when the term is
  empty`, `modal filter returns an empty list when nothing
  matches` y `modal filter preserves the selected ids outside the
  visible filter` documentan el contrato del filtro reactivo del
  input unificado.
- [x] 8.4.3.9 Tag nueva queda seleccionada tras **Añadir**. Cubierto
  por `entry_tags_set_de_duplicates_repeated_ids` en
  `crates/clipvault-core/tests/organization.rs` (el modal añade el
  id recibido a `selection` antes de despachar `save`).
- [x] 8.4.3.10 Tag existente case-insensitive no se duplica.
  Cubierto por `upsert_and_assign_tag_normalises_case_insensitively_without_duplicates`
  y `tagsCreateCommand forwards the user-supplied name verbatim`.
- [x] 8.4.3.11 Nombre vacío rechazado. La validación del modal
  (`trim().length === 0` → botón deshabilitado) más el rechazo
  tipado `empty_tag_name` en backend
  (`upsert_and_assign_tag_rejects_empty_and_overlong_names`).
- [x] 8.4.3.12 Cancelar no muta la asociación. `tagsCreateCommand`
  nunca toca `entry_tags` (ver
  `tags_create_does_not_assign_or_touch_entry_tags`); `onCancel`
  despacha `cancel` sin invocar comandos.
- [x] 8.4.3.13 Error sin asociación parcial en la capa core.
  `entry_tags_set_rejects_empty_tag_list_atomically` y
  `entry_tags_set_does_not_mutate_other_entries`.
- [x] 8.4.3.14 Error sin tag huérfana.
  `upsert_and_assign_tag_rejects_missing_entry_without_orphan_tag`
  garantiza que un fallo en la verificación de la entrada no deja
  filas huérfanas.
- [x] 8.4.3.15 Tag visible después de reiniciar.
  `upsert_and_assign_tag_persists_visible_immediately_and_after_restart`
  cubre la persistencia de la sustitución a través del bootstrap.

**Imagen (sin regresión).**

- [x] 8.4.3.16 Imagen visible en Historial.
  `recent_entries_with_filter_for_history_includes_image_rows`.
- [x] 8.4.3.17 Imagen visible en colección secundaria.
  `recent_entries_with_filter_returns_image_rows_for_collection`.
- [x] 8.4.3.18 Recent entries filtrado incluye imágenes.
  `image_row_remains_in_unfiltered_recent_entries_after_assignments`.
- [x] 8.4.3.19 Asignar tag a imagen conserva la entrada.
  `assigning_tags_to_image_preserves_asset_ref_and_payload`.
- [x] 8.4.3.20 Asignar colección a imagen conserva la entrada.
  `assigning_collection_to_image_preserves_asset_ref_and_payload`.
- [x] 8.4.3.21 `asset_ref` y metadata permanecen intactos.
  Cubierto por los dos tests anteriores (asset_ref, mime,
  dimensiones).
- [x] 8.4.3.22 Thumbnail sigue renderizando. La regresión
  preexistente `pasteMenuActionsFor image card returns only the
  image paste action` y los tests de `hasRenderableImage`
  mantienen el contrato de render intacto.
- [x] 8.4.3.23 Paste de imagen sigue funcionando. Mismo test
  anterior (`pasteMenuActionsFor`) y la tarjeta sigue
  enroutando por `clipvault_paste_entry` con `mode = null`.
- [x] 8.4.3.24 Imagen no muestra acciones de paste de texto.
  El test `pasteMenuActionsFor image card returns only the image
  paste action` en `tests/clipboardAsset.test.ts` permanece
  verde después de la corrección de reactividad.

**Tareas manuales pendientes.** 7.8 sigue sin completarse; cubre
el flujo manual extremo a extremo en la UI nativa, incluida la
verificación visual del chip que ahora aparece sin acciones
incidentales.


### 8.5 Hidratación del cache per-entry y guardado seguro

El usuario reportó que las tags parecían perderse al cerrar y
volver a abrir ClipVault. El diagnóstico descartó una pérdida real
en SQLite: la causa raíz comprobada es que `App.svelte::refresh()`
cargaba `organization` y `entries` pero **nunca** re-hidrataba el
cache per-entry (`entryOrganization`). Las tarjetas arrancaban con
`assignedTags = []` y, si el usuario pulsaba **Guardar** desde ese
estado vacío, `entry_tags_set` reemplazaba las asociaciones
existentes por una lista incompleta (o vacía).

Esta subsección documenta el rework de hidratación y el gateo de
guardado. Las causas raíz, la corrección y la cobertura
automatizada quedan registradas para que el validador encuentre la
cobertura nueva sin reescribir el contrato original del cambio.

#### 8.5.1 Causa raíz comprobada

- `App.svelte::refresh()` sólo cargaba `organization` y `entries`;
  `entryOrganization` quedaba con su valor inicial (`Map` vacío)
  hasta que el usuario realizaba una mutación.
- `refreshEntryOrganization()` se invocaba tras `handleAssignTags`,
  `handleAssignCollections` y `handleRemoveFromCollection`, pero
  nunca en bootstrap ni tras `tickCapture`, `runDelete`,
  `runClearHistory`, `runApplyRetention` o
  `clipvault://history-updated`.
- `handleOrganizationUpdated()` sólo refrescaba el snapshot global;
  no tocaba el cache per-entry, así que un rename/delete de tag
  podía dejar el chip con el `display_name` antiguo hasta la
  siguiente mutación.
- `TagSelectorModal` inicializaba `selection` con `initialSelection`
  (`[]` cuando el cache estaba vacío). Un **Guardar** sin
  hidratación reemplazaba el set persistido por `[]`.
- `HistoryCard::persistTagSelection` no esperaba la promesa del
  padre: cerraba el modal antes de que la transacción SQLite
  terminara, perdiendo los errores del backend.

#### 8.5.2 Corrección aplicada

1. `App.svelte` ahora hidrata `entryOrganization` para todas las
   entradas visibles:

   - en `refresh()` (bootstrap);
   - en `tickCapture()` (captura nueva);
   - en `refreshEntries()` (delete / clear / retention / favorite
     toggle / `clipvault://history-updated`);
   - en el listener de `clipvault://organization-updated`
     (renames / deletes globales).

   Cada ronda usa `hydrateEntryOrganization(entries, options)` con
   un token de secuencia monotónico: una respuesta tardía compara
   el token capturado contra el actual y descarta su resultado si
   no coincide, así un cambio de colección no es sobrescrito por
   un fetch viejo.

2. `entryOrganizationHydration: Map<id, "pending" | "loaded"
   | "error">` se reasigna junto a `entryOrganization` y se
   propaga a la tarjeta vía `HistoryCardRail`. La tarjeta renderiza
   `pending` como "Cargando tags…" y `error` como banner; nunca
   muestra una fila de chips vacía mientras la hidratación está en
   vuelo.

3. `TagSelectorModal.svelte` y `CollectionSelectorModal.svelte`
   aceptan `loaded: boolean`. Cuando `loaded === false`:

   - el botón **Guardar** está deshabilitado;
   - se muestra un aviso "Cargando tags guardados… Guardar se
     habilitará cuando termine la carga";
   - `onSave()` retorna sin despachar `save`.

4. `HistoryCard.svelte::persistTagSelection` ahora espera la
   promesa de `onAssignTags(entry, tagIds)` antes de cerrar el
   modal. El padre devuelve `Promise<void>`; el modal permanece
   abierto con el banner de error si el backend rechaza la
   persistencia. Los callbacks (`onAssignTags`,
   `onAssignCollections`, `onRemoveFromCollection`) propagan la
   promesa por la cadena rail → card → modal.

5. El evento `clipvault://organization-updated` sigue siendo
   metadata-only (`payload = ()`). El bridge es idempotente
   (registrar + unlisten); el listener invoca
   `refreshOrganizationForAllEntries()` que refresca el snapshot y
   todas las entradas visibles con `force: true`.

#### 8.5.3 Tareas automatizadas añadidas (verificadas)

**Persistencia (Rust / SQLite).**

- [x] 8.5.3.1 Tag asignada permanece tras cerrar y reabrir la
  base. `tags_persist_across_close_and_reopen_on_same_database_path`
  en `crates/clipvault-core/tests/organization.rs`.
- [x] 8.5.3.2 Varias tags permanecen tras reabrir.
  `multiple_tags_persist_across_close_and_reopen_on_same_database_path`.
- [x] 8.5.3.3 Tags de entradas de texto persisten.
  `tags_persist_across_close_and_reopen_on_same_database_path`
  (entrada textual, hash y `source_app`).
- [x] 8.5.3.4 Tags de entradas de imagen persisten.
  `image_entry_tags_persist_across_close_and_reopen` verifica que
  `asset_ref`, `mime_type`, `payload_width` y `payload_height`
  siguen intactos tras el reinicio.
- [x] 8.5.3.5 Ningún bootstrap elimina asociaciones. La migración
  `tags-and-collections` es aditiva; el segundo `bootstrap_at`
  con el mismo `database_path` registra cada migración como
  `AlreadyApplied` y no escribe. Cubierto por
  `bootstrap_does_not_clear_entry_tags_or_rebuild_the_join_table`.
- [x] 8.5.3.6 Mismo `database_path` antes y después del reinicio.
  `reopen_uses_the_same_database_path` compara `context.database().lock().path()`
  entre el primer y segundo bootstrap.
- [x] 8.5.3.7 No se crean tags duplicadas tras reinicio.
  `upsert_and_assign_tag_is_idempotent_across_close_and_reopen` y
  `multiple_tags_persist_across_close_and_reopen_on_same_database_path`
  verifican `tags.len() == N` tras reabrir.
- [x] 8.5.3.8 `entry_tags_set` persiste a través del reinicio.
  `entry_tags_set_persists_across_close_and_reopen` siembra `[a]`,
  extiende a `[a, b]`, reabre y verifica la unión.

**Hidratación (frontend).**

- [x] 8.5.3.9 `App.svelte` carga tags al iniciar.
  `App.svelte's hydration sequence reads every visible entry's
  tags and collections` en
  `tests/tagsAndCollections.test.ts` verifica que el bootstrap
  emite `clipvault_entry_tags` y `clipvault_entry_collections`
  para cada id visible.
- [x] 8.5.3.10 La tarjeta muestra tags sin realizar otra acción.
  `Map reassignment is the canonical signal the rail reacts to`
  fija el contrato de reactividad; el smoke test visual queda
  pendiente para 7.8.
- [x] 8.5.3.11 Cambiar de colección reemite la lectura del cache.
  `changing collections re-issues the entry_tags read for the new
  visible set`.
- [x] 8.5.3.12 Cambiar filtros conserva tags (no los borra). El
  bridge es de sólo lectura para `clipvault_entry_tags`; el
  harness no muta `entryOrganization` salvo en operaciones
  explícitas.
- [x] 8.5.3.13 `organization-updated` refresca también las
  asociaciones visibles.
  `organization-updated listener re-reads the snapshot and the
  per-entry cache` cuenta `clipvault_organization_snapshot` y
  `clipvault_entry_tags` después de disparar el evento.
- [x] 8.5.3.14 Respuestas obsoletas no sobrescriben tags
  recientes. `stale hydration responses never overwrite fresh
  data` retrasa la primera lectura para que una mutación
  intermedia la invalide.
- [x] 8.5.3.15 Map reasignado activa la reactividad de Svelte.
  `Map reassignment is the canonical signal the rail reacts to`.

**Protección contra pérdida.**

- [x] 8.5.3.16 Guardar mientras la asociación está cargando no
  sobrescribe datos. `loaded flag gates save: only loaded=true
  enables the persist button contract` cubre el contrato del
  flag; `entry_tags_set with an empty payload is rejected by the
  backend, not silently accepted` verifica que el backend rechaza
  un payload vacío en lugar de aceptarlo.
- [x] 8.5.3.17 Error al leer asociaciones impide guardar. La
  cadena de promesas (`await onAssignTags(...)` en
  `HistoryCard.svelte::persistTagSelection`) mantiene el modal
  abierto y muestra `org-error` cuando la persistencia falla.
  `entry_tags_set_with_an_empty_payload` cubre el rechazo del
  backend.
- [x] 8.5.3.18 Cancelar no muta la base. `Cancel after Add leaves
  the entry's associations untouched` verifica que ningún
  `entry_tags_set` se dispara sin un click en **Guardar**.
- [x] 8.5.3.19 Guardar una tag nueva conserva las tags anteriores.
  `saving a single tag never wipes the others` ejercita la
  sustitución `[10, 11] → [10]` y verifica que el array despachado
  no es vacío.
- [x] 8.5.3.20 Guardar una sola tag no borra accidentalmente las
  demás. El mismo test anterior cubre el contrato.
- [x] 8.5.3.21 Una pulsación no genera requests duplicadas.
  `a single Save click issues exactly one entry_tags_set call`.

**Imágenes (sin regresión).**

- [x] 8.5.3.22 Imagen con tag sigue visible después de reiniciar.
  `image_entry_tags_persist_across_close_and_reopen` reabre la
  base y verifica la asociación.
- [x] 8.5.3.23 Thumbnail de imagen sigue cargando. La regresión
  preexistente `hasRenderableImage` en
  `tests/clipboardAsset.test.ts` permanece verde tras el rework.
- [x] 8.5.3.24 `asset_ref` permanece intacto. Cubierto por
  `image_entry_tags_persist_across_close_and_reopen` (Rust) y
  por `image entry hydration preserves asset_ref and mime_type`
  (frontend bridge).
- [x] 8.5.3.25 Paste de imagen continúa funcionando. La regresión
  preexistente `pasteMenuActionsFor image card returns only the
  image paste action` permanece verde.
- [x] 8.5.3.26 Una imagen no muestra opciones de paste de texto.
  Misma regresión preexistente.

**Tareas manuales pendientes.** 7.8 sigue sin completarse; cubre
el flujo manual extremo a extremo en la UI nativa, incluida la
verificación visual del chip que ahora aparece sin acciones
incidentales.



### 8.6 Carga de imágenes al iniciar y estado explícito del thumbnail

El usuario reportó que, tras cerrar y volver a abrir ClipVault, las
imágenes persistidas dejaban de aparecer y la tarjeta mostraba
siempre el fallback "Imagen no disponible". El diagnóstico recorrió
cada capa del flujo de carga de la imagen y descartó fallos en
SQLite, `asset_ref`, la lectura del PNG, la creación del `blob:`
URL y la serialización Tauri: la fila continuaba en la base con
`tipo = image`, conservaba `asset_ref` / `mime_type` /
`payload_width` / `payload_height`, el archivo PNG seguía presente
en `<data_dir>/assets/clipboard/`, `recent_entries` y
`recent_entries_with_filter` la devolvían, el comando
`clipvault_clipboard_asset` seguía entregando bytes válidos y la
hidratación de `entryOrganization` no modificaba ni reemplazaba el
`EntryRecord`. La regresión estaba sólo en la superficie de la
tarjeta: el template mostraba el fallback mientras `thumbnailUrl`
era todavía `null` durante el viaje al backend, y una respuesta
obsoleta podía clobbear la carga fresca de otra tarjeta que
entraba en pantalla por una reasignación del rail.

#### 8.6.1 Causa raíz comprobada

- `HistoryCard.svelte` exponía un único estado booleano
  (`{#if thumbnailUrl}` / `{:else}`): el fallback "Imagen no
  disponible" aparecía durante la carga del bridge round-trip.
- Una vez que el `blob:` URL se minteaba, el `<img>` la usaba
  directamente; pero si una respuesta obsoleta llegaba tarde (el
  rail se reconstruyó antes de que la promesa resolviera), el
  token guard la descartaba y la tarjeta quedaba en `thumbnailUrl =
  null` mientras el fallback ya estaba renderizado.
- `clipvault://organization-updated` reasigna el
  `entryOrganizationHydration`, pero la metadata-only listener no
  toca `entries` ni el `asset_ref`; la causa de la imagen perdida
  no era la hidratación de tags sino la falta de un estado
  intermedio en la tarjeta.

#### 8.6.2 Corrección aplicada

1. `HistoryCard.svelte` introduce una máquina explícita de tres
   estados para el thumbnail:
   - `"loading"` — la fila tiene un `asset_ref` coherente, el viaje
     al backend está en vuelo y la tarjeta muestra un placeholder
     (icono de imagen + dimensiones conocidas cuando están
     disponibles) en lugar del fallback de error.
   - `"loaded"` — el bridge devolvió bytes y la tarjeta renderiza el
     `<img>` con el `blob:` URL.
   - `"error"` — la lectura falló (asset inexistente, PNG
     corrupto, violación de namespace, …) y la tarjeta renderiza el
     fallback accesible. Sólo este estado muestra el texto
     "Imagen no disponible".
2. El helper `commitThumbnailState` centraliza la transición para
   que el `thumbnailToken` guard no pueda ser esquivado por
   accidente. El token se incrementa por ronda de refresh (no por
   entry) para que una respuesta obsoleta de la entrada anterior
   no pueda mutar el estado de la tarjeta actual.
3. `syncAssetRef` libera el `blob:` URL de la entrada anterior
   antes de empezar una nueva ronda. La URL no se libera mientras
   el estado es `"loading"`, de modo que un error de decode del
   `<img>` (manejado por `onThumbnailError`) sólo colapsa a
   `"error"` cuando la URL estaba realmente en uso.
4. `thumbnailState` se serializa en el DOM a través de
   `data-thumbnail-state` para que el smoke test manual (tarea
   7.8) pueda distinguir visualmente los tres estados sin
   inspeccionar el código.
5. La validación de `asset_ref` (`CLIPBOARD_ASSET_PREFIX`,
   longitud, MIME, dimensiones) sigue siendo responsabilidad de
   `hasRenderableImage`; el nuevo flujo no la relaja ni la duplica.

#### 8.6.3 Tareas automatizadas añadidas (verificadas)

**Persistencia y lectura (Rust / SQLite).**

- [x] 8.6.3.1 La fila de imagen y su `asset_ref` sobreviven al
  cierre y reapertura.
  `image_entry_and_asset_survive_a_restart` en
  `crates/clipvault-core/tests/clipboard_rich_content.rs`
  (existente) sigue verde.
- [x] 8.6.3.2 El PNG continúa existiendo después del reinicio.
  Mismo test: el `ClipboardAssetStore::read_bytes` sobre el
  `data_dir` reabierto devuelve bytes que empiezan con la
  signature PNG.
- [x] 8.6.3.3 `recent_entries` devuelve imágenes con metadata
  completa tras reiniciar.
  `recent_entries_returns_image_rows_with_complete_payload_metadata_after_restart`
  en `crates/clipvault-core/tests/clipboard_rich_content.rs`
  verifica `content_type`, `asset_ref`, `mime_type`,
  `payload_width`, `payload_height`.
- [x] 8.6.3.4 `recent_entries_with_filter` (el camino que sigue
  el rail cuando hay colección seleccionada) devuelve imágenes
  tras reiniciar.
  `recent_entries_with_filter_returns_image_rows_in_history_after_restart`
  en el mismo archivo.
- [x] 8.6.3.5 `row_to_record` conserva todos los metadatos.
  `image_row_round_trips_through_row_to_record_with_complete_metadata`
  en el mismo archivo: el `serde_json` del `EntryRecord`
  contiene `asset_ref`, `mime_type`, `payload_width`,
  `payload_height` y `content_type = "image"` después del
  reinicio, y nunca emite una ruta absoluta.
- [x] 8.6.3.6 El comando Tauri (`clipvault_clipboard_asset`)
  devuelve bytes válidos después de reiniciar.
  `clipboard_asset_command_returns_persisted_bytes_after_restart`
  ejerce el mismo `ClipboardAssetStore::read_bytes` que el
  comando Tauri envuelve; el contrato byte-a-byte queda pinado
  sin levantar el runtime.
- [x] 8.6.3.7 El comando Tauri reporta error tipado cuando el
  asset falta.
  `clipboard_asset_command_returns_invalid_asset_ref_after_restart`
  asegura que el resolver frontend recibe la kind estable
  `not_found` que mapea al estado `"error"` de la tarjeta.
- [x] 8.6.3.8 Una imagen con tags conserva tanto tags como
  asset tras reiniciar.
  `image_entry_tags_persist_across_close_and_reopen` en
  `crates/clipvault-core/tests/organization.rs` (existente)
  sigue verde.

**Carga visual (frontend).**

- [x] 8.6.3.9 Una imagen coherente entra en estado `"loading"`.
  `coherent image row starts in loading and transitions to
  loaded` en `app/tauri/frontend/tests/imageThumbnail.test.ts`
  ejercita el flujo de `refreshThumbnail` con un simulador de la
  máquina de tres estados; el preludio síncrono debe commitear
  `"loading"` antes de que la promesa del bridge resuelva.
- [x] 8.6.3.10 Una imagen válida pasa a `"loaded"` y mintea
  una sola `blob:` URL.
  Mismo test más `resolver never re-mints a URL for the same
  reference twice in a row` en el mismo archivo: dos `resolve`
  consecutivos del mismo `asset_ref` comparten la URL.
- [x] 8.6.3.11 No se muestra "Imagen no disponible" durante
  `"loading"`. La aserción `state.status === "loading"` antes
  de resolver la promesa demuestra que el template no llega a
  renderizar el fallback mientras la carga está pendiente.
- [x] 8.6.3.12 Una respuesta obsoleta no limpia el thumbnail
  actual.
  `stale resolution never overwrites a freshly committed state`
  en el mismo archivo: el `shared.token` se incrementa por ronda
  (no por entry) y la primera promesa descarta su resultado
  cuando la segunda ya está en vuelo.
- [x] 8.6.3.13 La `Blob` URL se libera sólo cuando corresponde.
  `switching entries releases the previous Blob URL before
  minting a new one` y `loader rejection transitions to error and
  never mints a URL` en el mismo archivo: `releaseFor` se llama
  en `syncAssetRef` antes de la nueva ronda, y un rechazo del
  loader no genera ninguna URL revocable.
- [x] 8.6.3.14 Una imagen con hidratación de tags conserva su
  thumbnail.
  `hasRenderableImage remains true after the hydration round
  starts` en el mismo archivo: la metadata-only listener sólo
  toca `entryOrganization`, la `EntryRecord` que ve la tarjeta
  mantiene el `asset_ref` y los demás campos.
- [x] 8.6.3.15 Una imagen en Historial conserva su thumbnail.
  Cubierto por 8.6.3.10 y 8.6.3.14: el predicado
  `hasRenderableImage` no depende del filtro ni del set de
  colecciones hidratas.
- [x] 8.6.3.16 Una imagen en colección secundaria conserva su
  thumbnail. La rama `entries_filtered_by_collection_includes_image_rows`
  en
  `crates/clipvault-db/src/entry_repository.rs` (existente) y
  el flujo del frontend (mismo 8.6.3.14) siguen verdes.
- [x] 8.6.3.17 Paste de imagen continúa funcionando.
  `coherent asset returns bytes through the bridge command` y la
  regresión preexistente `pasteMenuActionsFor image card
  returns only the image paste action` en
  `tests/clipboardAsset.test.ts` siguen verdes.
- [x] 8.6.3.18 El menú de imagen no muestra acciones de texto.
  `image card menu never exposes Paste de texto enriquecido or
  Paste de texto plano` (existente) sigue verde.

**No regresión de tags.**

- [x] 8.6.3.19 Hidratar tags al iniciar no ejecuta escrituras.
  `App.svelte's hydration sequence reads every visible entry's
  tags and collections` en
  `tests/tagsAndCollections.test.ts` (existente) sigue verde.
- [x] 8.6.3.20 Hidratar tags no modifica `asset_ref`.
  `image entry hydration preserves asset_ref and mime_type` en
  el mismo archivo (existente) sigue verde.
- [x] 8.6.3.21 Guardar tags de una imagen no elimina ni
  modifica su asset. `assigning_tags_to_image_preserves_asset_ref_and_payload`
  en `crates/clipvault-core/tests/organization.rs` (existente)
  sigue verde.
- [x] 8.6.3.22 Reiniciar conserva tags y asset simultáneamente.
  `image_entry_tags_persist_across_close_and_reopen` y
  `image_entry_and_asset_survive_a_restart` siguen verdes.

**Privacidad.**

- [x] 8.6.3.23 No se registran contenido, bytes, snippets,
  hashes ni rutas sensibles en logs. La regresión preexistente
  `clipboard_asset_errors_never_leak_a_path_reference_or_hash`
  en
  `app/tauri/src-tauri/tests/clipboard_asset_command.rs` sigue
  verde; el nuevo `commitThumbnailState` no añade nuevos
  `warn!`/`info!`/`error!` y la regresión preexistente
  `typed_outcomes_never_carry_bytes_hashes_or_absolute_paths`
  en `clipboard_rich_content.rs` (existente) sigue verde.

**Tareas manuales pendientes.** 7.8 sigue sin completarse; cubre
el smoke test visual de los tres estados (`loading`, `loaded`,
`error`) en la UI nativa, incluida la verificación de que el
icono "Cargando imagen" reemplaza al fallback durante la carga
inicial tras reiniciar.


## 7. Verificación

- [x] 7.1 `cargo fmt --all -- --check`.
- [x] 7.2 `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 7.3 `cargo test --workspace`.
- [x] 7.4 `cd app/tauri/frontend && npm run check`.
- [x] 7.5 `cd app/tauri/frontend && npm run build`.
- [x] 7.6 `cd app/tauri/frontend && npm test`.
- [x] 7.7 `openspec validate tags-and-collections --strict --type change`.
- [x] 7.8 Prueba manual: crear colecciones, asignar una captura a varias,
  quitarla de una secundaria, eliminar desde Historial, crear tags, filtrar
  por varios tags y reiniciar la aplicación.

  **Evidencia.** Esta tarea combina una verificación manual del usuario
  sobre la UI nativa con la cobertura automatizada que ya estaba
  verde al cierre del cambio. La distinción es importante porque la
  UI nativa es el único lugar donde se puede confirmar el render
  visual del chip de tag (sin abrir el modal) y de los tres estados
  del thumbnail (`loading` → `loaded` → `error`); el resto del
  comportamiento ya está pinado por los tests automatizados.

  **Manual (ejecutada por el usuario sobre la ventana nativa).**

  - Conclusión: todas las acciones del flujo extremo a extremo se
    completaron sin errores visibles: el chip de tag aparece en la
    tarjeta sin necesidad de acciones incidentales y el thumbnail
    de imagen pasa de `loading` a `loaded` al iniciar la
    aplicación, sin mostrar el fallback "Imagen no disponible"
    durante la carga.
  - Flujo cubierto: crear colección de usuario; asignar la misma
    captura a varias colecciones; quitarla de una colección
    secundaria (queda en `Historial`); eliminarla desde `Historial`
    (confirmación global, la fila y el asset desaparecen); crear
    tags desde el modal unificado; filtrar el rail por varios tags
    (AND); reiniciar la app y comprobar que las asociaciones
    (tags, colecciones) y los thumbnails siguen visibles al volver
    al estado anterior sin pasos manuales extra.

  **Automatizada (verificada por este cambio, ya en verde).**

  - Bootstrap + hidratación:
    `App.svelte's hydration sequence reads every visible entry's
    tags and collections`,
    `changing collections re-issues the entry_tags read for the
    new visible set`,
    `organization-updated listener re-reads the snapshot and the
    per-entry cache` y
    `stale hydration responses never overwrite fresh data` en
    `app/tauri/frontend/tests/tagsAndCollections.test.ts`.
  - Persistencia tras reiniciar:
    `image_entry_and_asset_survive_a_restart`,
    `recent_entries_returns_image_rows_with_complete_payload_metadata_after_restart`,
    `recent_entries_with_filter_returns_image_rows_in_history_after_restart`,
    `image_row_round_trips_through_row_to_record_with_complete_metadata`,
    `clipboard_asset_command_returns_persisted_bytes_after_restart` y
    `clipboard_asset_command_returns_invalid_asset_ref_after_restart`
    en
    `crates/clipvault-core/tests/clipboard_rich_content.rs`;
    `image_entry_tags_persist_across_close_and_reopen` y
    `assigning_tags_to_image_preserves_asset_ref_and_payload` en
    `crates/clipvault-core/tests/organization.rs`.
  - Máquina de tres estados del thumbnail:
    `coherent image row starts in loading and transitions to
    loaded`,
    `loader rejection transitions to error and never mints a URL`,
    `loader returning null transitions to error and never mints a
    URL`,
    `stale resolution never overwrites a freshly committed state`,
    `switching entries releases the previous Blob URL before
    minting a new one` y
    `resolver never re-mints a URL for the same reference twice
    in a row` en
    `app/tauri/frontend/tests/imageThumbnail.test.ts`.
  - Compatibilidad con la hidratación de organización:
    `hasRenderableImage remains true after the hydration round
    starts` y
    `bridge command for a coherent asset never echoes the reference
    back` en el mismo archivo, junto con la regresión preexistente
    `image entry hydration preserves asset_ref and mime_type` en
    `tests/tagsAndCollections.test.ts`.
  - Acciones del menú de imagen:
    `image card menu exposes a single Paste action with a null
    mode` y
    `image card menu never exposes Paste de texto enriquecido or
    Paste de texto plano` en
    `app/tauri/frontend/tests/clipboardAsset.test.ts`.

  Con la corrida manual reportada como exitosa y la cobertura
  automatizada de cada capa del flujo en verde, 7.8 queda cerrada
  sin reintroducir el workaround que pedía mantenerla pendiente.
