# Propuesta: iconos Linux para Desktop File IDs en Wayland

## Problema

La integración opcional GNOME Wayland ya establece el canal local y comunica
únicamente el identificador de la aplicación enfocada. La validación manual
confirmó `accepted` → `identified` y publicó `window:6`.

GNOME define el `id` de `Shell.App` como un nombre de archivo `.desktop` o
como un valor especial `window:<id>` para una aplicación respaldada únicamente
por una ventana. En el segundo caso no existe una asociación `.desktop`, por
lo que no hay un icono local seguro que ClipVault pueda resolver. El proveedor
actual de Linux, en cambio, fue diseñado alrededor de `WM_CLASS`: compara el
identificador con `StartupWMClass`, `X-GNOME-WMClass` y el nombre del archivo
sin extensión. Por eso un Desktop File ID estándar como
`org.mozilla.firefox.desktop` no tiene por qué alcanzar el `Icon=` de su
archivo `.desktop` aunque sea una identidad válida de GNOME Wayland.

ClipVault ya detecta el origen de capturas en X11 y Wayland, pero las cards
Wayland no muestran el icono de la aplicación. Este cambio corrige el puente
entre Desktop File ID y el provider local; no convierte un identificador de
ventana efímero en una aplicación atribuida.

## Objetivo

1. Distinguir un Desktop File ID de un identificador especial `window:*` en
   la extensión GNOME, sin transportar datos adicionales.
2. Resolver Desktop File IDs válidos contra el archivo `.desktop` local y su
   `Icon=` mediante el `LinuxApplicationMetadataProvider` existente.
3. Mantener el valor original de `source_app` para PrivacyGate, persistencia y
   diagnósticos; la normalización sólo ocurre dentro del lookup de metadata.
4. Reutilizar el namespace seguro `assets/application-icons/`, el bridge de
   PNG y los fallbacks actuales de Desktop y Quick Paste.
5. Mantener una degradación honesta: una app window-backed, sin `.desktop` o
   sin icono resoluble conserva captura y fallback, sin nombre ni icono
   inventados.

## Alcance

- Extensión GNOME: publicar ausencia cuando `Shell.App` sea window-backed o
  su id sea el formato especial `window:*`; conservar Desktop File IDs como
  `firefox.desktop`, `google-chrome.desktop` u `org.gnome.Terminal.desktop`.
- Provider Linux: agregar una coincidencia exacta, sin distinción de
  mayúsculas, entre el Desktop File ID y el archivo `.desktop`; respetar el
  orden de precedencia XDG cuando varias raíces declaren el mismo ID.
- Persistencia y UI: reutilizar los campos existentes `source_app_name` y
  `source_app_icon_ref`; las filas legacy `window:*` no se reescriben ni se
  borran, pero se excluyen del backfill para no reintentar una resolución que
  el contrato declara imposible.
- Diagnósticos metadata-only: distinguir `desktop_file_id` de los matchers
  X11 existentes y de la ausencia window-backed, sin rutas ni contenido.

## Fuera de alcance

- Enviar iconos, rutas, títulos, PID, `Exec`, bytes, hash o contenido por el
  socket de GNOME Shell.
- Inferir una aplicación a partir de `window:*`, títulos, PID, `/proc`, una
  lista de ventanas o heurísticas privadas.
- Ejecutar archivos `.desktop`, procesos externos, búsquedas de red,
  telemetría o D-Bus para extraer un icono.
- Cambiar el schema de `EntryRecord`, los campos de contenido, el algoritmo
  de PrivacyGate, los assets persistidos existentes, imágenes, búsqueda,
  tags, colecciones, favoritos, Quick Paste o drag-and-drop.
- Resolver todos los IDs posibles de apps sandboxed o sin `.desktop`; éstos
  siguen el fallback conocido.

## Resultado esperado

En GNOME Wayland, si Shell asocia la aplicación enfocada con un Desktop File
ID instalado localmente, una captura permitida conserva ese identificador y
el provider completa nombre e icono PNG controlado cuando `Icon=` es
resoluble. Desktop y Quick Paste muestran el mismo icono existente de X11.

Cuando Shell informe `window:6` u otra app window-backed, la integración
publica ausencia de aplicación en vez de una identidad efímera. La captura
continúa operativa y las cards conservan el fallback accesible, sin crear
assets ni intentar una correlación inventada.

## Referencias técnicas

- [Desktop Entry Specification — Desktop File ID](https://specifications.freedesktop.org/desktop-entry/latest-single/)
- [GNOME Shell `Shell.App`](https://gnome.pages.gitlab.gnome.org/gnome-shell/shell/class.App.html)
- [GNOME Shell `Shell.WindowTracker`](https://gnome.pages.gitlab.gnome.org/gnome-shell/shell/class.WindowTracker.html)
