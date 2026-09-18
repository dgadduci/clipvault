# Proposal: quick-paste-text-editing

## Contexto

ClipVault ya permite editar una captura textual desde el menú de una card del
historial principal. La edición usa el core Rust y persiste el nuevo contenido
en la misma fila de SQLite. Sin embargo, la ventana transitoria Quick Paste
que el usuario utiliza para buscar y reutilizar capturas sólo ofrece acciones
de copia y `Previsualizar`; para editar hay que abandonar ese flujo y abrir el
desktop principal.

Además, el menú de Quick Paste no muestra los atajos de `Editar captura` ni de
`Previsualizar`, aunque ambas acciones ya tienen contratos platform-aware en
el desktop y Quick Paste ya reconoce el atajo de previsualización.

## Objetivo

Permitir que una captura textual elegible se edite directamente desde el menú
de su resultado Quick Paste, guardando la modificación de forma persistente y
reflejándola en la lista, la búsqueda, el historial principal y el siguiente
uso de Quick Paste. El menú debe mostrar de forma clara y accesible:

- `Editar captura` con `⌘E` en macOS y `Ctrl+E` en Linux;
- `Previsualizar` con `⌘Enter` en macOS y `Ctrl Enter` en Linux.

Las etiquetas, los matchers, los valores `aria-keyshortcuts` y el
comportamiento deben provenir de los helpers existentes para que no haya
divergencias entre desktop y Quick Paste.

## Alcance

- Extender la matriz de acciones del menú `...` de Quick Paste.
- Mostrar `Editar captura` sólo para entradas aceptadas por
  `isEditableTextEntry`.
- Abrir el `EntryTextEditorModal.svelte` existente dentro de la misma ventana
  Quick Paste, con el título resuelto de la captura y foco inicial en el
  editor.
- Reutilizar `updateTextEntryCommand` y el servicio Rust ya implementado; no
  crear una migración ni una segunda operación de persistencia.
- Conservar ID, título, favoritos, tags, colecciones, source-app metadata,
  assets y timestamps no relacionados con la mutación textual.
- Refrescar Quick Paste después de un guardado exitoso mediante el evento
  metadata-only existente o el flujo de recarga ya establecido, preservando
  query, selección por ID y foco cuando sea posible.
- Mostrar los atajos de edición y previsualización dentro de los respectivos
  items del menú, manteniendo `role="menu"`, `role="menuitem"` y la
  navegación por teclado.
- Hacer que `Cmd/Ctrl+E` sea utilizable en la ventana Quick Paste cuando ésta
  está activa y existe una entrada textual elegible seleccionada. El shortcut
  debe reutilizar el matcher existente y respetar campos de edición, el menú
  y el modal como superficies interactivas.

## Fuera de alcance

- Editar imágenes, rich text, HTML/RTF o cualquier entrada no elegible.
- Cambiar la semántica de copiar, pegar, `Enter`, `Shift+Enter`, favoritos o
  `Cmd/Ctrl+Enter` de Quick Paste.
- Crear otra ventana, otro comando Rust, otra migración o una dependencia de
  editor.
- Cambiar el tamaño fijo `720 × 520`, la altura de las filas, el ordenamiento,
  el sistema de búsqueda, la captura del portapapeles o el drag and drop.
- Modificar el menú nativo del tray/menu bar de Tauri, que no tiene contexto
  de una captura individual. Este cambio se refiere al menú `...` de cada
  resultado de la ventana Quick Paste.
- Agregar red, telemetría, nube, logs de contenido o almacenamiento en
  `localStorage`.

## Criterio de aceptación

Desde Quick Paste, una entrada textual elegible permite abrir `Editar
captura`, modificar el contenido y guardarlo. El cambio sobrevive al reinicio,
mantiene el mismo ID y metadatos, y queda disponible para búsqueda, historial
principal y Quick Paste. Entradas de imagen o rich text no muestran la acción.
El menú muestra el texto platform-aware de ambos atajos, y sus atributos
accesibles coinciden con los matchers que realmente se ejecutan.
