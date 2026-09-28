# kde-wayland-clipboard-capture

Investigar y corregir el caso reportado en Arch Linux con KDE Plasma y Wayland:
las copias del sistema no llegan al historial de ClipVault, mientras que la
captura funciona en macOS y en Ubuntu GNOME tanto con Wayland como con X11.

La revisión estática encontró una hipótesis concreta que aún necesita
confirmación en runtime: `arboard 3.6.1` ofrece su backend Wayland mediante la
feature opcional `wayland-data-control`, pero ClipVault no parece activarla; a
la vez, el contador XFixes del adapter Linux sólo observa selecciones X11. El
spec exige comprobar ambas rutas antes de atribuirles la causa raíz.

Estado: implementación completada. Verificación manual 4.1 aprobada por el
usuario en Arch Linux KDE Plasma Wayland: las capturas desde distintas
aplicaciones aparecen en la lista de historial. Las verificaciones manuales
4.2–4.4 siguen pendientes.
