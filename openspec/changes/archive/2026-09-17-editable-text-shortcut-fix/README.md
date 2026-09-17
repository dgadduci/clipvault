# editable-text-shortcut-fix

Corrección del atajo de edición de capturas textuales. La implementación
actual sólo evalúa el evento en el elemento `article` de la card y sólo
reconoce `ctrlKey`, por lo que no cubre el flujo de una card seleccionada cuyo
foco está en otra superficie ni el modificador Command de macOS.

Estado: completado y aprobado manualmente en Wayland, X11 y macOS. El cambio
queda listo para sincronizar y archivar.
