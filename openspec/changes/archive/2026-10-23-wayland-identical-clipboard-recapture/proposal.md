# Propuesta: recaptura de contenido idéntico en Wayland

## Problema

Después de eliminar una captura, volver a copiar el mismo texto o la misma
imagen puede no crear una captura nueva. El watcher necesita una señal de
cambio del portapapeles distinta del contenido: el contenido idéntico produce
el mismo fingerprint tanto cuando permanece copiado como cuando el usuario lo
copia otra vez.

En Linux Wayland, la revisión actual se elige solo por el tipo de sesión. Si el
compositor no ofrece el protocolo nativo `ext-data-control-v1` o
`wlr-data-control-unstable-v1`, `arboard` usa X11/XWayland, pero ClipVault deja
la revisión en `UNKNOWN`. El watcher conserva su fingerprint y suprime la copia
posterior.

## Cambio propuesto

- Seleccionar el origen de revisión según el transporte de portapapeles que
  `arboard` puede usar en esa sesión:
  - data-control nativo disponible: observar eventos de cambio de selección
    Wayland;
  - data-control no publicado: observar XFixes en X11/XWayland, que es la ruta
    de fallback de `arboard`;
  - errores de conexión o falta de una fuente fiable: conservar `UNKNOWN`.
- Aplicar la misma semántica de recaptura a texto, rich text e imágenes, ya que
  el watcher comparte la señal de revisión para todos los payloads.
- Mantener la garantía de que borrar una captura no vuelve a crearla mientras
  la selección del portapapeles siga en la misma revisión.

## Impacto

- Afecta al adaptador de portapapeles Linux y a sus pruebas de integración con
  el watcher y la persistencia.
- No cambia la base de datos ni los formatos de captura.
- No expone payloads, MIME types, fingerprints ni rutas en la señal de revisión.
- Añade dependencias Wayland como dependencias directas opcionales de la
  plataforma; ya están presentes transitivamente por `wl-clipboard-rs`.

## Fuera de alcance

- Inventar revisiones comparando el contenido.
- Cambiar el orden de prioridad de texto, rich text e imágenes.
- Modificar la sincronización o persistencia de historial.
