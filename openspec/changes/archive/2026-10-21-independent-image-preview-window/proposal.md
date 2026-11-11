# Propuesta: ventana independiente para preview de imágenes

## Why

El preview de imágenes se dibuja dentro del webview que lo abrió, por lo que su
tamaño máximo depende de la ventana Desktop o QuickPaste. QuickPaste tiene una
geometría fija y no puede ofrecer un área mayor para revisar una imagen.

## What Changes

- Abrir los previews de capturas de imagen en una ventana nativa Tauri
  independiente y redimensionable, tanto desde Desktop como desde QuickPaste.
- Renderizar la imagen directamente en el contenido de esa ventana, sin un
  segundo modal u overlay interno. Los controles de giro quedan en la propia
  vista del preview; el cierre se realiza con el control nativo de la ventana
  o con `Escape`.
- Calcular el tamaño inicial usando las dimensiones de la imagen y el área de
  trabajo del monitor, conservando la proporción y ajustando automáticamente la
  imagen al espacio disponible cada vez que cambia el tamaño de la ventana,
  incluso cuando esto requiere ampliarla por encima de su resolución original.
- Cargar la captura en la ventana de preview a partir de su identificador local;
  la comunicación entre ventanas no transporta contenido ni bytes de assets.
- Cerrar el preview con el control nativo de cierre o con `Escape` y devolver el
  foco a la superficie que lo abrió. QuickPaste vuelve a mostrarse después del
  cierre.
- Mantener los previews de texto y rich text dentro del overlay actual.

## Fuera de alcance

- Cambiar la geometría fija de QuickPaste.
- Cambiar el preview de texto o rich text.
- Persistir las dimensiones elegidas entre aperturas o sesiones.
- Ofrecer un control interno para redimensionar la imagen con el puntero; el
  tamaño de la imagen se deriva del espacio disponible en la ventana nativa.
- Agregar dependencias o enviar contenido a servicios externos.

## Impacto

- Agrega una ruta frontend para la ventana de preview, una consulta por ID en el
  core y un comando Tauri delgado para que esa ventana cargue una captura.
- Ajusta el bridge de preview de Desktop y QuickPaste, los permisos de ventana
  Tauri y agrega la traducción del título. Para una captura eliminada, reutiliza
  el mensaje de preview no disponible que ya existe en los cinco idiomas.
- No cambia el esquema de SQLite ni la persistencia de assets.
