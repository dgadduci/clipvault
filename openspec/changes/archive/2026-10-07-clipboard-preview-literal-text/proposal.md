# Propuesta: conservar los caracteres originales en el preview de texto

## Why

El preview compartido de capturas textuales pre-escapa el contenido como
entidades HTML y luego lo entrega a una interpolación de texto de Svelte. Esa
interpolación vuelve a escapar el ampersand, por lo que una comilla capturada
puede aparecer literalmente como `&quot;`. La captura almacenada permanece
intacta, pero el preview no muestra los mismos caracteres.

## What Changes

- Mostrar el texto canónico de la captura con una interpolación de texto segura
  del framework, sin codificarlo antes como entidades HTML.
- Conservar las comillas, apóstrofes, ampersands, texto parecido a entidades,
  signos de menor/mayor y espacios tal como fueron capturados.
- Mantener el contenido inerte: texto parecido a HTML no debe crear elementos,
  ejecutar scripts, activar handlers ni iniciar navegación.
- Aplicar el comportamiento en el componente compartido que usan el Desktop y
  Quick Paste; mantener la representación de resaltado de código con su ruta
  segura actual.
- No modificar el contenido persistido, su búsqueda ni las acciones de copiar
  y pegar.

## Capacidad afectada

- Modifica `clipboard-history-cards`: contrato de caracteres visibles en el
  preview compartido de Desktop y Quick Paste.

## Impacto

- Frontend: `ClipboardPreview.svelte`, documentación de helpers y pruebas de
  preview de texto y código.
- Backend, persistencia, catálogos, dependencias y contratos Tauri: sin cambios
  previstos.
