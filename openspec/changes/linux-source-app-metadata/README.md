# linux-source-app-metadata

Cambio OpenSpec para completar la metadata de aplicación origen en Linux.

El cambio reutiliza el probe EWMH/`WM_CLASS` y el contrato de
`ApplicationMetadataProvider` ya existentes. Añade resolución de archivos
`.desktop` e iconos locales para Linux X11 y para aplicaciones X11 ejecutadas
en una sesión Wayland mediante XWayland.

En Wayland nativo no inventa una aplicación origen: devuelve una capacidad
tipada como no disponible y conserva el comportamiento seguro del blacklist.

Estado: implementado y versionado como `0.0.1`.

No modifica macOS, no cambia la captura o el pegado, no introduce red,
telemetría, contenido de clipboard en logs ni una extensión de GNOME.

## Causa raíz confirmada

La prueba manual falló tanto en X11 como en GNOME Wayland. La regresión
compartida fue el probe EWMH: el código pedía la propiedad `WM_CLASS`
usando el tipo `UTF8_STRING`, mientras que el ICCCM define `WM_CLASS` como
una lista de `STRING` (Latin-1). El servidor X devolvía `type = NONE` y la
caché quedaba vacía, así que el provider Linux nunca recibía un
identificador estable y el blacklist, la metadata y los iconos nunca
llegaban a resolverse. La corrección cambia el probe a `STRING` (con
fallback a `AnyPropertyType`) y mantiene `_NET_WM_NAME` en `UTF8_STRING`.

## Cambios adicionales del cambio

- El provider Linux de `.desktop` recorre ahora el árbol canónico de
  iconos XDG (`<root>/icons/<theme>/<size>x<size>/apps/`,
  `<root>/icons/<theme>/scalable/apps/`) además del layout legacy
  `<root>/icons/<size>x<size>/apps/`. La distribución real de Ubuntu
  instala los iconos bajo `/usr/share/icons/hicolor/<size>x<size>/apps/`
  y el probe anterior solo buscaba en el layout legacy.
- El desktop agrega la opción **Acerca de** al menú global de puntos
  suspensivos, reusando el patrón de modales existente. La versión se
  lee desde el comando `clipvault_diagnostics` (que a su vez lee
  `Cargo.toml`) y nunca desde un literal hardcodeado en `Svelte`.
- La política de versionado canónico queda documentada en `projects.md`
  con los tres manifests (Cargo, Tauri config, frontend package)
  sincronizados en `0.0.1`.
