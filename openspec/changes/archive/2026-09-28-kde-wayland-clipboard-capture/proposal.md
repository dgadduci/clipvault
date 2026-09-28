# Propuesta: captura del portapapeles en KDE Plasma Wayland

## Problema

En Arch Linux con KDE Plasma y sesión Wayland, ClipVault no agrega al historial
el contenido que se copia al portapapeles. El usuario informa que la captura
funciona en macOS y en Ubuntu GNOME Wayland/X11. El fallo observado está en la
ruta de captura del portapapeles; la identificación de la aplicación activa y
el pegado no forman parte del síntoma reportado.

La revisión estática encontró dos posibles causas relacionadas:

- `arboard 3.6.1` implementa Wayland mediante la feature opcional
  `wayland-data-control`; la configuración de la dependencia de ClipVault no
  parece habilitarla y el lockfile actual no incluye `wl-clipboard-rs`.
- En Linux, `ArboardClipboard` intenta crear un monitor XFixes cuando puede
  conectarse a X11. El watcher considera autoritativa una revisión conocida y
  puede devolver `Unchanged` antes de comparar el payload. Un contador X11 no
  representa necesariamente los cambios del portapapeles Wayland nativo de
  KDE.

Estos indicios no prueban por sí solos la causa en el equipo del usuario. La
implementación y la configuración real de compilación deben reproducirse en
KDE Plasma Wayland antes de cerrar el diagnóstico.

## Objetivos

- Identificar por qué la build Linux actual no captura copias en KDE Plasma
  Wayland, distinguiendo la inicialización/lectura del backend de la detección
  de cambios y de la persistencia.
- Corregir la selección del backend Linux para que la sesión Wayland pueda
  leer el portapapeles nativo cuando el compositor ofrezca el protocolo
  soportado.
- Alinear la señal de cambio/revisión con el mismo dominio del portapapeles
  que lee el adapter; una revisión X11 no debe suprimir una observación nativa
  Wayland.
- Conservar los pipelines existentes de texto, texto enriquecido e imágenes,
  la deduplicación, la privacidad y el comportamiento X11.
- Añadir diagnósticos locales metadata-only que permitan diferenciar backend
  seleccionado, inicialización, lectura y señal de cambio sin incluir el
  contenido copiado.

## Alcance

Incluye adapters de plataforma Linux, la configuración de features/dependencias
necesaria para el build Linux y las pruebas de captura asociadas. La
verificación manual principal se realiza en Arch Linux con KDE Plasma Wayland;
GNOME Wayland/X11, Linux X11 y macOS son controles de no-regresión.

La causa raíz que se confirme durante la implementación debe quedar registrada
en este cambio antes de cerrar las tareas. Si el backend existente no sirve
para el protocolo real de la versión KDE objetivo, se actualizan diseño y
justificación OpenSpec antes de introducir otra dependencia arquitectónica.

## Fuera de alcance

- Cambiar detección de aplicación activa, la extensión GNOME, hotkeys o
  Quick Paste.
- Añadir formatos de portapapeles que el pipeline actual no soporte.
- Requerir `wl-paste`, `xclip`, `xsel`, otros procesos externos o paquetes del
  sistema para capturar.
- Usar una API privada de KDE/KWin que impida mantener el adapter Wayland
  reutilizable en otros compositores compatibles.
- Cambiar la UI, la base de datos, el modelo de datos o los assets existentes.
- Agregar red, telemetría, logging de contenido o servicios externos.

## Capacidad afectada

- `desktop-platform-integration`: lectura del portapapeles y detección de
  cambios coherentes con la sesión Linux activa, incluida Wayland.
