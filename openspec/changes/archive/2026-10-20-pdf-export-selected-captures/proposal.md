# Proposal: pdf-export-selected-captures

## Why

Las capturas ya pueden seleccionarse en Desktop y QuickPaste, pero no existe
una forma de reunirlas en un documento portátil. El usuario necesita crear un
PDF local a partir de esa selección conservando su orden y pudiendo elegir si
cada captura comienza en una página nueva.

## What Changes

- Agregar el botón PDF en Desktop entre el menú de acciones y el botón de
  borrado, y junto al indicador de selección de QuickPaste. Estará disponible
  con una o más capturas seleccionadas.
- Generar exactamente un PDF por acción. Incluir capturas de texto, rich text
  como texto plano con sus saltos de línea, e imágenes.
- Añadir a Configuración general la preferencia de iniciar cada captura en
  una página nueva. El valor predeterminado será activado.
- Abrir un único diálogo nativo para elegir el nombre y la ubicación del PDF;
  el archivo solo se escribirá en el destino elegido por el usuario.

## Capabilities

### New Capabilities

- `capture-pdf-export`: generación local de un PDF único a partir de la
  selección ordenada, inclusión de texto e imágenes, paginación configurable
  y guardado explícito en Desktop y QuickPaste.

## Impact

- El core Rust incorporará un servicio de exportación PDF basado en
  `printpdf` 0.12.8. Se usará su layout HTML/XHTML acotado a texto escapado,
  imágenes PNG y estilos simples, porque proporciona ajuste de texto y
  paginación automática para el flujo continuo. La documentación del crate
  describe este layout como todavía en evolución; el alcance no dependerá de
  estilos CSS arbitrarios ni ejecutará contenido activo. La alternativa de
  dibujar todas las líneas y paginar manualmente se descarta para evitar
  duplicar el motor de layout. Se incluirá una fuente DejaVu Sans con su
  licencia para mantener una salida legible sin depender de fuentes instaladas
  en el sistema.
- El shell Tauri usará `tauri-plugin-dialog` 2.8.3 para el diálogo nativo de
  guardado en macOS y Linux. El core generará los bytes; el adaptador Tauri
  mostrará el diálogo y escribirá el único archivo en la ruta elegida.
- Se persistirá la preferencia de paginación en `app_settings`; no requiere
  migración de SQLite.
- La UI de Desktop, QuickPaste y Configuración general incorporará las nuevas
  acciones y etiquetas traducidas en `en`, `es`, `pt`, `de` y `fr`.
- El generador leerá las capturas y assets ya persistidos sin modificar las
  capturas originales ni mantener copias del PDF en el almacenamiento de
  ClipVault.
