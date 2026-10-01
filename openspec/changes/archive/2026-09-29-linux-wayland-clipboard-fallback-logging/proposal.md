# Propuesta: inicialización estable del clipboard fallback en Wayland

## Why

En Ubuntu Wayland, la captura continúa sin regresiones observables, pero la
consola repite a intervalos cortos el warning de `arboard` que anuncia que el
compositor no ofrece `ext-data-control` ni `wlr-data-control` y que se usará
el protocolo de clipboard X11. La prueba manual del 2026-09-28 confirmó el
mismo comportamiento funcional para macOS y Ubuntu/X11, y reportó la repetición
del warning en Ubuntu/Wayland.

La revisión del código localizó la repetición: `arboard::Clipboard::new()`
intenta negociar Wayland cada vez que se crea un cliente, y
`ArboardClipboard` crea uno por operación. El watcher vuelve a leer el
clipboard periódicamente. En el caso común de un compositor sin data-control
pero con fallback X11 utilizable, la negociación fallida es una limitación
conocida del compositor y no un fallo de captura, por lo que el mismo warning
se vuelve ruido operativo.

El usuario autorizó corregir la inicialización repetida el 2026-09-28.

## What Changes

- Inicializar y reutilizar el cliente de clipboard de `arboard` durante la
  sesión de la aplicación cuando su conexión siga siendo válida.
- Mantener la selección de Wayland data-control cuando el compositor lo
  soporte y conservar el fallback existente cuando no lo soporte.
- Evitar un warning por cada sondeo para una misma limitación estable del
  compositor, sin ocultar otros errores de plataforma.
- Conservar los diagnósticos metadata-only existentes y permitir la
  recuperación después de un fallo real de conexión.
- No cambiar captura, deduplicación ni persistencia en macOS, Linux X11 o KDE
  Wayland compatible.

## Alcance

El cambio se limita al ciclo de vida del cliente `arboard` en el adaptador de
clipboard Linux y a los diagnósticos locales relacionados. No agrega protocolos,
dependencias, procesos externos ni llamadas de red.

## Fuera de alcance

- Resolver la identificación de la aplicación fuente en KDE, que permanece en
  `kde-wayland-source-app-detection`.
- Cambiar qué formatos de clipboard admite cada backend.
- Quitar el fallback X11 o anunciar que éste captura cambios nativos Wayland
  cuando el compositor no publica data-control.
- Filtrar globalmente los warnings de dependencias o desactivar diagnósticos
  de errores inesperados.
