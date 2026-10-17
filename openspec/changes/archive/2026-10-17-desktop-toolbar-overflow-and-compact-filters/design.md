# Diseño: desktop-toolbar-overflow-and-compact-filters

## Barra desplazable

- `DesktopToolbar` conserva una sola fila sin salto. Su viewport tiene ancho
  limitado por la columna de contenido, `min-width: 0` y `overflow-x: auto`;
  `overflow-y` permanece oculto para que el desplazamiento no expanda la
  página.
- La fila permanece enfocada por teclado para permitir desplazamiento con
  las teclas del navegador y admite desplazamiento horizontal con puntero y
  touch. El scroll no oculta ni elimina controles.
- El menú existente continúa anclado al viewport y no queda recortado por el
  viewport scrolleable.
- La barra no genera overflow horizontal a nivel de página ni cambia el rail
  horizontal, tamaño, contenido o interacciones de las cards.

## Anchos

- `SourceAppFilter` conserva el token actual de 220 px y mínimo de 11rem.
- `TagFilter` y `CollectionFilter` usan un token compacto independiente de
  110 px y mínimo de 5.5rem. La lista emergente de tags sigue coincidiendo con
  el ancho del control.
- La búsqueda pasa de la base actual de 240 px a 180 px (75 %), con mínimo de
  9rem y sin crecer para absorber el espacio libre. Así se reduce su tamaño
  visible y aumenta la posibilidad de acceder a la fila sin quitar acciones.
- Los controles mantienen sus anchos en vez de aplastarse cuando la fila no
  cabe; el usuario desplaza el viewport para llegar al menú y al botón de
  borrar.

## Atajo y localización

- El atajo dinámico de crear captura permanece dentro del botón, con
  presentación visual de badge compacto.
- Se mantienen `aria-keyshortcuts`, el nombre accesible, el tooltip y la
  activación actual. El badge es decorativo para tecnologías de asistencia.
- `toolbar.search.placeholder` usa una etiqueta breve equivalente a «Buscar»
  en los catálogos `en`, `es`, `pt`, `de` y `fr`; la etiqueta accesible de
  búsqueda sigue describiendo su alcance.

## Opciones de filtros

- Cada opción visible en la lista del filtro de tags expone su nombre
  completo en `title`, aunque el texto del item esté truncado por el ancho
  compacto.
- Las opciones del filtro de colecciones exponen el nombre completo con el
  atributo `title` del item nativo; la opción `Todas` muestra su propio texto.
- El tooltip usa el mismo texto visible de la opción, sin abreviarlo ni
  modificar el nombre persistido.

## Selector de tags de una captura

- El modal reinicializa su selección al abrirse. Si la selección persistida
  todavía se está hidratando, deshabilita solo las casillas y, al terminar,
  combina los IDs persistidos con los tags añadidos durante la carga. Las
  actualizaciones posteriores del snapshot no reemplazan la selección local;
  así, el tag creado durante la sesión permanece marcado.
- Cada fila incluye un botón de papelera con nombre accesible localizado. Al
  activarlo, la fila pide confirmación explícita de que el tag se eliminará de
  todas las capturas. Cancelar no cambia datos ni selección.
- Al confirmar, `TagSelectorModal` usa `tagsDeleteCommand`. El backend elimina
  la definición y sus asociaciones dentro de una transacción y emite el evento
  existente `clipvault://organization-updated` solo tras completarse el
  comando. El selector quita el ID de su selección local y del conjunto de
  tags creados localmente.
- `App.svelte` consume el evento para actualizar el snapshot y la metadata de
  todas las capturas cargadas. La opción desaparece de los filtros y, si era
  el tag activo del filtro global, ese filtro vuelve a `Todas` y recarga el
  historial.
- Un error conserva la definición, las asociaciones y el modal abierto; se
  muestra un mensaje localizado y se permite reintentar.

## Verificación

- Revisar visualmente que una ventana estrecha conserva una fila horizontal
  desplazable, que todos los controles siguen alcanzables y que no aparece
  overflow horizontal de página.
- Confirmar los anchos de los tres filtros afectados, la etiqueta breve y el
  badge del atajo.
- Hover sobre opciones largas de tags y colecciones y confirmar que se ve su
  nombre completo.
- Crear un tag con el modal abierto, comprobar que continúa marcado después
  del refresh, y probar cancelar/confirmar la eliminación en varias capturas.
- Ejecutar los checks y build de frontend, validación estricta OpenSpec y
  regresiones aplicables de drag-and-drop antes de entregar.
