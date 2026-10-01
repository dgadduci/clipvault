# Diseño: ciclo de vida del cliente de clipboard Wayland

## Causa confirmada

El lockfile fija `arboard 3.6.1`. En su implementación Linux,
`Clipboard::new()` intenta `wayland::Clipboard::new()` cuando existe
`WAYLAND_DISPLAY`; si la negociación de data-control falla, emite el warning
reportado y construye el cliente X11 como fallback.

`ArboardClipboard` invocaba `Arboard::new()` en cada operación de lectura o
escritura. El watcher de captura sondea el portapapeles periódicamente, por lo
que una sesión donde la negociación siempre falla repetía el warning en cada
nuevo cliente. El resultado manual confirmó que el fallback observado permite
el uso de la aplicación y que la captura se recupera tras una desconexión.

## Decisión aprobada

El adaptador Linux mantendrá un cliente `arboard` inicializado de forma
perezosa y compartido detrás de un lock propio. Las operaciones de clipboard
usarán el mismo cliente mientras siga conectado. Así, una negociación opcional
fallida seguida de fallback exitoso se reporta durante la creación del cliente
y no en cada lectura periódica.

En Linux, `ArboardClipboard` conserva el cliente tras una inicialización
correcta. Lo inicializa al primer uso bajo un `parking_lot::Mutex` para
serializar operaciones concurrentes. El adaptador conserva el wrapper también
si una operación devuelve `ArboardError::Unknown`, devuelve ese error al
llamador y permite que las operaciones posteriores sigan usando el backend.
La prueba manual confirmó que la captura se recupera tras una desconexión; el
adaptador no fuerza una recreación del cliente para obtener esa recuperación.
Los errores de formato ausente, conversión, clipboard ocupado o clipboard no
soportado tampoco invalidan la instancia.

Esta decisión evita describir como reinicio del backend la recreación del
wrapper de `arboard`: en la versión 3.6.1, el backend X11 reutiliza estado
compartido a nivel de proceso, así que crear otro wrapper no garantiza una
conexión nativa nueva. En una sesión Wayland que usa ese fallback, recrear el
wrapper también repetiría la negociación opcional que originó el warning.

La API documenta que la instancia de Linux mantiene el acceso necesario al
clipboard mientras vive. El lock evita operaciones simultáneas a través de
esta instancia. No se silencia un target de logging globalmente ni se añade
una dependencia para filtrar el warning. Los errores distintos de la
negociación opcional siguen visibles.

## Invariantes de plataforma

- Linux Wayland conserva la preferencia por `ext-data-control` o
  `wlr-data-control` cuando el compositor los publica.
- Si esos protocolos no están disponibles, `arboard` conserva el fallback
  XWayland existente. Los campos actuales de backend permanecen sin cambios;
  no se afirma que identifiquen cuál transporte eligió la negociación interna
  de `arboard`.
- Linux X11 conserva su cliente y señal XFixes.
- macOS sigue usando su adaptador actual.
- El adaptador conserva errores tipados, sondeo, privacidad y deduplicación.
- Los logs no incluyen contenido, hashes, rutas ni valores crudos del entorno.

## Verificación manual de entrada

El usuario reportó el 2026-09-28 que macOS y Ubuntu/X11 no muestran
regresiones. Ubuntu/Wayland tampoco mostró regresiones de captura, pero sí el
warning repetido de protocolo no soportado. En Arch/KDE/Wayland, la captura
funciona pero el nombre e icono fuente sólo aparecen para Warp; esa limitación
se sigue en el cambio OpenSpec de detección KDE.
