# Proposal: image-preview-image-only-zoom-controls

## Why

El gesto de zoom del touchpad amplía actualmente el webview completo del
preview de imagen. Al hacerlo, también aumenta la barra superior y se pierden
de vista los controles de rotación.

## What Changes

- Interceptar los gestos de zoom del touchpad en la ventana nativa de preview y
  aplicarlos únicamente a la imagen.
- Agregar botones accesibles de zoom in y zoom out, con iconos, en la misma
  línea que los controles de rotación.
- Mantener la barra y los controles en su tamaño y posición mientras cambia la
  escala de la imagen.
- Reiniciar el nivel de zoom al abrir otra captura y mantenerlo solo en memoria
  durante el preview actual.
- Traducir las etiquetas accesibles de los botones en los cinco idiomas
  disponibles.
- Corregir el estado reactivo de los botones y bloquear el zoom nativo del
  webview para que el gesto solo escale la imagen.
- Mantener Desktop y QuickPaste en el zoom base para evitar cambios de escala
  en filtros, colecciones y rail de capturas.
- Conservar el rail de capturas horizontal y la escala tipográfica existente
  en el escritorio; mantener blancos los números de los badges de colección.

## No Goals

- No cambiar el tamaño de la ventana nativa ni agregar controles de paneo.
- No persistir el nivel de zoom entre capturas, ventanas o sesiones.
- No modificar la rotación persistente ni el ajuste proporcional inicial de la
  imagen.
- No modificar el contenido ni el formato del preview de texto o rich text, ni
  el desplazamiento ordinario de esas superficies.

## Impact

- Actualiza el componente compartido `ClipboardPreview.svelte`, agrega un
  helper puro para limitar la escala y suma etiquetas a los catálogos `en`,
  `es`, `pt`, `de` y `fr`.
- No requiere dependencias nuevas, cambios de backend ni cambios de
  persistencia.
