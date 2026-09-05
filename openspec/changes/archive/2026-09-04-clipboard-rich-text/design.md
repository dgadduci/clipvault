# Design: clipboard-rich-text

## 1. Límites de arquitectura

El flujo debe conservar la separación existente:

```text
OS clipboard
  -> platform ClipboardBackend
  -> neutral ClipboardPayload
  -> core normalizer + privacy + dedupe
  -> SQLite + local rich-text assets
  -> thin Tauri commands
  -> HistoryCard / menu
```

El core no importa Tauri ni tipos del navegador. El frontend no lee SQLite ni
decide cómo se detecta, normaliza, deduplica o pega un formato.

## 2. Modelo neutral y prioridad

Extender `clipvault-platform::ClipboardPayload` con un valor rich textual, sin
eliminar `Text` ni `Image`. La forma conceptual es:

```text
RichText {
  plain_text: String,
  html: Option<String>,
  rtf: Option<Vec<u8>>,
}
```

El valor es válido sólo si `plain_text` no está vacío y al menos `html` o
`rtf` contiene una representación soportada. Las implementaciones pueden
exponer sólo una de las dos representaciones.

`ClipboardBackend` debe añadir operaciones opcionales y capacidades separadas
para lectura y escritura rich. El algoritmo común de `read_payload` queda:

1. intentar rich text;
2. si no hay rich válido, intentar texto plano;
3. si no hay texto usable, intentar la imagen existente;
4. si nada es compatible, devolver `None`/unsupported sin detener el watcher.

Una respuesta rich con texto plano cuenta como contenido textual y por ello
gana frente a una imagen copiada junto con la representación textual. El
backend no debe llamar dos veces a APIs nativas que puedan cambiar el
clipboard entre representaciones; cuando sea necesario, debe tomar una
instantánea consistente detrás de su adapter.

La lectura rich debe ser best-effort: un fallo de HTML no debe descartar un
RTF válido ni impedir el fallback a `Text`. Un error real del backend se
traduce al error tipado existente sin incluir bytes ni contenido.

## 3. Normalización, preview y seguridad

El core conserva los bytes/strings originales para pegado y genera una
representación de preview independiente. La normalización de preview debe:

- conservar texto, párrafos, saltos, listas, alineación, bloques de código y
  atributos de fuente/color/peso/cursiva/subrayado/tachado que puedan
  expresarse de forma segura;
- aplicar una allow-list explícita de elementos y propiedades CSS, con límites
  de tamaño y profundidad;
- eliminar `script`, `style` arbitrario, handlers `on*`, `iframe`, `object`,
  `embed`, formularios, URLs `javascript:`, recursos remotos, data URLs de
  imágenes y objetos/adjuntos embebidos;
- producir HTML válido aun cuando el clipboard entregue markup incompleto;
- usar texto plano como preview si la sanitización falla.

La implementación puede reutilizar una dependencia pequeña y madura si el
parser/sanitizer actual no alcanza el contrato. Si agrega una dependencia,
MiniMax debe justificar en el diff de `design.md` por qué la biblioteca
estándar o el código existente no bastan. No se permite resolverlo con un
`{@html}` de HTML crudo en Svelte.

## 4. Persistencia y deduplicación

Crear una migración aditiva y reversible posterior a la última migración
actual. Agregar a `clipboard_entries` columnas nullable:

- `rich_text_hash TEXT`;
- `rich_html_ref TEXT`;
- `rich_rtf_ref TEXT`;
- `rich_preview_ref TEXT`;
- `rich_html_size INTEGER`;
- `rich_rtf_size INTEGER`.

Las filas antiguas permanecen con todos esos campos en `NULL`. `content` sigue
siendo el texto plano canónico y `content_hash` conserva su semántica histórica.
Para una fila rich, `rich_text_hash` es el SHA-256 determinístico de una
representación canónica versionada de los formatos presentes. El repositorio
debe considerar duplicado sólo cuando coincide `content_hash` y, para filas
rich, `rich_text_hash`; el mismo texto con estilos diferentes no se colapsa
silenciosamente.

`EntryRecord` y `NewEntry` exponen sólo los campos metadata/ref equivalentes.
No se devuelven HTML ni RTF originales en el listado de recientes, búsqueda,
eventos `history-updated` o diagnósticos.

## 5. Asset store

Usar un namespace separado:

```text
<data_dir>/assets/rich-text/
  <sha256>.html
  <sha256>.rtf
  <sha256>.preview.html
```

Las referencias guardadas son relativas y no contienen paths absolutos. Cada
archivo se escribe en temporal dentro del mismo directorio, se valida, se
renombra atómicamente y se reutiliza si ya existe el hash. Los límites de
tamaño deben ser finitos y configurables como constantes.

El bridge para preview acepta únicamente `rich-text/<sha256>.preview.html`,
rechaza traversal, symlinks escapados, extensiones equivocadas, archivos
demasiado grandes y contenido que no tenga el marcador/validación esperado.
El bridge para paste acepta sólo las referencias asociadas al entry solicitado
por el core; no permite que el frontend indique una ruta arbitraria.

Delete, clear y retention deben eliminar primero las filas en una transacción
y recolectar sólo assets rich sin referencias restantes. La recolección debe
ser idempotente, tolerar archivos huérfanos y nunca borrar un asset compartido.

## 6. Pasting y capacidades

Introducir un modo neutral `Plain`/`Rich` en el servicio core. Mantener la
compatibilidad del comando existente: una llamada sin modo conserva el modo
actual (plain, usado por quick-paste). La card invoca el mismo comando thin
con el modo elegido.

Para `Rich`, el adapter escribe las representaciones originales disponibles y
el `plain_text` como flavor de fallback en una sola operación lógica. Luego se
reutiliza el paste controller existente y el target previamente capturado. No
se cambia el foco ni se ejecuta pegado si no hay selección.

Para `Plain` el adapter nativo (macOS) llama a `clearContents()` antes de
publicar el texto y declara únicamente los flavors `NSPasteboardTypeString`
y `public.utf8-plain-text`. La composite clipboard enruta `write_text` al
adapter nativo cuando el flag `supports_native_plain_write` lo permite; en
otro caso cae al adapter plain (`arboard`) que en Linux X11 reemplaza
completamente la selección y descarta los targets anteriores. El contrato
plain no publica rich flavours residuales y nunca fabrica un RTF
artificial con color negro para "preservar" el formato.

Antes de cada escritura al clipboard el servicio de paste arma un
token de supresión metadata-only en un registro compartido
(`AppContext::paste_suppression`). El watcher consume el token
antes de PrivacyGate, persistencia, enriquecimiento de metadata y
emisión de `history-updated`. El token se compara con la
observación por hashes canónicos (plain SHA-256, rich canónico,
image SHA-256), expira tras un TTL corto, es de un solo uso y se
limpia en caso de fallo de la escritura o del synthetic paste.
Los hashes nunca incluyen texto, HTML, RTF, bytes ni paths.

Agregar al capability matrix, sin inferencias, `clipboard_read_rich_text` y
`clipboard_write_rich_text`. macOS, Linux X11 y Linux Wayland pueden informar
resultados distintos. La falta de soporte rich no debe deshabilitar
`clipboard_read`/`clipboard_write` plain ni producir una instrucción de
permisos que la plataforma no requiere.

## 7. Tauri y frontend

Comandos mínimos y delgados:

- extender `clipvault_paste_entry` con un modo opcional serializable;
- agregar un bridge de preview rich por referencia validada, si el bridge de
  imágenes existente no puede servir HTML;
- mantener las respuestas de captura/pegado metadata-only, con outcomes
  estables (`pasted`, `pasted_plain_fallback`, `failed`,
  `capability_unavailable`).
- propagar la nueva variante `WatchTickOutcome::Suppressed` como
  respuesta `suppressed` en `clipvault_capture_tick` y como
  categoría de diagnostics `suppressed:paste_owned`.

`HistoryCard` debe seguir siendo la card cuadrada existente. Tras
la regresión de autocaptura la card **siempre** renderiza el preview
plano canónico (`entry.content` mediante `entryPreviewText`); no
carga `rich_preview_ref`, no crea iframe sandbox, no genera
`blob:` URL ni monta el resolver rich. El tamaño fijo, el
truncamiento con `line-clamp` y el overflow siguen siendo los
mismos. Las entradas rich y plain se ven idénticas en la card.

El menú debe contener exactamente estas acciones de pegado:

- `Paste de texto enriquecido`;
- `Paste de texto plano`.

La primera queda deshabilitada para entradas sin representación rich. Ambas
usan el controller/bridge existente y cierran el menú una sola vez. Los
errores muestran una guía no sensible y no imprimen el payload.

## 8. Privacidad

La PrivacyGate se evalúa antes de persistir el payload rich o cualquier asset.
No se registran texto plano, HTML, RTF, hashes, paths, snippets ni bytes. Los
eventos sólo llevan `()` o metadata ya definida. Una captura ignorada no debe
crear archivos temporales persistentes ni una preview.

## 9. Compatibilidad y degradación

| Situación | Captura | Card | Paste rich | Paste plain |
|---|---|---|---|---|
| HTML/RTF disponibles | RichText | Preview segura | Rich o fallback tipado | Plain |
| Sólo plain disponible | Text | Preview plain | Deshabilitado | Plain |
| Rich no soportado en la sesión | Text si existe | Preview plain | Deshabilitado/fallback | Plain |
| Rich write falla | Entrada intacta | Sin cambios | Fallback plain si posible | Plain |
| Clipboard ilegible | Sin fila | Historial usable | Sin operación | Sin operación |

