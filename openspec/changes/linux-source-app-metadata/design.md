# Diseño: linux-source-app-metadata

## Principio arquitectónico

La aplicación origen debe atravesar el mismo flujo que ya funciona en macOS:

```text
active-app probe
    -> source identifier (WM_CLASS class)
    -> ApplicationMetadataProvider::lookup
    -> source_app_name / source_app_icon_ref
    -> SQLite existente
    -> bridge de iconos existente
    -> HistoryCard / Quick Paste existentes
```

El core no debe conocer X11, Wayland, archivos `.desktop` ni rutas del sistema.
La resolución de plataforma vive en `clipvault-platform`; el shell solamente
selecciona el adapter compatible. El provider sigue devolviendo
`ApplicationMetadata` y `ApplicationMetadataError` existentes, salvo que una
extensión tipada del diagnóstico sea estrictamente necesaria.

## Matriz de sesión

| Sesión | Ventana activa | Backend de active app | Resultado |
|---|---|---|---|
| Linux X11 | X11 | EWMH + `WM_CLASS` | nombre/icono cuando existe `.desktop` |
| GNOME Wayland + XWayland | aplicación X11 | EWMH sobre `DISPLAY` | nombre/icono de la ventana X11 |
| GNOME Wayland nativo | aplicación Wayland | no portable en este cambio | indisponibilidad tipada, sin datos inventados |
| Sin display usable | ninguna | no disponible | historial sigue funcionando; origen desconocido |

La presencia de `DISPLAY` en Wayland sólo habilita el intento XWayland. El
probe debe aceptar una respuesta únicamente si obtiene una ventana X11 real y
un identificador no vacío. No se debe presentar una ventana Wayland nativa
como si fuera X11.

## Selección del active-app probe

- En `DisplayServer::X11`, conservar `X11ActiveApplication` existente.
- En `DisplayServer::Wayland`, intentar construir el adapter X11 solamente
  cuando `DISPLAY` esté definido y la conexión sea válida. Si no conecta o no
  hay una ventana X11 activa, conservar el fallback `NoopActiveApplicationProbe`.
- Mantener la cache existente y su contrato de hilo. El watcher nunca debe
  invocar desde un hilo incorrecto un adapter que requiera otro hilo.
- El identificador que alimenta el privacy gate y `source_app` sigue siendo
  el segmento `class` de `WM_CLASS`, con el fallback actual al segmento
  `instance` sólo cuando corresponde. No usar el título de la ventana como
  identificador estable.
- El backend de diagnóstico debe poder diferenciar al menos `x11_ewmh`,
  `xwayland_ewmh` y `unavailable`, sin romper la deserialización existente.

## Proveedor Linux de metadata

Crear un provider detrás del trait existente, por ejemplo
`LinuxApplicationMetadataProvider`, parametrizado por el directorio de
assets y por un lector de filesystem inyectable para tests cuando sea
necesario.

### Búsqueda de archivos `.desktop`

La búsqueda debe ser determinista y no ejecutar el campo `Exec`:

1. `$XDG_DATA_HOME/applications` si está definido; si no, usar
   `$HOME/.local/share/applications`.
2. Cada directorio `applications` bajo las entradas de `$XDG_DATA_DIRS`.
3. Fallbacks estándar `/usr/local/share/applications` y
   `/usr/share/applications` cuando no estén ya incluidos.

El parser sólo necesita las claves del grupo `[Desktop Entry]` que afectan a
este contrato: `Type`, `Hidden`, `Name`, `Name[locale]`, `Icon`,
`StartupWMClass` y `X-GNOME-WMClass`. Debe ignorar comentarios, grupos ajenos,
entradas `Hidden=true` y archivos que no sean `Type=Application`. `NoDisplay`
no implica que el metadata no pueda resolverse.

La prioridad de coincidencia, siempre con comparación normalizada sin
importar mayúsculas, será:

1. `StartupWMClass` exacto.
2. `X-GNOME-WMClass` exacto.
3. nombre del archivo `.desktop` sin extensión.
4. identificador normalizado cuando el archivo lo declare de forma
   inequívoca.

Si hay varios candidatos con la misma prioridad, elegir el path lexicográfico
menor. Nunca usar el título de ventana, `Exec` completo o una coincidencia
parcial ambigua.

El nombre visible debe preferir `Name[<locale>]` compatible con la locale del
proceso, después `Name` y finalmente devolver `None` si no hay un nombre no
vacío. No se debe almacenar el contenido del archivo `.desktop` en SQLite.

## Resolución y persistencia del icono

- `Icon=/ruta/icono.png` se resuelve como path absoluto sólo después de
  validarlo como archivo regular local que viva bajo una raíz XDG
  permitida.
- `Icon=nombre` se resuelve recorriendo el árbol canónico de iconos
  XDG en este orden:
  1. `<root>/icons/<theme>/<size>x<size>/apps/<name>.png`
  2. `<root>/icons/<theme>/scalable/apps/<name>.png`
  3. layout legacy `<root>/icons/<size>x<size>/apps/<name>.png`
  Los temas se descubren dinámicamente con `hicolor` añadido al
  final como fallback determinístico. La distribución real de Ubuntu
  instala los iconos en `/usr/share/icons/hicolor/<size>x<size>/apps/`
  y el árbol canónico es la única ruta que los encuentra.
- Solo se aceptan PNGs validados con la firma canónica
  (`\x89PNG\r\n\x1a\n`). El provider no rasteriza SVG ni invoca
  procesos externos (`convert`, `gio`, `magick`, …) — esa limitación
  está documentada en `tasks.md` y se justifica porque las
  aplicaciones objetivo de las pruebas usan PNG.
- El resultado se guarda como PNG bajo
  `<data_dir>/assets/application-icons/<safe-identifier>.png` y se persiste
  como referencia relativa mediante `icon_ref_for` o un helper compartido.
- La escritura debe ser atómica: crear temporal dentro del mismo directorio,
  escribir completamente, validar firma/tamaño/dimensiones y renombrar al
  destino. Limpiar temporales ante error.
- El provider puede devolver el nombre aunque el icono no pueda resolverse.
  La falta del icono nunca debe convertir una captura válida en `Failed`.
- No devolver paths absolutos al core, Tauri, frontend, logs ni eventos. El
  bridge existente sigue siendo el único lector de bytes.
- No sobrescribir con un icono vacío una referencia válida ya persistida.

## Probe X11/XWayland

- `WM_CLASS` se consulta como `STRING` (con fallback a
  `AnyPropertyType`). El ICCCM define `WM_CLASS` como lista de
  cadenas Latin-1; pedirla como `UTF8_STRING` hacía que el servidor X
  no devolviera bytes para ventanas GTK/Qt y la cache quedaba
  vacía.
- `_NET_WM_NAME` se consulta como `UTF8_STRING` (con fallback a
  `AnyPropertyType`); el EWMH define este propiedad como texto
  Unicode.
- Cada respuesta se valida con `format == 8` y `bytes_after == 0`
  antes de devolverla; si no cumple, el helper pasa al siguiente
  candidato.
- El segmento `class` de `WM_CLASS` es el identificador estable que
  alimenta el blacklist y el provider `.desktop`; cuando falta, se
  usa el segmento `instance`. Nunca se usa el título de la ventana.

## Selección del active-app probe

- En `DisplayServer::X11` se construye
  `X11ActiveApplication::with_kind(None, ProbeKind::X11)`. La sonda
  expone `name() = "x11_ewmh"`.
- En `DisplayServer::Wayland` se construye
  `X11ActiveApplication::with_kind(None, ProbeKind::XWayland)` cuando
  `DISPLAY` está definido y la conexión X11 es válida. La sonda expone
  `name() = "xwayland_ewmh"`. Si la conexión falla, el bootstrap cae
  a `NoopActiveApplicationProbe` y la matriz de capacidades reporta
  `active_application = false`.
- En Wayland nativo sin DISPLAY se omite la sonda por completo.
- La matriz de capacidades (`detect_capabilities_runtime`) no se
  modifica por este cambio — el `active_application` se calcula a
  partir del probe efectivamente instalado, no del display server
  detectado.

## Modal Acerca de

- El desktop agrega el ítem "Acerca de" al menú global de puntos
  suspensivos del toolbar. Las cards no exponen un ítem paralelo.
- El modal reusa el patrón `Modal.svelte` existente: mismo shell,
  mismo `onClose`, misma trampa de foco, mismo comportamiento de
  `Escape` y backdrop.
- La versión se lee siempre desde `diagnostics.version` (canal
  `clipvault_diagnostics`), que a su vez lee
  `Cargo.toml` `[workspace.package].version`. Nunca se usa un
  literal hardcodeado en `Svelte`.

## Captura, backfill y privacidad

- `PrivacyGate` debe ejecutarse antes de cualquier lectura o escritura del
  icono, igual que antes de cualquier otro asset.
- La captura conserva `source_app` como identificador de matching y sólo
  completa `source_app_name` / `source_app_icon_ref` mediante el provider.
- Reutilizar `enrich_metadata` y el backfill existente. El backfill debe ser
  acotado, idempotente y no modificar `content`, hashes, timestamps de captura,
  `asset_ref`, dimensiones, tags, colecciones ni favoritos.
- En Wayland nativo sin identificador, conservar `source_app = NULL` o el
  valor ya existente y aplicar el contrato actual de origen desconocido. No
  convertir `unavailable` en un nombre fijo como "Wayland" o "GNOME".
- Logs y errores sólo pueden contener categorías técnicas estables. No
  registrar clipboard, snippets, hashes de contenido, paths absolutos,
  contenido de `.desktop` ni bytes de iconos.

## Frontend y compatibilidad

No crear un bridge nuevo para Linux. Las cards y Quick Paste deben seguir
recibiendo `source_app_name` y `source_app_icon_ref` por el mismo DTO y el
mismo comando de icono existentes. El fallback visual actual debe continuar
funcionando cuando el provider no encuentra `.desktop` o icono.

Si el diagnóstico necesita exponer el backend, hacerlo con un campo
metadata-only y una etiqueta clara como `X11/XWayland` o `Wayland nativo no
disponible`; no mostrar paths ni identificadores internos innecesarios.

## Tests requeridos

### Unitarios

- Parseo de `.desktop` con comentarios, grupos extra, `Hidden`, `NoDisplay`,
  `Type`, claves localizadas y escapes básicos.
- Coincidencia por `StartupWMClass`, `X-GNOME-WMClass`, nombre del archivo,
  prioridades, comparación de mayúsculas y empate determinista.
- Resolución de icono absoluto, icono por nombre, fallback de tamaños,
  formatos no soportados, archivo ausente y path fuera de los directorios
  permitidos.
- Escritura atómica, validación PNG, referencias relativas y cleanup de
  temporales.
- Provider `Send + Sync`, errores tipados y nombre sin icono.

### Integración y regresión

- Bootstrap Linux X11 instala el provider Linux y el probe X11.
- Bootstrap GNOME Wayland usa XWayland sólo cuando `DISPLAY` y una conexión
  X11 válida lo permiten.
- Wayland nativo sin ventana X11 mantiene `Unavailable` sin nombre/icono
  inventados.
- Una captura permitida persiste nombre/icono; una captura blacklisted no
  crea ni lee assets.
- Backfill de filas antiguas es acotado, idempotente y conserva todos los
  campos de contenido y organización.
- Se conservan imágenes después de reiniciar, búsqueda, cambio de colección,
  tags, pin/unpin, preview, Quick Paste y drag-and-drop.
- macOS y `platform-permission-guidance` no sufren cambios funcionales.

### Manuales Ubuntu

- Sesión X11: copiar desde una aplicación con `.desktop`, confirmar nombre e
  icono en desktop y Quick Paste, reiniciar y verificar persistencia.
- GNOME Wayland con una aplicación X11/XWayland: repetir la prueba y revisar
  el diagnóstico `xwayland_ewmh`.
- GNOME Wayland con una aplicación nativa: confirmar fallback explícito sin
  nombre/icono falso y sin romper captura, historial o blacklist.

## Verificación

El cambio debe pasar los comandos habituales del repositorio:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd app/tauri/frontend && npm run check
cd app/tauri/frontend && npm run build
cd app/tauri/frontend && npm test
openspec validate linux-source-app-metadata --strict --type change
```

Las pruebas reales de Ubuntu X11 y GNOME Wayland quedan como tareas manuales
y no pueden marcarse por inferencia desde macOS.
