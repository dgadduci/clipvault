# linux-source-app-metadata

Cambio OpenSpec para completar la metadata de aplicación origen en Linux.

El cambio reutiliza el probe EWMH/`WM_CLASS` y el contrato de
`ApplicationMetadataProvider` ya existentes. Añade resolución de archivos
`.desktop` e iconos locales para Linux X11 y para aplicaciones X11 ejecutadas
en una sesión Wayland mediante XWayland.

En Wayland nativo no inventa una aplicación origen: devuelve una capacidad
tipada como no disponible y conserva el comportamiento seguro del blacklist.

Estado: propuesto, pendiente de implementación.

No modifica macOS, no cambia la captura o el pegado, no introduce red,
telemetría, contenido de clipboard en logs ni una extensión de GNOME.
