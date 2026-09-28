# Diseño: captura del portapapeles en KDE Plasma Wayland

## Diagnóstico que debe confirmarse

El build actual usa `arboard 3.6.1`. Su fuente declara `wayland-data-control`
como feature opcional y selecciona el adapter Wayland cuando `WAYLAND_DISPLAY`
está disponible y esa feature está compilada; sin ella, la ruta Linux usa X11.
La configuración del workspace activa `clipboard-arboard`, pero no declara
esa feature de Wayland. El lockfile tampoco contiene `wl-clipboard-rs`.

Por separado, `X11ClipboardRevisionMonitor` escucha `CLIPBOARD` mediante
XFixes. `CaptureWatcher` confía en una revisión conocida y considera la
observación sin cambio antes de comparar su payload. En Wayland, si el lector
es nativo y el contador observado pertenece a X11, ambas señales pueden
desacoplarse.

La implementación debe confirmar cuál de estos casos explica la reproducción
real, incluyendo la línea de comandos/features del binario que se prueba. El
diagnóstico debe distinguir, al menos, backend ausente/no compilado,
inicialización Wayland fallida, lectura vacía/error, revisión obsoleta y
persistencia rechazada. No se debe tratar `source_app`, blacklist o
identificación de foco como explicación sin evidencia del capture pipeline.

## Ruta de plataforma

La selección debe describir el transporte de clipboard efectivo para la
sesión, no inferir que XWayland refleja siempre el portapapeles nativo:

1. Linux X11 conserva el adapter y el monitor XFixes existentes.
2. Linux Wayland usa el protocolo nativo de clipboard de Wayland cuando el
   build y el compositor lo soportan. Se valida el protocolo contra KDE Plasma
   en el runtime objetivo.
3. Sólo si la ruta nativa no está disponible puede usarse una ruta XWayland
   como fallback cuando su conexión sea realmente utilizable. El estado y la
   causa quedan tipados; una sesión Wayland no se marca como captura nativa por
   haber conectado a X11.
4. Si ninguna ruta sirve, el backend devuelve un error/capacidad tipados y el
   watcher sigue ejecutándose.

Como primera opción se evaluará activar el soporte opcional
`arboard/wayland-data-control` sólo en builds Linux. Ya existe en la
dependencia usada; incorpora `wl-clipboard-rs` al grafo Linux, pero no añade
una segunda biblioteca directa ni exige un ejecutable del sistema. El build
real debe confirmar que el protocolo soportado funciona en la versión de KDE
reproducida y conserva lecturas de texto, HTML e imágenes que hoy expone
`arboard`.

Si el protocolo de esa feature no es compatible con la versión de KDE objetivo,
MiniMax debe pausar y proponer una actualización de este diseño con la API,
dependencia o fallback alternativo, su impacto y las opciones descartadas
antes de modificar la arquitectura.

## Coherencia entre lectura y revisión

La señal usada para descartar lecturas repetidas debe pertenecer al mismo
portapapeles que entrega el payload:

- XFixes puede representar cambios de una selección X11/XWayland observable.
- Una lectura Wayland nativa no debe combinarse con una revisión X11 que no
  observa esos cambios.
- Si la ruta Wayland no dispone de una revisión confiable, el adapter informa
  revisión desconocida y permite que el `CaptureWatcher` compare fingerprints
  locales de payload como fallback. No inventa una revisión monotónica ni
  usa un contador de otro servidor para declarar `Unchanged`.
- Si se añade una fuente nativa de cambios, debe ser metadata-only, usar el
  mismo transporte y mantener el comportamiento actual de duplicados y de
  recaptura tras una eliminación cuando la fuente real permite distinguir una
  nueva copia idéntica.

La implementación reutiliza el único `CaptureWatcher` existente y no añade
otro hilo de captura, deduplicador ni persistencia paralela.

## Diagnósticos y privacidad

Los diagnósticos pueden indicar backend efectivo (`x11`, `wayland_data_control`,
`xwayland_fallback` o `unavailable`), estado de inicialización/lectura y tipo
estable de error. Sólo registran presencia de variables de sesión si hace
falta; no registran su valor, rutas de socket, texto, hashes, snippets, bytes,
formatos con nombre de archivo ni títulos/PID de ventanas.

Los errores del lector no pueden terminar el loop ni corromper historial.
Contenido inválido/no soportado mantiene los resultados tipados actuales.
`PrivacyGate` sigue ejecutándose antes de persistir payloads y assets.

## Dependencias y alternativas

La opción preferida es habilitar el soporte Wayland que ya publica `arboard`,
condicionado a Linux y verificado contra KDE. Esto evita una nueva abstracción
de clipboard y mantiene juntas la negociación de texto e imagen.

Se descartan como requisitos de runtime:

- `wl-paste`/`wl-copy` u otros comandos, porque exigirían instalación del
  sistema y otro proceso para cada lectura;
- APIs privadas de KWin, porque acoplarían una capacidad de plataforma a un
  compositor y no resolverían los demás Wayland;
- duplicar `wl-clipboard-rs` directamente si `arboard` puede mantener la
  lectura y escritura coherentes con los formatos existentes.

Si las pruebas demuestran que la feature existente no es suficiente, cualquier
dependencia adicional requiere motivo, impacto y alternativa descartada
documentados aquí antes de implementarla.

## No regresiones

- macOS mantiene el adapter nativo y sus rutas de fidelidad de imagen/rich
  text.
- Linux X11 conserva XFixes y su comportamiento de captura/deduplicación.
- GNOME Wayland mantiene la integración de metadata de aplicación en una capa
  independiente; el arreglo no depende de instalar la extensión GNOME.
- Aplicaciones XWayland siguen pudiendo usar el fallback disponible.
- Se mantienen privacidad, blacklist, formatos existentes, base de datos,
  assets persistidos y demás comportamiento del historial.
