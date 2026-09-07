# Proposal: quick-paste-preview-ui

## Contexto

Quick Paste ya tiene una ventana fija de `720 × 520`, una lista vertical
compacta, selección con teclado, favoritos, acciones de pegado y un flujo
copy-only para `Enter`/`Shift+Enter`. En la prueba visual actual hay cuatro
problemas que reducen su utilidad:

- el indicador del tipo de captura aparece como un cuadrado vacío;
- el icono de la aplicación fuente aparece como un cuadrado vacío;
- el botón `...` no muestra las acciones definidas para cada tipo;
- no existe una previsualización completa de la captura.

Además, la ventana necesita integrarse mejor con la estética del desktop: sus
bordes deben ser redondeados, la tipografía debe usar la misma escala visual
que las cards y la búsqueda debe tener un atajo visible.

## Objetivos

- Renderizar el icono real del tipo de captura usando el registro de iconos
  existente y fallbacks accesibles cuando el tipo no tenga un icono específico.
- Renderizar el icono real de la aplicación fuente usando los resolvers y
  referencias de assets existentes, sin mostrar identificadores crudos.
- Restaurar el menú `...` con acciones válidas por tipo:
  `Pegar`, `Pegar texto enriquecido`, `Pegar texto plano` y
  `Previsualizar` según corresponda.
- Mantener el pin como control separado del menú.
- Agregar `Cmd+K` en macOS y `Ctrl+K` en Linux para enfocar la búsqueda y
  mostrar visualmente el atajo.
- Agregar `Cmd+Enter` en macOS y `Ctrl+Enter` en Linux para previsualizar la
  captura seleccionada.
- Mostrar la previsualización dentro de la misma ventana fija, sin crear otra
  ventana ni alterar el clipboard o la aplicación activa.
- Redondear el contenedor de la ventana y alinear fuente, pesos, colores y
  tamaños con las cards del desktop principal.
- Mantener la lista, el foco, el scroll y la geometría fija sin overflow
  horizontal.

## Fuera de alcance

- No cambiar el tamaño fijo ni crear una nueva ventana Quick Paste.
- No cambiar el comportamiento aprobado de `Enter`/`Shift+Enter`: continúa
  siendo copy-only y no envía `Cmd/Ctrl+V` sintético.
- No cambiar el flujo de pegado directo de las acciones explícitas del menú.
- No agregar campos de persistencia ni modificar `asset_ref` de imágenes.
- No reimplementar iconos de aplicación, búsqueda, favoritos, tags,
  colecciones, drag and drop o captura fuera de los adapters existentes.
- No agregar red, telemetría, embeddings, LLMs ni dependencias innecesarias.
