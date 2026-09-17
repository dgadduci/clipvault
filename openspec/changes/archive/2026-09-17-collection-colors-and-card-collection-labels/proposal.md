# Propuesta: colores de colecciones y membresía visible en cards

## Problema

Las colecciones actuales sólo exponen nombre y tipo. La lista lateral no tiene
una señal visual rápida para distinguirlas y las cards muestran tags, pero no
informan a qué colecciones pertenece cada captura. Además, el modelo de
colecciones no persiste un color que pueda reutilizarse de forma consistente en
la sidebar y en las cards.

## Objetivo

Agregar un color persistente a cada colección, asignar automáticamente uno de
cuatro colores al crear una colección y permitir cambiarlo desde un modal de
selección de color. Las cards deben mostrar, debajo de las tags, todas las
colecciones de la captura usando el color configurado en cada etiqueta.

## Alcance

- Extender `Collection`, SQLite, el servicio de organización y los comandos
  Tauri con un color HEX opaco normalizado.
- Asignar un color aleatorio de la paleta rojo, amarillo, verde o azul a cada
  nueva colección de usuario.
- Dar un color inicial a `Historial` y a colecciones existentes durante la
  migración, sin perder datos ni memberships.
- Mostrar un cuadrado de color junto al nombre de cada colección.
- Abrir un modal mediante doble click sobre el cuadrado, con selector visual,
  guardar, cancelar, Escape y foco accesible.
- Mostrar las colecciones asignadas a cada captura debajo de las tags con chips
  visualmente consistentes con las tags y el color de texto de cada colección.
- Cuando los chips no entren en el ancho de la card, mostrar un chip `+N` con
  la cantidad pendiente; al activarlo debe abrir un modal con el listado
  completo de colecciones de esa captura.
- Mantener el funcionamiento local/offline, los comandos thin y el payload de
  drag and drop limitado al identificador opaco de la entrada.

## Fuera de alcance

- Colores para tags, entradas, aplicaciones o tipos de clipboard.
- Transparencia, gradientes, múltiples colores por colección o temas de color.
- Reordenamiento, jerarquías o cambios en la semántica de memberships.
- Cambios en Quick Paste, búsqueda, captura, pegado, assets o drag and drop.
- Uso de APIs nativas de Wayland, X11 o macOS para abrir el selector.

## Capacidades afectadas

- `tags-and-collections`: color persistente, color editable y paleta inicial.
- `clipboard-history-cards`: etiquetas de membresía de colecciones debajo de
  las tags.

## Criterio de aceptación

Una colección nueva recibe uno de los cuatro colores base y conserva ese valor
tras reiniciar. El usuario puede editarlo desde la sidebar y ver el cambio en
las cards asociadas. Cada card muestra sus colecciones de usuario debajo de
las tags; `Historial` no se muestra como chip inline. Si los chips no entran,
un chip `+N` abre un modal con todas las memberships, incluida `Historial`. El
flujo mantiene la geometría de las cards y no introduce regresiones en
Wayland, X11, macOS ni en drag and drop. La aceptación del overflow requiere
verificar el comportamiento en una card real: la medición debe comparar el
conjunto completo de chips con el ancho disponible, incluso cuando la fila
visible ya está recortada, y el `+N` debe aparecer en la ejecución de Tauri.
