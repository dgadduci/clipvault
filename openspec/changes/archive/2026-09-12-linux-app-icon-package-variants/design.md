# Diseño: aliases seguros y raíces de iconos de paquetes

## Diagnóstico

El cambio `linux-wayland-desktop-file-icons` corrigió la forma canónica de un
Desktop File ID con sufijo `.desktop`, pero el resultado manual dejó dos casos
válidos fuera de esa forma:

```text
fuente activa              entrada local                  icono
firefox_firefox.desktop   firefox_firefox.desktop        /snap/.../default256.png
xterm                      debian-xterm.desktop           mini.xterm
```

Firefox tiene una identidad `.desktop` válida, pero su `Icon=` es una ruta de
payload de Snap. xterm tiene un icono de tema válido, pero el nombre de la
entrada contiene un prefijo de distribución y no declara un WM_CLASS. Ambos
casos requieren resolver metadata local, no ampliar la identidad que llega
desde la plataforma.

## Flujo

```text
source_app original
        │
        ├─ termina en .desktop → Desktop File ID exacto
        └─ sin .desktop        → StartupWMClass
                                  X-GNOME-WMClass
                                  filename stem exacto
                                  Exec basename exacto
                                          ↓
                            DesktopEntry local
                              ├─ Icon=name → XDG theme/pixmaps
                              └─ Icon=/path → raíz local permitida
                                          ↓
                            PNG validado + asset relativo
```

El valor original se conserva para PrivacyGate, persistencia, blacklist,
diagnósticos y el nombre del asset. Los aliases sólo existen en memoria
durante la resolución.

## Matching

### Alias `Exec=`

`DesktopEntry` incorpora un campo interno `exec_basename` derivado únicamente
de la primera palabra de `Exec=` dentro de `[Desktop Entry]`.

El parser acepta sólo una forma segura y determinista:

- elimina espacios iniciales y toma el primer token sin interpretar shell;
- acepta un basename simple o una ruta absoluta y conserva sólo su basename;
- rechaza tokens vacíos, códigos de campo, comillas incompletas y metacaracteres
  de shell;
- nunca ejecuta el valor, consulta el proceso ni registra el contenido.

El orden para identificadores sin `.desktop` queda:

1. `StartupWMClass`;
2. `X-GNOME-WMClass`;
3. filename stem exacto;
4. `exec_basename` exacto.

La comparación es case-insensitive ASCII, igual que los matchers existentes.
No se acepta `debian-xterm` por ser un prefijo de `xterm`, ni se elimina un
prefijo/sufijo para fabricar una coincidencia. Si varias entradas coinciden,
se mantienen la precedencia XDG y el desempate lexicográfico dentro de la
misma raíz ya definidos por el provider.

Un identificador que termina en `.desktop` sigue siendo estricto: si no existe
un Desktop File ID igual, no cae a `Exec=` ni a un filename stem distinto.

`MatchStrategy::ExecBasename` se expone como `exec_basename`. No se exponen el
comando, la ruta del ejecutable, el ID completo, la ruta del icono ni el
contenido del archivo.

## Raíces absolutas de iconos

La resolución de `Icon=/ruta` reutiliza la validación y escritura existentes,
pero recibe un conjunto de raíces permitido que contiene:

- las raíces XDG de iconos y pixmaps ya calculadas;
- raíces de payload de paquetes locales reconocidas por el adaptador Linux,
  incluyendo `/snap` y `/var/lib/snapd` cuando existen;
- el equivalente local de raíces Flatpak sólo cuando el adaptador lo detecta
  explícitamente.

La lista no incluye `/`, `/tmp` ni el home completo. El provider debe:

1. canonicalizar el candidato;
2. comprobar que es archivo regular;
3. comprobar que queda bajo una raíz permitida;
4. clasificar PNG/SVG y reutilizar rasterización, validación y escritura
   atómica actuales.

Symlinks que escapan de la raíz, archivos ausentes, formatos no soportados y
errores de lectura mantienen el fallback actual. Un rechazo por raíz se
diagnostica como `out_of_roots`; nunca se registra la ruta.

Las raíces de paquetes son una política local del provider/adaptador y no
cruzan la frontera de Tauri, Svelte o GNOME Shell. No se agrega un permiso ni
una dependencia.

## Persistencia y UI

`source_app`, `source_app_name` y `source_app_icon_ref` conservan su contrato.
El asset se guarda en `application-icons/` usando el identificador original,
por lo que no se renombran ni limpian assets previos. PrivacyGate y blacklist
se ejecutan antes de cualquier lectura de `.desktop` o icono.

Desktop y Quick Paste continúan recibiendo únicamente la referencia relativa
controlada. Si la resolución falla, muestran el fallback accesible actual; no
se agregan campos, rutas absolutas ni bytes al DTO de las cards.

## Pruebas

Fixtures mínimos representativos:

1. `firefox_firefox.desktop` con `Icon=/snap/firefox/current/default256.png`
   y un PNG bajo `/snap`; resuelve nombre, estrategia `desktop_file_id` e
   icono.
2. La misma entrada con el archivo bajo `/tmp` o un symlink que escapa;
   conserva el nombre pero no persiste icono.
3. `debian-xterm.desktop` con `Exec=xterm`, `Icon=mini.xterm` y un SVG/PNG
   bajo `icons/hicolor`; `lookup("xterm")` usa `exec_basename`.
4. Un identificador `xterm-extra` no coincide por substring y un `foo.desktop`
   desconocido no cae a aliases.
5. Un candidato con `StartupWMClass` conserva prioridad sobre el alias
   `Exec=`; un filename stem exacto conserva prioridad sobre `Exec=`.
6. XDG duplicado y raíces de paquetes duplicadas conservan el orden de
   precedencia sin ordenar globalmente las raíces.
7. Captura permitida, blacklist y backfill preservan contenido, hashes,
   imágenes, tags, colecciones, favoritos y assets existentes.

La regresión frontend de drag-and-drop se ejecuta porque el pipeline de
metadata y el bootstrap participan del cambio, aunque no se modifiquen cards.

## Decisión de cierre

La entrega se cierra explícitamente con Firefox Snap como caso funcional
aceptado. La sesión manual detectó su nombre e icono. xterm mantiene el
fallback sin icono; se registra como limitación conocida aceptada y no se
abre otro ciclo de implementación para este cambio.
