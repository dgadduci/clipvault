# Design: code-language-detection

## 1. Estado actual y compatibilidad

`clipvault-core::detect_content_type` es hoy el detector de tipos estructurados
y ya contiene `ContentType::Code`. Su comportamiento y precedencia deben
seguir siendo la base: JSON, HTML, SQL, shell y las categorías específicas no
se deben degradar a `code` por introducir detección automática.

La nueva metadata es complementaria:

```text
ContentType::Code       -> code_language puede tener valor
ContentType::Text       -> code_language puede promoverse a un lenguaje sólo
                            cuando el detector local supera el umbral
Otros tipos específicos  -> code_language permanece null en esta versión
```

Las filas antiguas tienen `code_language = null` y continúan siendo válidas.

## 2. Biblioteca y ubicación

La biblioteca elegida para frontend es `highlight.js`, importada desde su
entry point modular (`highlight.js/lib/core`) y con un conjunto explícito de
gramáticas registradas. Su documentación oficial mantiene la lista de
lenguajes y soporta detección automática:

- https://github.com/highlightjs/highlight.js/blob/main/SUPPORTED_LANGUAGES.md
- https://highlightjs.org/

No se debe importar el bundle completo ni `all` sin justificar el impacto en
el bundle. La versión debe quedar fijada en el lockfile.

`tree-sitter` queda fuera de esta implementación: sería apropiado para AST,
edición incremental o análisis estructural, pero agrega parsers/WASM y no es
necesario para esta primera clasificación visual.

## 3. Modelo persistido

La migración siguiente a la versión vigente debe agregar de forma aditiva:

```sql
ALTER TABLE clipboard_entries ADD COLUMN code_language TEXT;
CREATE INDEX IF NOT EXISTS idx_clipboard_entries_code_language
  ON clipboard_entries (code_language);
```

La migración debe ser reversible mediante el patrón de reconstrucción de tabla
que ya utiliza el repositorio. No se deben reescribir contenidos ni borrar
assets.

`EntryRecord`, `NewEntry`, los mapeos SQLite/Tauri y los tipos TypeScript deben
exponer `code_language: string | null`.

El valor persistido sólo puede ser uno de los identificadores canónicos del
allowlist. El repositorio debe rechazar valores desconocidos con un error
tipado; no debe mapearlos silenciosamente a otro lenguaje.

## 4. Detección local y persistencia

El frontend debe tener un helper puro y compartido, por ejemplo
`codeLanguageDetector.ts`, que:

1. normalice alias a identificadores canónicos;
2. respete primero las señales explícitas de fence/shebang ya reconocidas;
3. no analice automáticamente filas ya clasificadas como JSON, HTML, SQL,
   shell, URL, email, JWT, IP, UUID, color o ruta;
4. utilice `highlightAuto` sólo con el allowlist registrado;
5. exija evidencia de código y un umbral de confianza determinístico;
6. devuelva `null` cuando haya empate, poca relevancia o un fragmento muy
   corto/ambiguo.

Como baseline de la primera versión, el helper debe exigir:

- al menos dos líneas o 24 caracteres no blancos;
- al menos una señal estructural de código (`{}`, `;`, `=>`, `::`, `#include`,
  `def`, `class`, `fn`, `func`, `import`, `const`, `let`, `interface`, etc.);
- una relevancia mínima fijada en el código y un margen mínimo frente al
  segundo candidato; esos valores deben quedar cubiertos por tests y fijados
  junto con la versión de `highlight.js`.

Para fences o shebangs con lenguaje reconocido se puede omitir el umbral
automático porque la señal explícita tiene prioridad.

La detección se ejecuta sobre el contenido que ya recibió la UI. El comando
de persistencia sólo recibe `entry_id` y el identificador canónico, nunca el
contenido. Debe validar que la fila existe, es textual y que el lenguaje es
válido. La escritura es idempotente y no debe sobrescribir una clasificación
existente con `null` ni con otra inferencia de menor certeza.

La integración de Desktop y Quick Paste debe compartir el detector y
coalescer solicitudes por `entry_id`. El evento de historial continúa siendo
metadata-only; no se agregan payloads con contenido.

## 5. Servicio y comando Tauri

La lógica de validación y persistencia debe vivir en `clipvault-core`/`db`.
Tauri sólo expone un comando delgado, por ejemplo
`clipvault_code_language_set`, con:

```text
{ entry_id: number, code_language: string }
```

La respuesta debe ser metadata-only (`updated`, `unchanged`, `invalid` o
error tipado). No debe devolver ni registrar el contenido.

Si el tipo actual es `text` y la clasificación aceptada supera el umbral, el
servicio puede actualizarlo a `code`. Si el tipo ya es `code`, sólo completa o
conserva `code_language`. Los tipos específicos existentes no se cambian por
una inferencia automática.

## 6. Presentación

La card compacta conserva el cuerpo de texto actual para no cambiar su
geometría ni introducir HTML en cada fila. Su metadata puede mostrar
`Código · Python` usando el helper de etiquetas.

`ClipboardPreview.svelte` debe resaltar código usando el lenguaje persistido,
con escape/sanitización segura y sin guardar el HTML generado. Quick Paste y
Desktop deben llamar al mismo componente/helper de preview ya compartido.

Si `code_language` es `null`, la preview usa texto plano. Si el lenguaje no
está disponible en el bundle, también usa texto plano sin romper la captura.

## 7. Privacidad y rendimiento

- Todo el procesamiento es local.
- No se agregan requests HTTP, logs de contenido, hashes, snippets ni paths.
- El HTML generado por el highlighter no se persiste.
- La detección sólo se ejecuta para filas nuevas o sin metadata de lenguaje.
- Debe existir guard de solicitudes duplicadas y de respuestas obsoletas.
- El tamaño de la entrada analizada debe estar limitado de forma segura para
  evitar bloquear el hilo de UI con capturas enormes.

## 8. No-regresiones obligatorias

La implementación no debe cambiar la detección de URL, email, JSON, HTML,
SQL, shell, JWT, UUID, IP, colores ni rutas. Debe preservar imágenes, rich
text, títulos, tags, colecciones, favoritos, búsqueda por título, drag and
drop, Quick Paste, preview compartida y persistencia tras reinicio.
