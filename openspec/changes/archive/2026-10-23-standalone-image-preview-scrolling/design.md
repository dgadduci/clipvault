# Design: standalone image preview scrolling

## Decisión

El área de imagen de la ventana nativa será un contenedor desplazable en ambos
ejes. Se usará `overflow: auto`, de modo que cada barra aparezca cuando la
imagen exceda el espacio disponible. El desplazamiento queda dentro del área
de imagen y no altera el tamaño de la ventana ni la cabecera con los controles.

El zoom dejará de usar `transform: scale`, que cambia sólo la pintura y no el
tamaño que el contenedor puede desplazar. El ancho y el alto de layout de la
imagen se calcularán multiplicando por el mismo factor el tamaño ya ajustado al
viewport. Así se conserva la proporción y el contenedor detecta el overflow.

El cuerpo de la preview se alineará al inicio cuando el contenido exceda el
viewport. Márgenes automáticos en la imagen la mantienen centrada mientras
quepa y pasan a cero cuando desborda, dejando accesibles sus bordes superior e
izquierdo. Las reglas nuevas sólo se aplican a la preview nativa de imagen; la
preview embebida conserva su recorte actual.

El wheel sin modificadores queda disponible para desplazar el contenedor. Los
gestos de zoom y los botones existentes continúan actualizando el mismo factor
de escala.

## Alternativas descartadas

- Mantener `transform` y añadir `overflow: auto` no sirve porque la
  transformación no modifica la caja de layout y no crea área desplazable.
- Aplicar scroll a la ventana o al documento incluiría la cabecera en el
  desplazamiento y podría ocultar los controles.
- Agregar una biblioteca de zoom/pan no es necesario; el layout y el scroll
  nativos cubren el comportamiento solicitado.

## Verificación

- Añadir una regresión frontend que compruebe scroll en ambos ejes y que el
  tamaño renderizado de la imagen responda al zoom sin perder proporción.
- Ejecutar el check/build del frontend, las regresiones relevantes de preview,
  la validación estricta de OpenSpec y `git diff --check`.
