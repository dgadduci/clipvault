## Por qué

El desktop ya dispone de cards cuadradas y una organización por colecciones,
pero el panel de colecciones puede crecer sin límite, las acciones de
renombrado ocupan demasiado espacio y la información temporal y de tamaño de
las capturas no está visible. También falta un atajo directo para enfocar la
búsqueda.

La mejora debe hacer la interfaz más compacta y directa sin convertir las
acciones en controles inaccesibles ni alterar los contratos locales de
captura, SQLite, tags, colecciones, favoritos, imágenes o pegado.

## Qué cambia

- El panel de colecciones comparte la altura visual del rail de cards y su
  listado interno se desplaza verticalmente sin ensanchar ni alargar el
  desktop indefinidamente.
- La colección se renombra con doble clic sobre su nombre, con una alternativa
  de teclado y controles inline compactos; se elimina el botón textual
  Renombrar.
- El botón + Nueva se reemplaza por un icono y un formulario inline amplio
  con confirmación y cancelación mediante iconos, Enter y Escape.
- El título de una card se edita con doble clic; se elimina Editar título del
  menú y se mantiene la validación y persistencia actual.
- La barra de búsqueda acepta Cmd+F en macOS y Ctrl+F en Linux, mostrando el
  atajo de forma visible.
- Eliminar una card utiliza un icono rojo conservando confirmación y
  accesibilidad.
- Cada captura conserva y muestra el tiempo transcurrido desde su fecha
  original de captura.
- Las cards de texto muestran el total de caracteres y las de imagen el peso
  del payload en bytes, KB o MB.
- El rail se ordena por fecha de captura, de más reciente a más antigua.

## No objetivos

- No cambiar el modelo de tags o colecciones planas.
- No cambiar la semántica de Historial, favoritos, borrado, retención o
  búsqueda por colección.
- No crear una nueva galería para imágenes.
- No reemplazar created_at por updated_at o last_seen_at para mostrar la edad.
- No migrar, borrar, mover ni regenerar assets de imágenes como parte de esta
  mejora salvo que una auditoría demuestre una carencia real de schema.
- No agregar red, telemetría, dependencias innecesarias ni lógica de negocio a
  Svelte o Tauri.

## Capacidades

### Nuevas capacidades

- desktop-collection-card-polish: interacción compacta del panel de
  colecciones, edición directa, atajo de búsqueda y metadata visual de cards.

### Capacidades modificadas

- desktop-shell-layout: el panel y el rail mantienen límites de layout y
  controles compactos.
- clipboard-history-cards: títulos, acciones, orden cronológico y metadata
  visible se actualizan sin cambiar el payload.
- tags-and-collections: renombrado y listado se presentan inline sin alterar
  asociaciones ni la colección protegida Historial.

## Impacto esperado

- Frontend: OrganizationSidebar.svelte, HistoryCard.svelte,
  HistoryCardRail.svelte, DesktopToolbar.svelte, App.svelte y helpers puros
  de formato y atajos.
- Core/DB: auditoría de created_at, orden de consultas y tamaño persistido;
  sólo agregar cambios si el contrato actual no satisface los requisitos.
- Tauri: no debe recibir lógica de presentación ni crear un nuevo canal de
  datos para contadores que ya existen en EntryRecord.
- Tests: unitarios para formato, integración para orden/persistencia y
  frontend para interacción, carreras de hidratación y carga de imágenes.
