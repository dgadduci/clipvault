# Diseño: edición persistente de capturas textuales

## 1. Fronteras de arquitectura

```text
HistoryCard -> EntryTextEditorModal -> Tauri thin command
                                      ↓
                           TextHistoryService
                                      ↓
                           EntryRepository transaction
                                      ↓
                                  SQLite
```

La interfaz sólo mantiene un draft y renderiza resultados. La validación, la
reclasificación, el hash, la detección de duplicados y la transacción viven en
Rust. El comando Tauri acepta el ID opaco y el texto que el usuario decidió
guardar; no registra el texto ni lo incluye en errores o eventos de diagnóstico.

## 2. Editor y compatibilidad de plataforma

La primera implementación usa un `<textarea>` nativo. Es suficiente para
texto plano, conserva las operaciones nativas de selección/IME/copy-paste y no
requiere una biblioteca ni APIs de Wayland, X11 o macOS. Tauri usa WebKitGTK en
Linux y WKWebView en macOS; Wayland y X11 afectan al shell de ventana, no al
modelo de edición DOM del frontend.

Referencias de la decisión:

- Tauri process model: <https://tauri.app/concept/process-model/>
- Tauri webview versions: <https://v2.tauri.app/reference/webview-versions/>
- HTMLTextAreaElement: <https://developer.mozilla.org/en-US/docs/Web/API/HTMLTextAreaElement>
- Input event: <https://developer.mozilla.org/en-US/docs/Web/API/Element/input_event>

El componente debe preservar exactamente los saltos de línea y el contenido
Unicode escrito. No debe aplicar `trim`, normalizaciones visuales ni
transformaciones de formato al valor guardado. El texto vacío (`length == 0`)
se rechaza; el whitespace no se elimina automáticamente. El `textarea` debe
tener nombre accesible, foco inicial, estado de guardado, error visible y
controles de cierre. Guardar es explícito; no se escribe en SQLite por cada
tecla.

CodeMirror 6 queda como alternativa futura si el producto aprueba capacidades
de editor de código; su modelo modular y sus extensiones no justifican una
dependencia para este alcance. Monaco y Tiptap/ProseMirror quedan fuera de
esta implementación por su peso o por introducir un modelo de documento rico.

## 3. Elegibilidad

Una entrada es editable cuando:

- `content_type` es uno de los tipos para los que `ContentType::is_textual()`
  devuelve verdadero;
- `asset_ref`, `mime_type`, `payload_width` y `payload_height` son `None`;
- no tiene `rich_text_hash`, `rich_html_ref`, `rich_rtf_ref` ni
  `rich_preview_ref`.

Las imágenes y las entradas con payload rico no muestran la acción `Editar
captura`. El backend vuelve a validar la elegibilidad aunque el frontend sea
stale o manipulado.

## 4. Semántica de guardado

`TextHistoryService::update_text` recibe `entry_id` y el nuevo texto. Dentro de
una única transacción:

1. carga la fila y devuelve `not_found` si no existe;
2. valida que la entrada sea editable y que el texto no sea vacío;
3. calcula el hash con el mismo helper usado por `record_text`, el tamaño en
   bytes UTF-8 y `detect_content_type` sobre el nuevo texto;
4. si otra entrada posee el mismo hash canónico, aborta con
   `duplicate_content` sin modificar ninguna fila;
5. actualiza sólo `content`, `content_type`, `content_size`, `content_hash` y
   `updated_at`;
6. conserva `id`, `created_at`, `last_seen_at`, `title`, `is_pinned`,
   `source_app*`, tags, colecciones y todos los campos de assets/rich text;
7. devuelve la fila actualizada o un resultado `noop` si el texto es idéntico.

El tipo de contenido puede cambiar de `text` a `url`, `json`, `code`, etc. La
entrada no se mueve al frente del historial porque el rail y la búsqueda
siguen ordenados por `created_at`; `updated_at` sólo expresa la mutación.
No se agrega migración: las columnas existentes ya contienen todo lo
necesario.

El repositorio debe evitar depender sólo de un error SQL de índice para detectar
duplicados y devolver un resultado estable. Si ocurre cualquier error, la
transacción hace rollback y el registro previo queda intacto.

## 5. Comando, evento y actualización de vistas

Agregar un comando thin, por ejemplo `clipvault_update_text_entry`, y un bridge
TypeScript tipado. La respuesta debe ser una unión explícita:

- `updated` con la entrada actualizada;
- `noop` con la entrada sin cambios;
- `not_found`;
- `not_editable`;
- `empty_content`;
- `duplicate_content`;
- error de persistencia seguro.

Después de `updated`, el shell emite el evento existente
`clipvault://history-updated` con payload vacío o usa el mecanismo equivalente
ya establecido para refrescar la fuente de historial. No se emite el texto en
el evento. El frontend actualiza la card desde la respuesta y/o vuelve a
hidratar la lista de forma idempotente; la búsqueda se ejecuta nuevamente
contra la fuente persistida y Quick Paste recibe el nuevo contenido por su
flujo existente.

## 6. Modal y card

`HistoryCard` conserva `data-testid="history-card"`, `data-entry-id`,
`draggable="false"`, pin, título, tags, colecciones y el controlador singleton
de drag and drop. La edición se abre desde el menú, no al hacer click en la
superficie ni al iniciar drag.

El modal debe:

- reutilizar `Modal.svelte` o el shell común;
- cargar el contenido al abrir sin mutar el `entry` original;
- dar foco inicial al `textarea`;
- ofrecer Guardar y Cancelar, con Guardar deshabilitado mientras persiste;
- cerrar con Cancelar, Escape, backdrop y botón de cierre sin guardar el draft;
- mostrar errores de validación/duplicado sin incluir el texto en logs;
- restaurar el foco al disparador del menú al cerrar;
- impedir que los eventos del `textarea` seleccionen la card o inicien drag.

Mientras guarda, el modal no debe permitir doble submit. Si el backend falla,
el draft queda visible para que el usuario pueda corregirlo o cancelar.

El estado `open` debe tener una única fuente de verdad en `HistoryCard`. El
modal no debe cerrar sólo asignando su prop local: debe despachar un evento
`close` (o usar un binding explícito) para que el padre cambie
`textEditorOpen = false`. Esto debe ocurrir después de un guardado exitoso y
también al cancelar, presionar Escape, cerrar por backdrop o usar el botón de
cierre. La transición `true -> false -> true` debe funcionar para la misma
entrada sin desmontar toda la card.

## 7. Integridad y privacidad

La edición es una mutación local intencional del contenido; no se debe tratar
como una nueva captura, no debe crear una nueva fila ni re-ejecutar el
portapapeles o PrivacyGate. No se agregan logs con texto, hashes, snippets,
paths ni bytes. Los payloads de drag siguen llevando sólo el ID opaco de la
entrada. Tags, colecciones, favoritos, source-app metadata y assets no se
tocan.

## 8. Verificación

Automática:

- repository/service: actualización, hash/tamaño/tipo, timestamps, noop,
  not-found, entrada no editable, vacío, duplicado y rollback;
- conservación de todos los metadatos y memberships;
- comando Tauri, unión de resultados, evento vacío y bridge;
- modal, foco, Escape/backdrop/Cancelar, error, doble submit y refresh de card;
- búsqueda y Quick Paste con el texto actualizado;
- regresiones de card y pointer/mouse drag-and-drop.

Manual en Ubuntu GNOME Wayland, Ubuntu X11 y macOS:

- editar texto corto, multilinea y Unicode;
- guardar, cancelar y cerrar con Escape;
- reiniciar y comprobar persistencia, búsqueda y Quick Paste;
- intentar editar imagen/rich text y confirmar que no aparece la acción;
- provocar un duplicado y comprobar que la entrada original permanece intacta.

## 9. Ajustes posteriores a la prueba manual

La prueba manual aprobó la edición persistente y la reapertura del modal. Los
siguientes ajustes son exclusivamente de presentación y teclado:

- El título visible del diálogo debe ser el título resuelto de la captura
  (`displayTitle`, incluyendo su fallback existente), no el texto genérico
  hardcodeado `Editar captura`. Se conserva el etiquetado accesible mediante
  `aria-labelledby`.
- Se elimina el párrafo auxiliar inferior del modal y cualquier
  `aria-describedby` o estilo que exista únicamente para ese párrafo. Los
  errores visibles y su anuncio accesible se conservan.
- `Ctrl+E` abre el editor desde una card textual elegible. El atajo no debe
  ejecutarse para imágenes/rich text ni cuando el foco está en un control
  interactivo, un campo editable, el menú de la card o el propio modal. La
  acción del menú muestra literalmente `Ctrl+E` y expone
  `aria-keyshortcuts="Control+E"`.

El atajo debe usar el guard existente de targets interactivos y no puede crear
listeners globales ni modificar el controlador singleton de drag-and-drop.
