## Por qué

El desktop todavía reserva espacio para una línea de título y subtítulo que no
aporta al flujo principal. Además, las cards sólo pueden asociarse a
colecciones mediante el menú, la ventana conserva una altura mínima mayor a la
necesaria y el control visual de favoritos usa una estrella en lugar del pin
que representa la acción.

## Qué cambia

- Eliminar la línea de título de la aplicación y todos sus textos asociados,
  manteniendo los estados de error, carga y accesibilidad necesarios.
- Permitir arrastrar una card a una colección de usuario para agregar la
  asociación usando el flujo existente de colecciones.
- Mantener el panel de colecciones y el rail dentro de límites acotados, sin
  crecimiento vertical por la cantidad de colecciones.
- Reducir la altura mínima de la ventana al mínimo que permite mostrar el
  desktop usable.
- Al iniciar, ubicar la ventana principal centrada horizontalmente en el
  monitor primario y en la parte superior del área de trabajo.
- Reemplazar la estrella por un icono local de pin, sin cambiar la semántica
  persistente de favorito.

## No objetivos

- No cambiar la semántica de Historial, tags, colecciones, favoritos o
  eliminación.
- No quitar el acceso existente de asignación de colecciones desde la card.
- No implementar reordenamiento manual de cards mediante drag.
- No mover una card fuera de Historial al soltarla en otra colección.
- No incluir contenido del clipboard en el payload de drag, logs o eventos.
- No modificar la persistencia de imágenes ni sus referencias locales.
- No agregar red, telemetría, dependencias innecesarias ni nuevos formatos de
  clipboard.

## Capacidades

### Nuevas capacidades

- desktop-header-card-dnd: encabezado compacto, drag-and-drop de cards a
  colecciones, geometría inicial de ventana y control visual de pin.

### Capacidades modificadas

- desktop-shell-layout: desktop sin encabezado redundante, altura mínima
  acotada y posición inicial determinista.
- tags-and-collections: asociación adicional mediante drag-and-drop, sin
  alterar la relación many-to-many ni la colección Historial.
- clipboard-history-cards: icono de pin y atributo draggable conservando
  acciones y metadata existentes.

## Impacto esperado

- Frontend: App.svelte, OrganizationSidebar.svelte, HistoryCard.svelte,
  HistoryCardRail.svelte y helpers puros de drag and drop.
- Tauri: sólo configuración y coordinación de ventana si hace falta; no debe
  recibir contenido de clipboard ni lógica de presentación.
- Core/DB: reutilizar entry_collections_set y las invariantes existentes; no se
  espera una migración.
- Tests: frontend para drag/drop, teclado, pin y layout; integración Rust/DB
  para asociaciones, Historial e imágenes.
