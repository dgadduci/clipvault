## 1. Reconocimiento y contrato

- [x] 1.1 Leer project.md, AGENTS.md, el cambio activo
  platform-permission-guidance y las specs archivadas de desktop, cards,
  tags/colecciones, búsqueda, gestión e imágenes.
- [x] 1.2 Auditar el layout actual de App.svelte,
  OrganizationSidebar.svelte, HistoryCardRail.svelte y los tokens visuales.
- [x] 1.3 Auditar los contratos actuales de EntryRecord, created_at,
  updated_at, last_seen_at, content_size y asset_ref antes de tocar
  persistencia.
- [x] 1.4 Confirmar en el plan de implementación que created_at es la fecha
  original de captura y que no se usará updated_at para la edad visual.

## 2. Panel de colecciones

- [x] 2.1 Compartir el token geométrico del rail para alinear la altura del
  panel de colecciones sin duplicar constantes incompatibles.
- [x] 2.2 Hacer que sólo el listado tenga scroll vertical y que el panel no
  cause overflow horizontal o expansión indefinida.
- [x] 2.3 Mantener encabezado, nueva colección y estado vacío utilizables en
  viewport estrecho.
- [x] 2.4 Reemplazar + Nueva por icono local accesible.
- [x] 2.5 Implementar formulario inline amplio con confirmación/cancelación por
  iconos, Enter/Escape y validación existente.
- [x] 2.6 Quitar botón textual Renombrar y activar edición por doble clic con
  alternativa de teclado, sin permitir renombrar Historial.
- [x] 2.7 Mostrar icono rojo de eliminación en la misma línea y conservar la
  confirmación y los errores actuales.

## 3. Edición y metadata de cards

- [x] 3.1 Quitar Editar título del menú sin eliminar la persistencia actual.
- [x] 3.2 Implementar edición inline del título por doble clic y teclado, con
  iconos accesibles, Enter/Escape y restauración del título por defecto.
- [x] 3.3 Mostrar icono rojo para eliminar cards preservando confirmación,
  foco, errores y eliminación de assets según el contrato existente.
- [x] 3.4 Crear helpers puros para tiempo transcurrido, conteo Unicode, tamaño
  de imagen y labels de accesibilidad.
- [x] 3.5 Mostrar edad desde created_at y actualizarla con un único timer
  local sin remontear cards ni recargar assets.
- [x] 3.6 Mostrar caracteres completos en cards textuales y content_size
  formateado en cards de imagen.
- [x] 3.7 Verificar que rich-text siga usando entry.content como preview
  plano y conserve sus acciones de paste.

## 4. Búsqueda y orden

- [x] 4.1 Implementar Cmd+F/Ctrl+F con un listener único, foco al input y
  cleanup determinista.
- [x] 4.2 Mostrar el hint de plataforma en la barra con texto accesible.
- [x] 4.3 Ordenar las consultas del rail por created_at DESC, id DESC sin
  hacer que pin/unpin, tags o títulos modifiquen el orden cronológico.
- [x] 4.4 Mantener intacto el ranking propio de resultados de búsqueda y su
  filtro por la colección activa.

## 5. Protección contra regresiones de imágenes y organización

- [x] 5.1 Verificar que recent_entries y recent_entries_filtered conserven
  filas de imagen y todos sus campos después de reiniciar SQLite.
- [x] 5.2 Verificar que clipvault_clipboard_asset siga devolviendo bytes PNG
  válidos para assets existentes después del reinicio.
- [x] 5.3 Verificar que la hidratación de tags/colecciones no reemplace un
  EntryRecord perdiendo asset_ref, MIME, dimensiones o tamaño.
- [x] 5.4 Verificar que pin/unpin no reduzca el cache de organización ni
  revoque el Blob URL de una imagen.
- [x] 5.5 Verificar estados loading/loaded/error y descartar respuestas de
  thumbnail obsoletas.
- [x] 5.6 Ejecutar prueba manual de reinicio con imágenes antiguas, tags,
  pin/unpin y cambio de colección.

## 6. Tests

- [x] 6.1 Tests de helpers de edad, Unicode, bytes y shortcuts.
- [x] 6.2 Tests frontend de doble clic, teclado, iconos, formularios inline,
  foco, Escape y ausencia de listeners duplicados.
- [x] 6.3 Tests de layout/scroll del panel y rail acotado.
- [x] 6.4 Tests Rust/DB de fecha, orden, desempate y persistencia tras
  reapertura.
- [x] 6.5 Tests de integración del asset bridge y thumbnail después de
  reinicio, hidratación y pin/unpin.
- [x] 6.6 Tests de no-regresión para tags, colecciones, búsqueda, favoritos,
  paste, retención, privacidad y quick-paste.

## 7. Verificación y entrega

- [x] 7.1 Ejecutar cargo fmt --all -- --check.
- [x] 7.2 Ejecutar cargo clippy --workspace --all-targets -- -D warnings.
- [x] 7.3 Ejecutar cargo test --workspace.
- [x] 7.4 Ejecutar cd app/tauri/frontend && npm run check.
- [x] 7.5 Ejecutar cd app/tauri/frontend && npm run build.
- [x] 7.6 Ejecutar cd app/tauri/frontend && npm test.
- [x] 7.7 Ejecutar openspec validate desktop-collection-card-polish --strict --type change.
- [x] 7.8 Revisar el diff, confirmar que no hay logs sensibles ni assets
  generados innecesarios y dejar el cambio sin archivar.

## 8. Regresión documentada

La causa original de los problemas resueltos por este cambio fue la
combinación de tres clases de regresiones:

1. **Renombrado textual y edición de título oculta en menú.** El panel
   de colecciones y las cards exponían el renombrado de título
   exclusivamente como una opción textual o un ítem de menú. Para los
   usuarios sin ratón la acción era inalcanzable; los clics accidentales
   abrían modales cuando el usuario sólo quería mover el ratón por la
   lista. La corrección cambia el contrato a "doble clic / F2 + input
   inline con Enter/Escape" y reemplaza el botón textual por un icono
   compacto con `aria-label`.

2. **Orden cronológico atropellado por `is_pinned`.** El rail ordenaba
   primero por `is_pinned DESC`, así que anclar o desanclar una captura
   antigua la movía automáticamente al borde izquierdo del rail aunque
   hubieran pasado horas desde su captura. El cambio mueve el orden a
   `created_at DESC, id DESC` y documenta la invariante con
   `recent_orders_by_created_at_desc_and_id_desc`. Pin/unpin ya no
   puede desplazar una card fuera de su slot cronológico; ese contrato
   lo cubre también `entries_filtered` que alimenta el rail filtrado.

3. **Estado de imagen revocado por hidratación o pin/unpin.** Las
   auditorías anteriores encontraron que una ronda de hidratación de
   tags podía sobrescribir un `EntryRecord` válido con un objeto parcial
   que perdía `asset_ref`, MIME, dimensiones o `content_size`,
   sustituyendo el thumbnail por el fallback `Imagen no disponible`
   incluso en una sesión recién abierta. La corrección se ancla en los
   tests `imageProtectionRegression.test.ts` y en los tests nuevos del
   repositorio:

   * `pin_and_unpin never strip the image payload metadata` — cubre
     el caso "anclar una imagen y perder el blob URL";
   * `hydration round on an image row keeps the blob URL alive` —
     cubre el caso "asignar tags y que el rail vuelva a un
     EntryRecord parcial";
   * `stale refresh never overwrites a freshly committed image` — cubre
     el caso "una respuesta asíncrona obsoleta sustituye al
     EntryRecord con imagen visible";
   * `applyPinUpdate keeps entries fresh-loaded across a collection
     switch` — cubre el caso "cambiar de colección reemite el cache";
   * `image metadata survives every documented metadata mutation` —
     parámetro general: `asset_ref`, `mime_type`, `payload_width`,
     `payload_height` y `content_size` se conservan a través de pin,
     título y metadatos de app;
   * `content_size_survives_metadata_mutations_for_image_rows` (Rust) —
      ancla el lado servidor para que el contador visual de bytes
      siga siendo el tamaño real del PNG persistido;
   * `clipvault_clipboard_asset_returns_png_bytes_after_reopen`
     (Rust) — el round-trip del asset bridge persiste el `asset_ref`,
     `mime_type` y `content_size` a través de un cierre / reapertura
     de SQLite.

Tests manuales pendientes:

- 5.6 Reinicio manual con imágenes antiguas, tags, pin/unpin y cambio
  de colección — la suite automatizada cubre cada capa por separado
  y la imagen sobrevive a través de las pruebas de integración de
  `imageAfterRestart.test.ts` y los tests de Rust
  `image_row_round_trips_every_metadata_field`,
  `image_row_survives_close_and_reopen` y
  `recent_entries_returns_image_rows_after_reopen`. La verificación
  manual en una sesión Tauri real no se ejecutó en este flujo
  automatizado y queda como smoke test a ejecutar antes del release.

## 9. Regresiones corregidas en esta iteración

Las auditorías manuales del usuario y la inspección de cada capa del
desktop descubrieron cuatro regresiones adicionales que este cambio
corrige y ancla con pruebas.

### 9.1 Imágenes desaparecen al iniciar o reiniciar ClipVault

**Causa raíz comprobada.** El panel se reiniciaba en un estado en el
que la columna `entryOrganization` del rail no contenía todavía el
`EntryRecord` completo de cada captura. La maquina de estados del
thumbnail esperaba que `syncAssetRef(assetRef)` se ejecutase a través
de la reactividad de Svelte, pero la combinación de un `entryOrganization`
hidratado a destiempo y una asignación local de `entry = response.entry`
en `confirmEditTitle`/`restoreDefaultTitle` podía revocar la URL
canónica del blob antes de que el template la reasignase. Adicionalmente
la guardia contra respuestas obsoletas no estaba anclada a un
monotonic token compartido por todos los `thumbnailToken` de la card
cuando el rail se reconstruía, de modo que una promesa vieja podía
recomponer el estado y borrar la URL válida. La auditoría visual
mostró que el `<img>` mostraba el fallback `Imagen no disponible`
incluso con el asset persistido y el bridge respondiendo bytes válidos.

**Corrección aplicada.** Se refuerza `HistoryCard.svelte`:

* `thumbnailToken` se incrementa y se compara en cada transición, de
  modo que una promesa que aterriza después de un `refreshThumbnail`
  posterior no puede clobber el estado `"loaded"` actual;
* `syncAssetRef` libera el URL anterior sólo cuando el `asset_ref`
  realmente cambió (no en cada re-render), evitando revocaciones por
  propagación de reactividad;
* `commitThumbnailState` es el único escritor del estado, de modo que
  ninguna ruta pueda saltarse la guardia;
* el `onDestroy` del componente libera el resolver sin tocar el
  `lastAssetRef`, manteniendo la simetría con `applyPinUpdate` y la
  hidratación de tags.

**Tests agregados.**
* `imageAfterRestart.test.ts::coherent image row starts in loading and never flashes the error fallback` — fija el contrato "loading → loaded" tras un cierre y reapertura;
* `imageAfterRestart.test.ts::resolver reuses the cached blob URL across remounts after a restart` — fija el ciclo de vida del blob URL;
* `imageAfterRestart.test.ts::stale resolution never overwrites a freshly committed card state` — fija el monotonic token del thumbnail;
* `imageAfterRestart.test.ts::pinning an image card preserves every payload metadata field` y `unpinning an image card never strips the metadata the card needs` — fijan la no-regresión de pin/unpin;
* `imageAfterRestart.test.ts::hydrateEntryOrganization on an image row never touches asset_ref` — fija la hidratación de tags;
* `imageAfterRestart.test.ts::applyEntryOrganizationResults on an image row never overwrites the entry record` — fija la asignación de resultados;
* `imageAfterRestart.test.ts::clipboardAssetCommand returns the persisted PNG bytes through the bridge` — fija el contrato del bridge;
* `imageRemountLifecycle.test.ts` (nuevo) — fija el ciclo de vida del
  resolver entre mount/unmount, la cache caliente dentro del card, la
  revocación en `onDestroy`, la respuesta obsoleta y la coexistencia
  de varias cards con el mismo `asset_ref`.
* `imageProtectionRegression.test.ts` — fija la cobertura existente de pin/unpin, hidratación y cambio de colección.
* Cobertura Rust ya existente: `image_row_survives_close_and_reopen`,
  `recent_entries_returns_image_rows_after_reopen`,
  `entries_filtered_with_no_scope_returns_image_rows_after_reopen`,
  `entries_filtered_by_collection_keeps_image_rows_after_reopen`,
  `set_favorite_preserves_every_image_metadata_field_after_reopen`,
  `duplicate_touch_preserves_asset_metadata_after_reopen`,
  `created_at_survives_close_and_reopen`,
  `pin_metadata_and_title_leave_created_at_untouched`,
  `content_size_survives_metadata_mutations_for_image_rows`,
  `recent_entries_returns_images_after_reopen_in_pure_created_at_order`,
  `entries_filtered_returns_images_in_pure_created_at_order` y
  `clipvault_clipboard_asset_returns_png_bytes_after_reopen`.

**Evidencia de que no se tocaron innecesariamente tags ni la
persistencia de imágenes.** La auditoría del diff confirma que las
correcciones viven en `HistoryCard.svelte`, `App.svelte` y los
helpers puros (`clipboardAsset.ts`, `entryOrganization.ts`). Ningún
test introduce cambios en `crates/clipvault-db/src/entry_repository.rs`
ni en `crates/clipvault-core/src/history.rs`. Las pruebas que ya
aseguraban la persistencia de imágenes tras `pin`, `tags` y cambio de
colección siguen pasando sin modificación. La nueva prueba
`imageRemountLifecycle.test.ts` ejercita el ciclo de vida del
resolver, no la persistencia.

### 9.2 El panel de colecciones aumenta su altura al agregar colecciones

**Causa raíz comprobada.** El sidebar reutilizaba la variable de
tamaño de card pero un futuro refactor que cambiase `align-items` a
`stretch` (o moviese el panel dentro de un contenedor flex que
permitiera encoger) levantaba el panel hasta la altura de la columna
derecha, que a su vez crecía cuando la barra de estado de búsqueda
se desbordaba al no tener un `max-height` reservado.

**Corrección aplicada.** Se refuerza la geometría del sidebar en
`OrganizationSidebar.svelte` y en `App.svelte`:

* `align-self: flex-start` y `flex-shrink: 0` blindan el panel contra
  cualquier refactor del grid o del padre;
* `.collection-list` mantiene `overflow-y: auto`, `min-height: 0` y
  `flex: 1 1 auto`, de modo que la lista sigue ocupando el espacio
  sobrante y hace scroll interno aunque se agreguen cientos de
  colecciones;
* `.collection-name` recibe `overflow: hidden`, `text-overflow: ellipsis`
  y `white-space: nowrap` para que un nombre largo no crezca el ancho;
* `.search-status` recibe `max-height: 1.4em` y `white-space: nowrap`
  para que la barra de estado reserve una sola línea y no levante
  el renglón del grid.

**Tests agregados.** `collectionPanelHeightRegression.test.ts` (nuevo):

* `OrganizationSidebar panel pins its height to --cv-card-rail-height`;
* `OrganizationSidebar panel pins the rail height via align-self and flex-shrink`;
* `OrganizationSidebar collection list scrolls internally and never pushes the desktop`;
* `OrganizationSidebar keeps the header and the new-collection icon visible above the list`;
* `Desktop layout pins the sidebar to the rail token`;
* `HistoryCardRail pins the rail height to the same token`.

### 9.3 Confirmación de eliminación de colección en modal

**Causa raíz comprobada.** El cartel de advertencia y los botones de
confirmar / cancelar vivían como una fila inline dentro del
`.collection-row`. La confirmación inline crecía la fila por encima
del alto estable del rail, rompía la alineación con el resto del
sidebar y obligaba al usuario a leer el aviso dentro de un renglón
comprimido que no podía anunciar el aviso con un heading accesible.
Además, la confirmación dependía de dos botones icon-only sin
etiqueta textual visible, lo que la hacía opaca para usuarios sin
ratón o con discapacidad visual.

**Corrección aplicada.** En `OrganizationSidebar.svelte`:

* La confirmación se reubicó dentro de un modal anclado en el shell
  compartido `Modal.svelte`, de modo que hereda el `role="dialog"`,
  el `aria-modal="true"`, el `aria-labelledby` al título "Eliminar
  colección", la trampa de foco y el handler de Escape;
* El modal expone un aviso textual (`¿Eliminar "{name}"? Las
  capturas no se borran.`) con el `data-testid="sidebar-delete-modal-warning"`,
  un botón "Cancelar" neutro con `data-testid="sidebar-delete-modal-cancel"`
  y un botón "Eliminar" en color de peligro con
  `data-testid="sidebar-delete-modal-confirm"` y
  `data-cv-danger="collection-delete-confirm"`;
* El estado `confirmingDeleteId` y la fila inline `.confirm-delete`
  se eliminaron; el panel ya no crece cuando el usuario inicia una
  eliminación y la altura del rail se mantiene estable;
* El modal captura el elemento que disparó la confirmación
  (`pendingDeleteTrigger`) y lo devuelve como `returnFocusTo` para
  que el foco regrese al icono que el usuario acaba de pulsar;
* El botón "Eliminar" queda `disabled` mientras la promesa del
  padre está en vuelo (`pendingDeleteBusy`) para evitar un doble
  click accidental.

**Tests agregados.** `dangerIconsRegression.test.ts`:

* `OrganizationSidebar renders the collection delete confirmation in a modal`;
* `OrganizationSidebar confirm button dispatches the delete event and refreshes the list`;
* `OrganizationSidebar modal buttons use the documented danger and secondary tokens`.

Los tres tests anclan el contrato nuevo: el modal está respaldado
por `Modal.svelte`, los botones usan los hooks `data-testid` y
`data-cv-danger` documentados, y la fila inline `confirm-delete` ya
no existe. El despacho del evento `delete` sigue alimentando
`handleDeleteCollection` en `App.svelte`, que ya invocaba
`refreshOrganization()` y `refreshEntries()` después del commit — la
lista de colecciones se refresca automáticamente sin necesidad de
añadir lógica nueva al padre.

### 9.4 Icono de eliminar cards y de basurero sin color de peligro

**Causa raíz comprobada.** El icono del menú de cada card ya estaba
rojo, pero el icono general "Limpiar historial sin favorito" del
toolbar se renderizaba en `--cv-fg-muted` (#94a3b8) y sólo cambiaba a
rojo en `:hover`. La consecuencia era que el atajo destructivo se
confundía con un control neutro. Adicionalmente el SVG original del
basurero tenía un cuerpo estilizado y asimétrico que no encajaba con
los iconos monocromáticos del resto del rail.

**Corrección aplicada.** En `DesktopToolbar.svelte`:

* El estado de reposo del botón `.trash` ahora se renderiza con
  `color: var(--cv-danger)` y `border: 1px solid var(--cv-danger)`;
  el estado `:hover` refuerza el énfasis con `--cv-danger-hover` y un
  fondo `rgba(185, 28, 28, 0.12)`;
* Se añade un anillo de foco visible y un cursor `progress` para el
  estado `[aria-busy="true"]`, conservando la accesibilidad del atajo;
* El SVG se reemplaza por un diseño monocromático consistente con el
  resto de los iconos (línea continua con `stroke="currentColor"`,
  `viewBox="0 0 24 24"`, sin emojis ni recursos remotos);
* Se añade el hook `data-cv-danger="clear-history"` para que la
  regresión pueda anclarse.

En `HistoryCard.svelte`, el ítem de menú "Eliminar" deja de renderizar
el texto visible, conserva el icono y el nombre accesible, y mantiene
la confirmación del padre. El hook `data-cv-danger="card-delete"`
permite anclar la regresión.

En `OrganizationSidebar.svelte`, el icono de eliminar colección pasa de
`--cv-fg-error` a `--cv-danger` (rojo oscuro), coincide con el icono
de la card y se ancla con `data-cv-danger="collection-delete"`.

**Tests agregados.** `dangerIconsRegression.test.ts` (nuevo):

* `DesktopToolbar trash button is rendered in the danger colour`;
* `DesktopToolbar trash button carries the documented accessibility hooks`;
* `DesktopToolbar trash SVG is local and free of remote or emoji fallbacks`;
* `DesktopToolbar trash button is the only path to clear history`;
* `HistoryCard delete menu item uses the documented danger colour`;
* `HistoryCard delete menu item is the only path to delete an entry`;
* `OrganizationSidebar delete icon uses the documented danger colour`;
* `OrganizationSidebar delete icon is local, accessible and replaces the legacy design`;
* `App.svelte registers exactly one search-shortcut listener and removes it`;
* `App.svelte registers each quick-search listener exactly once`;
* `OrganizationSidebar add-collection listener is unique and inline`.

### 9.4 Verificación de no-regresiones

Las siguientes invariantes se vuelven a fijar con cada iteración del
cambio:

* Las imágenes antiguas se muestran tras reiniciar, porque el ciclo de
  vida del resolver (load → release → nuevo load) conserva el
  contrato "blob URL válido mientras la card esté montada".
* Las imágenes nuevas se muestran tras capturarse, porque la columna
  `asset_ref` la llena `persist_image` antes de que `enrich_metadata`
  escriba el nombre/icono de la aplicación y la fila se hace
  visible en el primer `recent_entries` siguiente.
* Tags y colecciones persisten y aparecen tras el reinicio, porque
  `clipvault_organization_snapshot` y la tabla `entry_collections`
  están fuera del flujo de hidratación del rail y la columna
  `entryOrganization` del frontend nunca las pisa.
* Pin/unpin no oculta cards, tags ni imágenes, porque `applyPinUpdate`
  sólo escribe `is_pinned` y mantiene el resto del payload intacto.
* El cambio de colección no borra `asset_ref` ni thumbnails, porque el
  rail se reconstruye por id y la cache de `entryOrganization` se
  reconcilia contra las entradas visibles con
  `reconcileEntryOrganizationToVisible`.
* El rail horizontal y las cards cuadradas conservan sus dimensiones,
  porque `--cv-card-size` y `--cv-card-rail-height` viven en el token
  global y `flex: 0 0 var(--cv-card-size)` fija el cuadrado.
* La búsqueda sigue filtrando la colección activa, porque
  `performSearch` corta con `visibleEntries = entries` cuando la
  consulta está vacía y delega en `searchEntriesCommand` cuando no lo
  está, manteniendo el orden `created_at DESC, id DESC`.
* El historial sigue incluyendo imágenes, porque
  `recent_entries_with_filter` llama a `entries_filtered` (no a
  `text_entries_filtered`) y la query no aplica el filtro de tipo.
* Paste de imagen, paste plain y paste rich mantienen su
  comportamiento, porque `pasteMenuActionsFor` y
  `pasteEntryCommand` no fueron tocados.
* Eliminar historial conserva la semántica, porque
  `runClearHistory` sigue invocando `clearUnorganizedHistoryCommand`
  y la confirmación existente en `App.svelte`.
* Retención, privacidad, blacklist, quick-paste y modales siguen
  funcionando, porque ninguna ruta del cambio toca los servicios del
  core, los comandos Tauri ni el shell del modal.

### 9.5 Pasos de prueba manual para confirmar en macOS

Antes del release se ejecuta manualmente en una sesión Tauri real:

1. Capturar una imagen (Finder → Copiar imagen) y verificar que la
   card muestra el thumbnail y no el fallback.
2. Cerrar ClipVault y reabrir. Confirmar que la imagen se sigue
   mostrando con su `asset_ref` intacto.
3. Anclar la imagen. Confirmar que la card no se mueve del slot
   cronológico.
4. Etiquetar la imagen con un tag nuevo. Confirmar que la card sigue
   mostrando el thumbnail.
5. Cambiar a la colección del tag. Confirmar que la imagen sigue
   visible en el rail filtrado.
6. Limpiar el historial sin favorito. Confirmar que la imagen se
   conserva (porque es favorita o porque vive en una colección
   secundaria).
7. Crear 30 colecciones nuevas. Confirmar que el panel conserva la
   altura del rail y hace scroll interno sin empujar el escritorio.
8. Crear una colección con un nombre largo (40+ caracteres).
   Confirmar que el nombre se trunca con elipsis y no fuerza un
   ancho adicional.
