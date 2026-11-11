# Design: collection-row-actions

## Row presentation

- Mantener el botón de selección y el área de drop como el contenido principal
  de cada fila. Aplicar el fondo translúcido al contenedor de la fila usando
  únicamente el color HEX validado de su colección y una opacidad fija de
  `0.18`.
- Mantener el énfasis de selección y el borde de drop sobre el tinte de base;
  el feedback temporal de drop puede añadir el acento actual.
- Mantener el nombre y, cuando corresponda, los badges `system` o `imported`
  junto con el contador. A continuación mostrar el icono de nota y luego el
  botón de menú.

## Actions menu

- Renderizar un botón de puntos suspensivos por fila con `aria-haspopup`,
  `aria-expanded`, nombre accesible localizado y un menú asociado.
- El menú muestra la acción local existente para cambiar color. Sólo las
  colecciones de usuario muestran eliminar. `Historial` conserva su protección
  contra eliminación. El icono de nota vive en la fila y abre el editor
  compartido; su apariencia distingue si la colección ya tiene una nota.
- Reutilizar `CollectionColorModal`, `TextNoteModal`, los eventos existentes y
  el flujo de confirmación de borrado de `App.svelte`; no añadir lógica de
  persistencia en el sidebar.
- Cerrar con Escape, al elegir una acción, al mover el foco fuera del menú o al
  hacer click fuera. Escape devuelve el foco al botón; un click fuera conserva
  el foco en el control elegido. Al abrir un modal, el modal recibe el foco; el
  editor de nota devuelve el foco al icono de nota de su fila.
- Mantener el menú dentro del viewport desplazable de colecciones y conservar
  los listeners singleton y los contratos de drop existentes.

## Localization and tests

- Agregar sólo la clave traducida necesaria para nombrar el botón de menú en
  los cinco catálogos. Reutilizar las traducciones de acciones existentes.
- Ajustar pruebas de fuente que dependan de los botones separados y cubrir el
  orden de controles de la fila, las acciones disponibles por tipo de
  colección, el tinte seguro de color y el cierre/teclado del menú.
- Ejecutar las pruebas frontend relevantes, incluidas las regresiones de
  drag-and-drop, validación de locales, check, build y validación estricta de
  OpenSpec.
