# Proposal: standalone-image-preview-scrolling

## Why

La ventana nativa de preview permite ampliar una captura de imagen mediante
los botones de zoom y los gestos del touchpad. Sin embargo, el área de imagen
recorta el contenido y el zoom no cambia sus dimensiones de layout, por lo que
las partes que quedan fuera del viewport no se pueden recorrer.

## What Changes

- Permitir desplazamiento horizontal y vertical cuando una imagen ampliada
  supera el área visible de la ventana nativa.
- Mantener la relación de aspecto de la imagen al cambiar el zoom.
- Mantener los controles de rotación y zoom visibles en la cabecera de la
  ventana.
- Aplicar el comportamiento al preview abierto tanto desde Desktop como desde
  QuickPaste, que comparten la misma ventana nativa.

## Fuera de alcance

- No cambiar el tamaño inicial ni el comportamiento redimensionable de la
  ventana.
- No cambiar los previews pequeños de las cards ni la ventana/modal de texto.
- No agregar dependencias ni modificar el almacenamiento de imágenes.
