# Diseño: iconos Wayland a partir de Desktop File IDs

## Diagnóstico y decisión

El canal GNOME actual publica `Shell.WindowTracker.focus_app.get_id()`. Según
la API de GNOME Shell, un `Shell.App` puede tener un id de archivo desktop o
un valor especial como `window:0xabcd`; `is_window_backed()` informa que no
hay asociación con un archivo `.desktop`. El valor manual `window:6` por tanto
prueba que el socket funciona, no que exista metadata de aplicación
resoluble.

Para una app asociada, el valor con sufijo `.desktop` es el Desktop File ID
definido por freedesktop. El provider existente lo trata como si fuera un
`WM_CLASS`: su matcher por filename compara solamente el stem, por ejemplo
`firefox`, y no puede igualar `firefox.desktop`. La corrección debe vivir en
el provider Linux; no debe transformar el identificador que el core persiste.

## Flujo resultante

```text
GNOME Shell focus_app
  ├─ Shell.App con .desktop → app_id metadata-only
  └─ window-backed / window:* → ausencia
                                ↓
cached active-app → source_app original → PrivacyGate → persistencia
                                                 ↓ permitido
                         LinuxApplicationMetadataProvider::lookup
                           ├─ Desktop File ID → Name + Icon local
                           └─ no match → fallback actual
```

La GUI continúa consumiendo únicamente `source_app_name` y una referencia
relativa `source_app_icon_ref`; nunca recibe una ruta del sistema ni un icono
desde la extensión.

## Extensión GNOME

`_resolveFocusedAppId()` conserva el uso de `Shell.WindowTracker`; no enumera
ventanas, no consulta títulos ni PID. Antes de publicar:

1. Si no hay `focus_app` o `get_id()` no devuelve una cadena no vacía, publica
   ausencia.
2. Si `is_window_backed()` existe y devuelve `true`, publica ausencia.
3. Como guard defensivo para runtimes donde esa API no esté disponible o
   lance, un id que empiece por `window:` se trata como ausencia.
4. Cualquier otro id se recorta y se publica sin convertirlo, incluyendo el
   sufijo `.desktop`.

No se usa `Meta.Window.get_title`, `get_pid`, `get_wm_class`,
`get_gtk_application_id`, `Shell.App.get_icon()` ni `Gio.Icon`: los cuatro
primeros ampliarían la fuente de identidad de manera no aprobada y los dos
últimos ampliarían el canal con iconos o rutas. El estado `identified` queda
reservado para un identificador de aplicación persistible; la ausencia
window-backed usa el estado ya existente de aplicación no activa/desconocida.

La implementación debe comprobar esta forma de API contra GNOME Shell 42.9 y
cada versión declarada en `metadata.json`, igual que el contrato actual de
`SocketClient`.

## Matching del provider Linux

### Inmutabilidad del identificador de origen

`source_app` conserva exactamente el identificador que publicó el probe. Es
el valor que recibe PrivacyGate y que queda en SQLite; no se eliminan sufijos,
no se convierten puntos ni se usan equivalencias parciales. Así una regla
existente sigue siendo predecible y no se vuelve a atribuir una captura a otra
aplicación.

El provider genera claves de búsqueda sólo en memoria:

- **Desktop File ID:** cuando el identificador termina en `.desktop`, compara
  exactamente (case-insensitive) con el Desktop File ID del candidato. Para
  el alcance inicial, las apps objetivo se encuentran directamente bajo cada
  `applications/` root y su ID es el nombre de archivo, incluido `.desktop`.
- **WM_CLASS existente:** cuando no tiene el sufijo `.desktop`, conserva la
  prioridad actual `StartupWMClass` → `X-GNOME-WMClass` → filename stem.
- **Especial `window:*`:** no genera claves y retorna `Ok(None)`; es una
  defensa también para filas heredadas creadas antes del filtro de extensión.

El matcher Desktop File ID no debe intentar quitar `.desktop` y reutilizar el
matcher de `WM_CLASS`, ni aceptar una coincidencia por prefijo. Eso evita que
una identidad declarada para una app distinta cambie de dueño.

Cuando dos archivos declaren el mismo Desktop File ID, gana la primera raíz
de datos XDG ya ordenada por el provider (`XDG_DATA_HOME`, luego
`XDG_DATA_DIRS` y fallbacks). Un desempate lexicográfico sólo puede ocurrir
dentro de la misma raíz. Esta regla sigue la especificación freedesktop y no
altera la prioridad existente de `StartupWMClass` y `X-GNOME-WMClass`.

El resultado añade el diagnóstico estable `desktop_file_id` a
`MatchStrategy`. Los errores de icono existentes permanecen tipados; ningún
diagnóstico incluye el id completo, rutas, valores `Icon=`, archivos desktop
ni bytes.

## Icono, persistencia y filas existentes

Después de seleccionar el mismo `DesktopEntry`, el provider reutiliza sin
cambios la resolución local de `Icon=`, rasterización SVG ya aprobada,
validación PNG, escritura atómica y `application-icons/`. El nombre puede
persistirse aunque el icono no sea resoluble. No se crean comandos Tauri ni
un namespace nuevo.

La clave de asset conserva el identificador de origen que ya recibe el
writer. No se renombran ni limpian assets existentes; una deduplicación entre
aliases X11 y Wayland sería un cambio separado con migración explícita.

Para no repetir I/O sin posibilidad de éxito, `pending_metadata_entries` y
la replanificación de metadata excluyen el patrón especial `window:`. Las
filas ya persistidas se conservan intactas, incluido su `source_app`; no se
ejecuta una migración que las reescriba o borre. Un identificador Desktop File
ID sí continúa siendo candidato a backfill, por lo que las capturas realizadas
antes de la corrección pueden obtener icono al reiniciar.

El orden no cambia:

`snapshot → source identifier → PrivacyGate → persistencia → metadata/icono`.

En especial, una app blacklisted no llega al provider ni puede crear un asset.

## UI y compatibilidad

Desktop y Quick Paste ya distinguen una referencia válida de su fallback; no
se requiere un componente ni bridge nuevo. La UI debe mostrar la imagen sólo
cuando el bridge existente pudo resolver un PNG local controlado. Para
Desktop File IDs con metadata, el comportamiento se vuelve idéntico al de
X11/XWayland. Para ausencia, `window:*`, `Icon=` inválido o archivo no
instalado conserva el fallback accesible actual.

No se modifican las cards, sus atributos protegidos ni el controlador
`pointerDragAndDrop.ts`. Como el bootstrap y el frontend participan del
cambio, la validación de drag-and-drop se ejecuta de nuevo.

## Pruebas y verificación

Pruebas unitarias representativas:

1. `firefox.desktop` y `org.gnome.Terminal.desktop` resuelven por Desktop File
   ID, conservando nombre e icono de la entrada.
2. Un input `firefox` preserva la prioridad actual WM_CLASS y no cambia el
   resultado X11/XWayland.
3. `window:6` no llama a filesystem ni persiste icono; una fila legacy con
   ese id no queda en el backfill pendiente.
4. Dos candidatos con el mismo Desktop File ID respetan precedencia XDG, no
   el path global lexicográficamente menor.
5. Un icono ausente conserva el nombre y el fallback; blacklist corta antes
   de lookup y asset I/O.
6. Tests de fuente/control de la extensión cubren `is_window_backed`, el
   guard `window:` y el hecho de que no se añaden campos al envelope IPC.

La prueba manual Ubuntu GNOME Wayland debe capturar desde Firefox, Chrome,
Terminal GNOME y Warp con cada app enfocada, y confirmar en Desktop y Quick
Paste nombre, icono y persistencia tras reinicio. También debe enfocar una app
window-backed (incluido el dev shell de ClipVault si reporta `window:*`) y
confirmar ausencia segura sin asset. Se registra sólo versión de GNOME,
sesión, tipo de id y resultado; nunca contenido de la captura.

## Dependencias y versión

No se agregan dependencias ni permisos. Una implementación funcional incrementa
una sola vez el patch canónico desde `0.0.13`, sincronizando manifests,
lockfiles necesarios y `projects.md`; reintentos dentro del mismo cambio no
vuelven a incrementar versión.
