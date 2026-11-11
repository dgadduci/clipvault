# Diseño: ventana independiente para preview de imágenes

## Ventana nativa

- Los previews de imágenes usan una `WebviewWindow` superior con una etiqueta
  estable (`image-preview`) y una página frontend dedicada. La ventana es
  redimensionable con los controles nativos del sistema y existe una sola a la
  vez; abrir otra captura reemplaza el preview anterior.
- La ventana se centra en la pantalla disponible y calcula su tamaño lógico
  inicial a partir del tamaño y proporción de la imagen, con límites basados en
  el área de trabajo visible. Si faltan las dimensiones de imagen o pantalla,
  usa un tamaño inicial seguro. El tamaño se limita al monitor; redimensionar la
  ventana vuelve a ajustar la imagen sin recortarla ni deformarla.
- La página de preview muestra la imagen directamente como contenido de la
  ventana nativa. No monta un `role=dialog`, backdrop ni tarjeta modal dentro de
  esa ventana. Su barra de herramientas incluye las acciones de giro, sin un
  botón de cierre adicional.
- La imagen ocupa el espacio disponible de la ventana manteniendo la proporción,
  aunque para ello deba mostrarse más grande que sus dimensiones intrínsecas.
  El ajuste se recalcula al redimensionar la ventana y al terminar una rotación;
  no existe un control de arrastre para cambiar el zoom manualmente.
- La ventana Desktop no queda siempre encima de otras aplicaciones. Si el
  origen es QuickPaste, QuickPaste se oculta al perder el foco como ya hace hoy;
  al cerrar el preview se vuelve a mostrar y enfocar conservando su consulta y
  selección.
- `Escape` o el botón nativo del marco cierra la ventana nativa y enfoca el
  origen; la vista no muestra un segundo botón de cierre. El evento de cierre
  de Tauri debe permitir explícitamente que `image-preview` se destruya y no
  aplicar el comportamiento de ocultar usado por las ventanas principales. La
  interacción fuera de la ventana no la cierra.

## Carga local de la captura

- El bridge envía únicamente `entry_id` y una etiqueta de origen permitida
  (`main` o `quick-paste`) en la URL local de la nueva ventana. No incluye texto,
  hashes, rutas, referencias de assets ni bytes.
- La página independiente lee el ID y solicita `clipvault_entry_by_id`. El
  comando adapta `TextHistoryService::entry_by_id`, que delega la consulta en
  `EntryRepository::find_by_id`; una captura eliminada responde `null`.
- Si la captura ya no existe, la ventana muestra un mensaje localizado. El
  preview reutiliza `ClipboardPreview` para cargar la
  imagen y conserva el resolver validado de assets; un modo de vista directa
  presenta esa captura sin el overlay compartido.
- La misma acción de preview abre la ventana independiente solo para imágenes.
  Los previews de texto y rich text permanecen en el overlay compartido actual.

## Tamaño inicial y resize

- Un helper puro calcula dimensiones lógicas de ventana desde las dimensiones
  intrínsecas de la captura y `screen.availWidth` / `screen.availHeight` del
  origen. La escala inicial de imagen sigue limitada al 80 % y los límites de
  ventana dejan espacio para los controles y metadatos del preview.
- La ventana establece mínimos razonables y `resizable: true`. Si el monitor es
  pequeño, el helper reduce uniformemente la imagen para caber en el área
  disponible. Un cambio posterior del tamaño nativo vuelve a ajustar el preview
  al viewport independiente, usando todo el espacio disponible y sin deformar
  ni recortar la imagen.
- El redimensionamiento se realiza únicamente con el marco nativo de la
  ventana; no hay un control interno de arrastre para cambiar el tamaño de la
  imagen.

## Seguridad y permisos

- Se agrega la etiqueta `image-preview` a la capability de la aplicación y se
  habilita la creación dinámica de `WebviewWindow` solo en las superficies que
  abren el preview. La ventana conserva únicamente los permisos necesarios para
  cerrar y devolver el foco a la superficie de origen.
- La etiqueta de origen se valida contra los dos nombres admitidos antes de
  restaurar el foco. Los IDs se validan como enteros positivos.
- No se añaden llamadas de red, dependencias, ni acceso frontend a SQLite.

## Verificación

- Probar el cálculo inicial con imágenes horizontales, verticales, cuadradas,
  metadatos inválidos y monitor pequeño; confirmar conservación de proporción.
- Verificar que Desktop y QuickPaste abren una ventana separada y que cambiar
  su tamaño ajusta la imagen sin cambiar las dimensiones de sus ventanas de
  origen.
- Verificar reemplazo de una captura ya abierta, cierre con `Escape`,
  cierre con el control nativo, retorno de foco, restauración de QuickPaste y
  mensaje si la captura fue eliminada antes de cargar.
- Confirmar que no hay un modal ni un botón de cierre dentro de la ventana
  nativa, que los controles de rotación están en esa vista y que el ajuste
  automático puede ampliar la imagen al agrandar la ventana, conservando la
  proporción tras varios tamaños.
- Ejecutar tests de core y frontend, checks/build Tauri/frontend, regresiones
  frontend relevantes, validación estricta de OpenSpec y `git diff --check`.
