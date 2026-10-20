# Design: multi-select-captures

## Modelo de selección

- La selección vive en el componente que posee cada lista y nunca se persiste.
- Click simple conserva la selección simple actual. `Ctrl`/`Cmd` + click
  alterna una captura; Desktop permite seleccionar rangos con `Shift` + click
  o `Shift` + flechas. QuickPaste permite seleccionar rangos con `Shift` +
  flechas para preservar su acción existente de `Shift` + click que copia
  texto plano.
- Desktop mantiene una captura activa para los atajos existentes y un conjunto
  de capturas seleccionadas para la operación grupal. Si una captura sale del
  resultado visible, se elimina del conjunto.
- QuickPaste mantiene el índice activo independiente del conjunto seleccionado.
  Un click simple conserva el flujo actual de copia para una captura; los
  clicks modificados, los controles visibles de selección por fila y la
  navegación con Shift solo cambian la selección. El control visible permite
  agregar o quitar una captura sin requerir un modificador de teclado.
  Enter con más de una captura seleccionada ejecuta un paste stack por medio
  de `clipvault_paste_entry`, en el orden en que aparecen los IDs en la lista.
- El paste stack oculta QuickPaste una vez antes de la primera operación y
  espera la respuesta de cada captura antes de iniciar la siguiente. Ante el
  primer error detiene el resto, vuelve a mostrar QuickPaste y comunica cuántas
  capturas alcanzaron a pegarse; las pegadas antes del error no se pueden
  revertir.
- Una búsqueda, recarga o cambio de ámbito elimina de la selección los IDs que
  ya no están visibles. Al abrir de nuevo QuickPaste se reinicia la selección.

## Drag-and-drop grupal

- El controlador singleton detecta la selección marcada en la rail. Si el
  origen pertenece a ella, la sesión en memoria guarda la selección visible;
  si no, la sesión contiene únicamente la captura de origen.
- `DataTransfer` mantiene el formato existente y lleva únicamente el ID opaco
  de la captura que inició el gesto. La sesión local y los eventos internos
  pueden transportar IDs opacos adicionales, sin texto ni metadata del
  portapapeles.
- La zona delegada conserva la misma resolución de hit testing, fallback de
  WebKit, feedback, cancelación y exclusión de colecciones peer-bound. Al
  soltar, el Desktop añade la colección destino para cada captura con la lógica
  existente de `combineMemberships` y `entry_collections_set`.
- Los IDs repetidos, inexistentes o fuera de la lista visible se descartan. La
  escritura es aditiva e idempotente; un error en una captura no impide intentar
  las restantes y el Desktop actualiza la organización una vez al finalizar.

## Interfaz y accesibilidad

- `aria-selected` refleja cada miembro de la selección múltiple y
  `aria-multiselectable` declara la capacidad de la lista QuickPaste.
- Cada fila QuickPaste incluye un botón de selección accesible que expone su
  estado con `aria-pressed`; activarlo no copia ni oculta la ventana.
- Los controles existentes y las superficies de edición, menús, títulos,
  favoritos, notas y chips siguen excluidos de la selección y del drag.
- Las cadenas accesibles nuevas y los errores parciales se traducen en todos
  los catálogos soportados.

## Verificación prevista

- Revisar el flujo de selección, la asignación por lote y el paste stack junto
  con los contratos existentes de drag and drop y QuickPaste.
- Ejecutar los checks/builds de frontend que no ejecuten pruebas, la validación
  estricta de OpenSpec y `git diff --check`.
- No realizar una prueba manual de arrastre en este entorno si no hay una
  sesión de escritorio accesible; dejarlo indicado como pendiente.
