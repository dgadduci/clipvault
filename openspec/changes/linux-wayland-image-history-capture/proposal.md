# Propuesta: capturar imágenes sucesivas en Ubuntu Wayland

## Por qué

Se observó que, en Linux Ubuntu con Wayland, la primera captura de imagen se
agrega al historial, pero las capturas de imagen posteriores no aparecen. La
causa está en la revisión sintética del clipboard: el fallback de Linux la
deriva solo del tramo de texto. Como las imágenes no tienen ese tramo, dos
capturas de imagen sucesivas reciben la misma revisión conocida y el watcher
descarta la posterior antes de comparar su contenido. El comportamiento
impide usar de forma continua la captura de imágenes mientras ClipVault
permanece abierto.

## Qué cambia

- Definir que ClipVault siga observando cambios del portapapeles después de
  guardar una imagen y capture imágenes compatibles posteriores en Ubuntu
  Wayland.
- Informar la revisión como desconocida cuando Linux no tiene un evento
  confiable de cambio y usar la comparación de fingerprint del watcher para
  detectar payloads distintos, incluidas imágenes.
- Mantener la deduplicación existente: una imagen con contenido persistido
  distinto crea una entrada nueva; una imagen idéntica sigue la política
  actual de duplicados.
- Asegurar que un error transitorio al leer una imagen no impida capturar una
  imagen compatible que llegue después.
- Mantener el contenido de las imágenes local y fuera de logs, errores y
  diagnósticos.

## Capacidades

### Nuevas capacidades

- `linux-wayland-image-history-capture`: captura continua de imágenes del
  portapapeles en sesiones Ubuntu Wayland.

### Capacidades modificadas

Ninguna. La capacidad nueva especifica la regresión de captura sin modificar
el contrato general de deduplicación y persistencia de imágenes.

## Impacto

La investigación de implementación deberá localizar en el adaptador Linux y
la coordinación de captura dónde deja de procesarse una actualización
posterior del portapapeles. La solución deberá conservar los límites entre el
adaptador de plataforma y el core. No cambia el esquema, el formato de assets
ni la UI. Las revisiones nativas de macOS y X11 con XFixes permanecen intactas;
los hosts sin una revisión confiable usan el fingerprint existente del watcher.

## Fuera de alcance

- Cambiar la deduplicación de imágenes idénticas.
- Cambiar la prioridad actual de texto enriquecido, texto plano e imagen.
- Modificar la detección de aplicación activa en Wayland.
- Cambiar la escritura de imágenes al portapapeles o Quick Paste.
- Garantizar este comportamiento para todos los compositores Wayland sin
  verificación específica.
