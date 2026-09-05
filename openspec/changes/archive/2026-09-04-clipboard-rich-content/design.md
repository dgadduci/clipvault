## Contexto

La interfaz de clipboard actual está separada en un backend Tauri delgado,
servicios de dominio en `clipvault-core`, persistencia SQLite y cards Svelte.
El backend de clipboard expone hoy `read_text`/`write_text`; el watcher y
`PasteService` dependen de ese contrato. Las imágenes deben entrar por la
misma pipeline, conservar PrivacyGate/dedupe y usar `HistoryCard` sin crear
una galería adicional.

La implementación se limita a imágenes raster estáticas. HTML y otras
representaciones ricas pueden tener texto alternativo en el sistema, pero no
se capturan como payload binario en este cambio.

## Decisiones arquitectónicas

### 1. Payload de plataforma y compatibilidad

Agregar en `clipvault-platform` un tipo de transporte neutral, por ejemplo:

```rust
enum ClipboardPayload {
    Text(String),
    Image {
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    },
}
```

La forma exacta puede cambiar si el adapter existente ofrece una estructura
equivalente, pero debe conservar estas propiedades:

- el core no conoce `arboard`, Tauri, AppKit ni el navegador;
- los bytes viven sólo en memoria durante la lectura y escritura del payload;
- las dimensiones se validan antes de reservar o persistir datos;
- `read_text` y `write_text` existentes continúan funcionando para los
  consumidores actuales;
- `read_payload` y `write_payload` pueden tener una implementación default que
  mantenga compatibilidad con fakes y adapters que sólo soporten texto.

La prioridad de lectura es determinística: si el clipboard entrega texto no
vacío, se captura como texto para preservar el comportamiento actual; si no
hay texto utilizable, se intenta capturar una imagen raster soportada. Un
clipboard sin texto ni imagen soportada devuelve `None`/`Ignored` y no crea
una entrada. Esta decisión evita que una representación visual acompañada por
texto plano cambie inesperadamente el historial textual.

### 2. Normalización de imágenes

El core recibe pixels RGBA y valida `width * height * 4` con operaciones
checked antes de usarlos. Un servicio de assets convierte el bitmap a PNG
canónico antes de escribirlo. El registro usa MIME `image/png`, dimensiones
originales y tamaño/hash de los bytes PNG normalizados.

La codificación PNG debe usar la dependencia pequeña y madura que ya exista en
el workspace o una nueva dependencia mínima justificada aquí. No agregar una
suite de procesamiento de imágenes sólo para renderizar un PNG. Si el backend
puede entregar PNG validado directamente, puede evitar la recodificación, pero
debe conservar un único formato de asset y los límites de seguridad.

### 3. Modelo SQLite y migración

Usar la siguiente migración aditiva y secuencial disponible después de las
migraciones existentes (previsiblemente `0008_clipboard_assets`):

- `asset_ref TEXT NULL`: referencia relativa, nunca una ruta absoluta;
- `mime_type TEXT NULL`: para esta fase sólo `image/png`;
- `payload_width INTEGER NULL`;
- `payload_height INTEGER NULL`.

`content_type`, `content_size` y `content_hash` existentes siguen siendo la
fuente de tipo, tamaño y dedupe. Para una entrada de imagen, `content_type` es
`image`, `content_hash` es el hash SHA-256 del PNG normalizado y
`content_size` es su tamaño en bytes. El campo textual `content` se mantiene
no nulo para no reconstruir innecesariamente la tabla histórica: una fila de
imagen guarda una cadena vacía como sentinel interno y sólo es válida como
imagen cuando también tiene `asset_ref` y metadata coherente. El frontend no
debe mostrar ese sentinel como texto.

La migración debe ser reversible cuando SQLite lo permita mediante la misma
estrategia de reconstrucción ya utilizada por el proyecto. Las filas antiguas
conservan `content_type = text` y todos sus valores.

`EntryRecord` agrega los campos opcionales serializables:

- `asset_ref: Option<String>`;
- `mime_type: Option<String>`;
- `payload_width: Option<u32>`;
- `payload_height: Option<u32>`.

No cambiar los nombres actuales de `content`, `source_app`, `title`,
`source_app_name` o `source_app_icon_ref`.

### 4. Dedupe y ciclo de vida de assets

El asset store local vive en:

```text
<data_dir>/assets/clipboard/<sha256-lowercase>.png
```

La referencia persistida es únicamente:

```text
clipboard/<sha256-lowercase>.png
```

El nombre se deriva del hash del PNG normalizado. Antes de escribir:

1. validar dimensiones, tamaño máximo y formato PNG;
2. crear el directorio permitido;
3. escribir a un temporal dentro del mismo directorio;
4. hacer `rename` atómico al nombre final;
5. reutilizar el archivo si ya existe y pasa la validación.

Si SQLite falla después de crear un asset, el archivo puede quedar como
huérfano seguro; el recolector posterior debe poder eliminarlo. Nunca borrar
un archivo sólo porque una operación individual falló sin comprobar las
referencias activas.

Al borrar una entrada o aplicar retención, la operación de datos debe terminar
primero y luego ejecutar una recolección local de assets no referenciados. La
recolección compara el conjunto de `asset_ref` de SQLite con los nombres de
`assets/clipboard`, ignora archivos temporales desconocidos y no atraviesa
fuera del directorio permitido. Debe ser idempotente y no afectar assets de
`ignored-apps` ni `application-icons`.

### 5. Privacidad y orden de la pipeline

El snapshot `source_app` debe obtenerse con el mecanismo existente. El
PrivacyGate se evalúa antes de crear una fila o un asset permanente. Una
captura blacklistada no debe generar PNG, metadata ni evento de historial.

El orden lógico es:

```text
source-app snapshot
    -> read payload in memory
    -> PrivacyGate
    -> normalize and hash
    -> write/reuse local asset
    -> SQLite transaction
    -> metadata enrichment
    -> history-updated
```

Si el payload es demasiado grande, está corrupto o el asset store falla, la
captura devuelve un outcome tipado y el watcher continúa. No registrar bytes,
contenido, hashes, thumbnails, rutas absolutas ni identificadores crudos en
logs o eventos.

### 6. Dedupe de texto e imagen

El hash de texto conserva su semántica actual. Para imágenes, el hash se
calcula sobre el PNG normalizado, no sobre un buffer RGBA que pueda cambiar de
representación entre plataformas. Una imagen idéntica no crea otra fila ni
otro asset; una imagen distinta crea una entrada nueva. El orden de recientes,
favoritos y timestamps sigue las reglas existentes.

### 7. Capacidades de plataforma y paste

Extender la matriz de capacidades con lectura/escritura de imagen, usando
nombres estables como `clipboard_read_image` y `clipboard_write_image` o un
equivalente documentado. No marcar una capacidad como disponible sólo porque
`read_text` funciona.

El adapter real puede reutilizar `arboard` si su API cubre el host. La
implementación debe comprobar el resultado real en macOS, Linux X11 y Linux
Wayland; si el entorno no soporta la operación, devolver el error typed
existente `Unavailable`/`Unsupported` con la capability correspondiente.

**La detección de capacidades no debe tener efectos secundarios.** Una
revisión inicial de este cambio construía un handle de `arboard` dentro
del probe para "comprobar el resultado real". En macOS eso resultó
observablemente side-effecting: crear el handle inicializa AppKit, y eso
altera la respuesta de `CGPreflightPostEventAccess` para el resto del
proceso, de modo que detectar la capacidad de imagen cambiaba la
capacidad `synthetic_paste`. El probe quedó por lo tanto como función
pura de (build, sesión):

- ¿está enlazado un adapter capaz de imágenes? (feature
  `clipboard-arboard`);
- ¿la sesión puede transportar imágenes? (macOS y Linux X11 sí; Wayland,
  display server desconocido, Windows y `Other` no).

Esto sigue cumpliendo el contrato: la disponibilidad **no** se deriva de
que `read_text` funcione, y Wayland nunca hereda el comportamiento de
X11. La comprobación del "resultado real" vive donde puede hacerse sin
efectos secundarios: en la operación. El adapter devuelve
`Unavailable`/`UnsupportedFormat`/`Backend` cuando una lectura o
escritura concreta falla, y `PasteService` convierte eso en
`CapabilityUnavailable` con guidance. La matriz es la respuesta
conservadora previa; el adapter es la autoridad en el momento de operar.

`PasteService` debe escribir el payload de imagen y después invocar el mismo
`PasteController`. Si escribir imagen no está disponible, devolver
`CapabilityUnavailable` con guidance reutilizable de
`platform-permission-guidance` cuando sea accionable. Wayland no debe recibir
un falso mensaje de permiso si la limitación es estructural o de sesión.

No convertir automáticamente una imagen a texto ni modificar la entrada
histórica en caso de fallo.

### 8. Tauri y bridge de assets

Mantener los comandos delgados. `clipvault_capture_tick`, el bucle automático,
`clipvault_recent_entries` y `clipvault_paste_entry` deben reutilizar los
servicios del core.

Agregar un comando de lectura de asset, por ejemplo
`clipvault_clipboard_asset`, que reciba sólo `asset_ref`, valide namespace,
traversal, symlinks, existencia, PNG, dimensiones y tamaño, y devuelva bytes
al frontend. Nunca devolver paths absolutos.

Los eventos de `history-updated` continúan llevando payload vacío/metadata-only.

### 9. Cards y quick-paste

Reutilizar `HistoryCard.svelte` y `HistoryCardRail.svelte`:

- una entrada textual conserva su preview actual;
- una entrada `image` usa el asset bridge y un Blob URL para el thumbnail;
- el thumbnail respeta el tamaño fijo de la card, object-fit y recorte seguro;
- si falla la carga, mostrar un fallback visual accesible sin mostrar bytes ni
  el sentinel de `content`;
- el icono y nombre de aplicación fuente mantienen el contrato icon-only
  actual;
- pin/unpin, menú, título y delete siguen funcionando.

Quick-paste puede mostrar un thumbnail de imagen y seleccionarla con las
 mismas teclas. La ventana debe ocultarse antes del paste y reutilizar la
 guidance existente en errores.

La búsqueda local sigue indexando sólo entradas textuales; no se implementa
OCR ni búsqueda por bytes.

### 10. Dependencias

Inspección realizada sobre `Cargo.toml`, `Cargo.lock` y las capacidades
de `arboard` antes de decidir.

**Clipboard de imágenes: `arboard` (ya presente, sin cambios).**
`arboard 3.6` ya es el adapter de texto para macOS y Linux X11 y su
feature `image-data` está activa por defecto — confirmado en
`Cargo.lock`, donde `arboard` depende de `image`. Esa feature expone
exactamente las dos operaciones necesarias, `get_image` y `set_image`,
ambas en RGBA directo. No se agrega ninguna dependencia nueva para el
transporte de imágenes y se conserva un único camino de negociación de
flavours por host.

Nota importante para la matriz de capacidades: ClipVault enlaza
`arboard` **sin** la feature `wayland-data-control` (no aparece
`wl-clipboard-rs` en `Cargo.lock`). Una sesión Wayland queda servida,
como mucho, vía XWayland, de modo que no existe una ruta de imagen
verificable. Por eso `clipboard_read_image` y `clipboard_write_image`
se reportan no disponibles en Wayland: es una limitación estructural de
la sesión, no un permiso que el usuario pueda conceder.

**Codificación/decodificación PNG: `png = "0.17"` (nueva dependencia
directa, sin código nuevo de terceros).**

- *Motivo:* el asset store necesita un PNG canónico y determinista —
  los mismos píxeles deben producir siempre los mismos bytes, porque el
  hash de esos bytes es a la vez la clave de dedupe y el nombre del
  archivo. También hace falta decodificar de vuelta a RGBA para el
  pegado.
- *Por qué no alcanza la biblioteca estándar:* `std` no incluye
  compresión DEFLATE ni el contenedor PNG (chunks, CRC, filtros por
  scanline). Implementarlo a mano sería criptográficamente irrelevante
  pero sí una superficie de bugs de memoria y de compatibilidad
  innecesaria.
- *Por qué no alcanza la API existente:* `arboard` entrega y acepta
  RGBA en memoria; no expone un encoder PNG reutilizable.
- *Por qué `png` y no `image`:* `image` es una suite de procesamiento
  (escalado, filtros, decenas de formatos) y el cambio no la necesita.
  `png` es el códec pequeño, maduro y puro-Rust que la propia `image`
  usa por debajo.
- *Impacto real en el build:* ninguno en términos de código nuevo.
  `png 0.17` ya está en `Cargo.lock` de forma transitiva (vía `image`,
  que arrastra `arboard`, y vía el manejo de iconos de `tauri`);
  declararla como dependencia directa de `clipvault-core` sólo la hace
  explícita.
- *Alternativa descartada:* pedirle al backend un PNG ya validado. Se
  descartó porque `arboard` no lo garantiza en todos los hosts y
  obligaría a confiar en bytes de origen desconocido; además rompería el
  requisito de un único formato de asset con límites propios.

**Hash: `sha2 = "0.10"` (nueva dependencia directa, sin código nuevo de
terceros).**

- *Motivo:* el nombre del asset y la clave de dedupe de imagen deben ser
  estables entre reinicios y entre plataformas. El `DefaultHasher` que
  usa el dedupe de texto no ofrece esa garantía (`std` documenta
  explícitamente que su salida puede cambiar entre versiones), así que
  no puede derivar un nombre de archivo persistente.
- *Impacto real en el build:* `sha2 0.10` ya está en `Cargo.lock` vía
  `tauri`. Su superficie transitiva se limita a `digest`.
- *Alternativa descartada:* reutilizar `hash_content`. Se descartó por
  la inestabilidad documentada de `DefaultHasher` y por su tamaño de 64
  bits, insuficiente para nombrar archivos sin riesgo de colisión.

El dedupe de texto **no** cambia: sigue usando `hash_content` para no
alterar el historial de instalaciones existentes.

No se agregan dependencias de frontend para imágenes: el thumbnail se
resuelve con `Blob` y `URL.createObjectURL`, que ya son APIs del
webview, y reutiliza el resolver de `lib/iconResolver.ts`.

## Contratos no negociables

- local-only y offline-first;
- no logs con contenido, bytes, hash, snippet ni paths absolutos;
- PrivacyGate antes de persistencia permanente;
- migración aditiva y compatible;
- no acceso del frontend a SQLite;
- no rediseñar las cards: la imagen usa la base existente;
- no tocar la validación manual pendiente de
  `platform-permission-guidance`.

## Verificación manual

En macOS, con una build actualizada:

1. Abrir una aplicación que permita copiar una imagen.
2. Copiar una imagen y esperar la captura automática.
3. Confirmar que aparece una card cuadrada usando el mismo rail de texto, con
   thumbnail, icono `Image`, título, aplicación fuente y acciones actuales.
4. Confirmar que el texto de una captura textual sigue funcionando igual.
5. Copiar dos veces la misma imagen y confirmar que no aparece una fila ni un
   asset duplicado.
6. Reiniciar ClipVault y confirmar que el thumbnail persiste.
7. Abrir quick-paste, seleccionar la imagen y confirmar que se pega en una
   aplicación receptora.
8. Eliminar la entrada y comprobar que el asset deja de estar referenciado.
9. Probar una captura desde una aplicación blacklistada y confirmar que no se
   crea card ni asset.

En Linux, repetir lo que soporte la sesión X11/Wayland y confirmar que una
capacidad no disponible genera guidance sin romper texto ni la UI.
