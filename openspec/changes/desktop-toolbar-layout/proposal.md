## Por qué

El desktop actual coloca la búsqueda y los controles globales en una barra
independiente por encima de todo el contenido. Esto separa visualmente la
búsqueda del rail de cards, deja al panel de colecciones con una altura
distinta y ocupa más espacio horizontal con cuatro botones de configuración
que pertenecen a acciones secundarias.

La referencia visual propone que la búsqueda sea el encabezado de la columna
de cards, que Development, Privacidad, Retención y Atajo se agrupen detrás de
un menú de puntos suspensivos, y que el basurero permanezca visible como
acción destructiva independiente a la derecha del menú.

## Qué cambia

- Mover la barra de búsqueda a la columna derecha, inmediatamente sobre el
  rail/listview horizontal de cards.
- Reemplazar los cuatro botones visibles por un único botón de puntos
  suspensivos con menú accesible.
- Mantener dentro del menú los accesos a Development, Privacidad, Retención y
  Atajo de pegado rápido, con sus callbacks y modales actuales.
- Colocar el icono de borrar a la derecha del menú, conservando su
  confirmación y su acción actual.
- Introducir un contenedor de trabajo común que alinee verticalmente el panel
  de colecciones con la columna de búsqueda y cards.
- Mantener el listado de colecciones dentro de un viewport scrolleable sin
  permitir que el desktop crezca por la cantidad de colecciones.
- Mantener el atajo visual Cmd-F/Ctrl-F dentro de la barra de búsqueda.
- Refluir la composición en ventanas estrechas sin ocultar acciones,
  recortar el input ni generar scroll horizontal en la página.

## No objetivos

- No cambiar la búsqueda local, sus filtros, ranking, debounce, límites ni
  selección de colección activa.
- No cambiar la semántica de Historial, colecciones, tags, favoritos,
  eliminación o retención.
- No modificar la captura, el almacenamiento o la carga de imágenes.
- No modificar el payload, el hit-testing ni el feedback del drag-and-drop.
- No cambiar quick-paste ni registrar listeners globales adicionales.
- No mover los contenidos de los modales ni duplicar sus comandos.
- No agregar dependencias, red, telemetría ni recursos visuales externos.

## Capacidades afectadas

### Capacidades modificadas

- desktop-shell-layout: nueva composición del workspace, barra de búsqueda,
  menú de acciones y alineación de alturas.
- clipboard-history-cards: el rail se mantiene en la columna derecha y
  conserva sus dimensiones y acciones.
- tags-and-collections: el panel conserva su viewport y sus operaciones;
  sólo cambia su relación geométrica con el rail.

## Impacto esperado

- Frontend: DesktopToolbar.svelte, App.svelte,
  OrganizationSidebar.svelte y, si hace falta, HistoryCardRail.svelte.
- Tests frontend: layout, menú, teclado, responsive y preservación de
  contratos.
- Tauri/core/DB: sin cambios esperados.
