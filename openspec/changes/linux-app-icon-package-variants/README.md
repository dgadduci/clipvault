# linux-app-icon-package-variants

Cambio propuesto para resolver iconos Linux cuando el empaquetado desacopla
el identificador de la aplicación, el nombre del archivo `.desktop` y la ruta
local declarada en `Icon=`.

Estado: implementación parcial validada manualmente para Firefox Snap. El
icono de xterm continúa sin resolverse en la sesión probada; queda como
limitación conocida y este cambio no se archiva todavía.

Resultado manual registrado: Firefox conserva su identidad, nombre e icono;
xterm conserva el fallback sin icono.
