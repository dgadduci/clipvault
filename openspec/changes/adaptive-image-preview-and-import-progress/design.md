# Design: adaptive image preview and import progress

## Image preview sizing

- El preview sigue siendo un overlay dentro de la ventana existente y conserva
  el tratamiento actual de texto, rich text y foco. El usuario cierra el
  preview únicamente con el botón de cierre o con `Escape`; hacer clic o soltar
  el puntero sobre el fondo no lo cierra.
- Para imágenes, el tamaño objetivo parte del 80 % de las dimensiones
  intrínsecas conservando la proporción. Si el viewport no alcanza, se aplica
  una escala menor uniforme para mantener visible la imagen completa.
- El cuerpo del modal de imagen usa 10 px de padding en los cuatro lados. La
  tarjeta se ajusta al ancho del cuerpo y conserva un máximo que cabe dentro
  del overlay, incluyendo encabezado, pie y bordes.
- La tarjeta de imagen agrega un control de redimensionado en su esquina
  inferior derecha. Arrastrarlo cambia el ancho y el alto de la tarjeta; las
  flechas del teclado cambian ambas dimensiones con incrementos pequeños y
  `Shift` usa incrementos mayores. El control tiene una etiqueta accesible
  localizada.
- El tamaño elegido solo vive mientras se muestra la captura actual. Al abrir
  otra captura o cerrar el modal se vuelve al tamaño automático inicial. Al
  redimensionar la tarjeta, la imagen se vuelve a ajustar a su área, conserva
  su proporción y puede crecer hasta su dimensión intrínseca. La tarjeta se
  limita al overlay; en QuickPaste sigue dentro de la ventana nativa fija.
- El tamaño se calcula usando las dimensiones de la captura persistida y los
  límites disponibles del overlay. Se recalcula cuando cambia la imagen por
  una rotación, cambia el viewport o el usuario redimensiona la tarjeta.
  QuickPaste permanece en su geometría fija; solo su tarjeta de preview cambia
  de tamaño dentro de esos límites.
- La imagen mantiene `object-fit: contain`; no se recortan ni deforman sus
  píxeles.
- El overlay no registra una acción de cierre al hacer clic fuera de la tarjeta.
  Esto permite arrastrar el control de redimensionado más allá de los límites
  actuales del modal y soltar el botón del mouse sin cerrar el preview.

## Import progress

- El core procesa cada archivo en el orden confirmado. Un lector contador
  reporta bytes leídos durante la decodificación y el core también reporta cada
  archivo completado, incluidos los archivos que fallan o se omiten. El método
  existente sin callback mantiene su contrato delegando al mismo procesamiento
  con callbacks vacíos.
- El adaptador Tauri emite `clipvault://image-import-progress` a la ventana
  que inició la importación. El payload contiene únicamente `session_id`,
  `bytes_read`, `total_bytes`, `completed` y `total`. Las notificaciones durante
  la lectura se limitan para no emitir un evento por cada lectura pequeña; cada
  archivo completado y el último evento sí se informan.
- El frontend se suscribe antes de invocar el comando de importación, descarta
  eventos de otras sesiones y calcula un porcentaje entero limitado entre 0 y
  99 usando hasta el 90 % para la proporción de bytes leídos y hasta el 10 %
  para archivos procesados. Al completar el comando muestra 100 %. Si no hay un
  tamaño total válido, usa el avance por archivos. Un fallo al registrar el
  listener no impide completar la importación.
- En el pie del modal, el progreso aparece a la izquierda de Cancelar y del
  botón principal. El botón principal conserva su estado busy y el texto
  localizado «Importando…»; los controles siguen bloqueados durante el proceso.
- La barra tiene semántica `progressbar`, valores accesibles y texto numérico
  visible. Su etiqueta accesible se localiza en `en`, `es`, `pt`, `de` y `fr`.

## Verification

- Comprobar el cálculo de dimensiones para orientación vertical, horizontal,
  cuadrada y viewport limitado, además de la preservación de proporción.
- Comprobar que el redimensionado manual mantiene la tarjeta dentro del
  viewport, que el teclado puede cambiar sus dimensiones y que la imagen
  conserva su proporción al crecer, reducirse o rotarse.
- Comprobar que hacer clic en el fondo o soltar fuera el control de
  redimensionado deja abierto el preview, y que el botón de cierre y `Escape`
  siguen cerrándolo.
- Verificar que el core reporta bytes leídos durante la decodificación y un
  avance por cada archivo completado, y que el comando Tauri emite únicamente
  el identificador de sesión y contadores numéricos.
- Ejecutar el check y build frontend, pruebas frontend relevantes para preview,
  importación y regresiones de drag and drop, validación OpenSpec y
  `git diff --check`; ejecutar los tests relevantes del core.
- Revisar manualmente un preview vertical y uno horizontal, el redimensionado
  con mouse y teclado, que al soltar el arrastre fuera del modal el preview
  permanezca abierto, los cambios tras rotar, y una importación de varios
  archivos con tamaños distintos.
