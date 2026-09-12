# linux-app-icon-package-variants

Cambio propuesto para resolver iconos Linux cuando el empaquetado desacopla
el identificador de la aplicación, el nombre del archivo `.desktop` y la ruta
local declarada en `Icon=`.

Estado: cerrado explícitamente y listo para archivar. La aceptación funcional
de esta entrega queda limitada a Firefox Snap. El icono de xterm continúa sin
resolverse en la sesión probada; queda como limitación conocida aceptada.

Resultado manual registrado: Firefox conserva su identidad, nombre e icono;
xterm conserva el fallback sin icono.
