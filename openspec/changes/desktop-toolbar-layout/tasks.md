# Tareas de implementación

## 1. Relevamiento y límites

- [x] 1.1 Leer DesktopToolbar.svelte, App.svelte,
  OrganizationSidebar.svelte, HistoryCardRail.svelte y sus tests antes de
  modificar el layout.
- [x] 1.2 Confirmar que los callbacks existentes de búsqueda, modales y
  borrado se pueden reutilizar sin cambios de backend.
- [x] 1.3 Registrar cualquier contradicción encontrada entre este cambio y
  desktop-header-card-dnd o platform-permission-guidance antes de implementar.

## 2. Contenedor compartido de workspace

- [x] 2.1 Reorganizar el markup para que el panel de colecciones y la columna
  de búsqueda/cards sean hermanos dentro de un contenedor común.
- [x] 2.2 Renderizar una sola DesktopToolbar dentro de la columna derecha,
  antes de search-status y HistoryCardRail.
- [x] 2.3 Aplicar min-width: 0 a los niveles de grid/flex que contienen el
  rail y overflow controlado para impedir crecimiento horizontal del body.
- [x] 2.4 Hacer que ambas columnas se estiren a la misma altura visible sin
  introducir un segundo valor fijo incompatible con el rail.
- [x] 2.5 Mantener la altura cuadrada fija de las cards y el scroll horizontal
  del rail.

## 3. Panel de colecciones

- [x] 3.1 Ajustar OrganizationSidebar para ocupar la altura del workspace
  mediante stretch/height: 100%.
- [x] 3.2 Mantener el encabezado del panel visible y hacer que sólo el
  listado de colecciones use flex: 1, min-height: 0 y overflow-y: auto.
- [x] 3.3 Confirmar que agregar muchas colecciones no aumenta la altura del
  desktop ni crea scroll vertical en el body.
- [x] 3.4 Preservar selección, creación, doble click de renombrado, borrado y
  drag-and-drop sobre filas de colecciones.

## 4. Toolbar y menú de acciones

- [x] 4.1 Mantener el input de búsqueda, sus estados, filtros, debounce,
  atajo Cmd-F/Ctrl-F y hint visible.
- [x] 4.2 Reemplazar los botones visibles Development, Privacidad, Retención
  y Atajo por un único botón de puntos suspensivos con icono local.
- [x] 4.3 Implementar el menú con role menu, menuitem, aria-haspopup,
  aria-expanded, foco visible y labels accesibles.
- [x] 4.4 Conservar exactamente los callbacks actuales para abrir cada modal;
  no duplicar estado ni listeners.
- [x] 4.5 Cerrar el menú por Escape, click fuera, selección y destroy;
  restaurar el foco al botón cuando corresponda.
- [x] 4.6 Mantener el basurero como hermano visible a la derecha del menú,
  fuera del menú, con su confirmación y estilo de peligro actuales.

## 5. Responsive y compatibilidad visual

- [x] 5.1 Ajustar los breakpoints para que búsqueda, menú y basurero sigan
  siendo utilizables en ventanas estrechas.
- [x] 5.2 Evitar que el menú quede recortado por overflow del toolbar.
- [x] 5.3 Preservar el tema local, tipografía, rail horizontal, tamaños de
  cards, iconos, tags, pin y acciones de cada card.
- [x] 5.4 Confirmar que el cambio no altera el drag pointer fallback ni el
  hit-testing de filas scrolleables.

## 6. Tests de no-regresión

- [x] 6.1 Testear la posición única de DesktopToolbar dentro de la columna
  derecha y la ausencia de una toolbar global duplicada.
- [x] 6.2 Testear que el menú contiene exactamente cuatro acciones y que cada
  una dispara el modal existente.
- [x] 6.3 Testear apertura, Escape, click fuera, selección, retorno de foco,
  cleanup e idempotencia de listeners.
- [x] 6.4 Testear que el basurero está fuera del menú y conserva el flujo de
  confirmación.
- [x] 6.5 Testear altura compartida, min-width: 0, scroll de colecciones y
  ausencia de crecimiento horizontal/vertical del body.
- [x] 6.6 Testear búsqueda sobre la colección activa y el atajo Cmd-F/Ctrl-F.
- [x] 6.7 Testear imágenes almacenadas después de reinicio, hidratación de
  tags, búsqueda, cambio de colección, pin/unpin y drop.
- [x] 6.8 Testear que el drag-and-drop a colecciones, el fallback de puntero
  y sus indicadores no sufren regresiones.

## 7. Verificación

- [x] 7.1 Ejecutar cargo fmt --all -- --check.
- [x] 7.2 Ejecutar cargo clippy --workspace --all-targets -- -D warnings.
- [x] 7.3 Ejecutar cargo test --workspace.
- [x] 7.4 Ejecutar npm run check.
- [x] 7.5 Ejecutar npm run build.
- [x] 7.6 Ejecutar npm test.
- [x] 7.7 Ejecutar openspec validate desktop-toolbar-layout --strict.
- [x] 7.8 Revisar el diff y confirmar que no se incluyeron secretos,
  contenido de clipboard, archivos generados, red ni dependencias nuevas.

## 8. Verificación manual

- [ ] 8.1 En macOS, confirmar visualmente que la búsqueda está sobre las
  cards, el menú y el basurero están a su derecha y el panel de colecciones
  tiene exactamente la misma altura.
- [ ] 8.2 En macOS, abrir cada item del menú, cerrar con Escape y click fuera,
  y comprobar el retorno de foco.
- [ ] 8.3 En macOS, crear suficientes colecciones para activar el scroll sin
  que crezca la ventana ni aparezca overflow horizontal.
- [ ] 8.4 En macOS, confirmar imágenes previamente guardadas, tags, pin y
  drag-and-drop después de reiniciar.
- [ ] 8.5 En Linux X11 y Wayland cuando estén disponibles, verificar el
  reflow, atajo Ctrl-F, menú, scroll y ausencia de overflow.

La validación manual de platform-permission-guidance permanece separada y no
debe marcarse como completada por este cambio.