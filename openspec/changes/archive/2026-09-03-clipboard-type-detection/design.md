## Context

ClipVault ya captura texto, lo deduplica por hash y lo persiste en SQLite
con migraciones explícitas. Hoy la captura guarda cualquier payload con
`content_type = 'text'`, lo que oculta información útil para
funcionalidades futuras (acciones contextuales por tipo, filtros de
búsqueda, snippets con variables) y obliga al frontend a adivinar el
tipo en cada render.

El estado actual separa el dominio en crates (`clipvault-core`,
`clipvault-db`, `clipvault-platform`, `clipvault-search`) más una GUI
Svelte en Tauri. Las reglas arquitectónicas prohíben duplicar reglas
de negocio entre core y frontend, acceder directamente a SQLite desde
la GUI o incluir lógica sustantiva en los comandos Tauri. Esta
propuesta mantiene esa separación: el detector vive en el core y la
GUI sólo consume la cadena `content_type` que el backend ya entrega.

## Goals / Non-Goals

**Goals:**

- Crear `clipvault_core::content_type::detect_content_type`, una función
  pura, determinística y testeable sin Tauri, SQLite, clipboard, red ni
  logs, que clasifica un payload textual en una de las variantes
  documentadas del enum `ContentType`.
- Extender `clipvault_db::ContentType` con las variantes nuevas en
  snake_case estable, conservando `Text` para datos existentes y
  rechazando valores desconocidos con un error tipado al deserializar.
- Cablear el detector en `TextHistoryService::record_payload` para que
  toda entrada nueva persista el tipo detectado.
- Ampliar `EntryRepository::text_entries()` para que la búsqueda local
  incluya todos los tipos textuales soportados.
- Exponer `content_type` como etiqueta legible en historial, búsqueda y
  quick-paste, con fallback seguro a "Texto" para valores no
  reconocidos.
- Garantizar que el deduplicado por hash, los favoritos, el borrado, la
  retención y la sweep de blacklist sigan funcionando idénticamente
  para todas las variantes nuevas.

**Non-Goals:**

- No se modifica el formato binario de la base de datos ni se agregan
  migraciones: ampliar los valores de `content_type` no requiere cambio
  de esquema y las filas pre-existentes con `'text'` siguen siendo
  válidas.
- No se agrega filtrado por tipo en la UI; sólo etiquetas. El
  quick-paste y la búsqueda siguen mostrando todas las entradas
  textuales.
- No se introduce detección por red, embeddings, LLMs ni heurísticas
  probabilísticas. Todo es determinístico.
- No se detecta ni clasifica contenido binario (imágenes, HTML
  enriquecido, archivos, etc.); eso queda fuera del alcance y se
  reserva para v0.2/v0.3.
- No se reescribe el historial existente ni se reclasifican filas
  pre-existentes; las filas con `'text'` se quedan como `Text`.
- No se reimplementa el detector en TypeScript. La UI sólo traduce el
  `content_type` devuelto por el backend.

## Decisions

### 1. Detector puro en `clipvault-core::content_type`

- Nuevo módulo `crates/clipvault-core/src/content_type.rs` con:
  - `pub fn detect_content_type(input: &str) -> ContentType`
  - `pub fn is_textual(content_type: ContentType) -> bool`
  - tabla de tipos textuales (`pub const TEXTUAL_CONTENT_TYPES: &[ContentType]`).
- Sin parámetros de configuración, sin estado global: la función toma
  sólo el payload y devuelve el tipo detectado.
- No llama a Tauri, SQLite, el reloj, la clipboard, el sistema de
  archivos ni la red. No registra logs.
- Trimming explícito al inicio; entradas vacías o sólo whitespace
  caen al fallback `Text`.

### 2. Precedencia estable y conservadora

Orden de evaluación (el primer match gana):

1. **JWT**: tres segmentos base64url con prefijo `eyJ` y longitudes
   mínimas; se valida localmente antes que UUID porque un JWT válido
   nunca tiene el formato de un UUID.
2. **UUID**: ocho formatos completos (`xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`,
   `urn:uuid:…`, sin guiones, con `{…}` braces, `({…})` paréntesis y
   variantes mayúsculas / minúsculas). Validación con un parser
   determinístico hecho a mano (posiciones fijas de guiones y
   dígitos hex) para evitar aceptar cadenas que se parecen pero no
   son UUIDs. No se introduce la dependencia `uuid` por privacidad /
   mínima superficie.
3. **IPv6**: parsing estricto con `std::net::Ipv6Addr::from_str`. Se
   aceptan formas con y sin corchetes (sin corchetes cuando el valor
   no contiene espacios ni puerto).
4. **IPv4**: parsing estricto con `std::net::Ipv4Addr::from_str`.
5. **Email**: el valor completo (sin espacios ni saltos) cumple un
   patrón conservador `local@domain.tld` validado por un parser
   manual sin regex (caracteres permitidos + un único `@` + al menos
   un punto en el dominio). Si cualquier parte del valor contiene
   whitespace o más de un `@` se rechaza para evitar falsos positivos
   con texto natural.
6. **URL**: esquema de la lista permitida (`http`, `https`, `ftp`,
   `sftp`, `ssh`, `file`, `mailto`, `tel`) seguido de `://`, más un
   host no vacío y ninguna presencia de whitespace ni caracteres de
   control. El parser es manual (sin `regex`, sin `url::Url`) y sólo
   acepta cuando el valor completo es una URL; URLs embebidas en
   texto natural caen al fallback `Text`.
7. **Hex color**: `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA` con la
   longitud exacta; se valida que el resto sean dígitos hex.
8. **JSON**: `serde_json::from_str::<serde_json::Value>` sobre el valor
   completo; sólo se acepta cuando el resultado es un `Object` o un
   `Array` no vacío. Literales (`null`, `true`, `false`, números,
   strings sueltos) caen al fallback `Text`.
9. **HTML**: presencia de una etiqueta reconocible con balance mínimo
   (`<html>…</html>`, `<div>…</div>`, `<p>…</p>`, `<a …>`, `<script …>…</script>`,
   `<style …>…</style>`). Texto con un solo `<` o `>` no califica.
10. **SQL**: comienza con una keyword DML/DDL (`SELECT`, `INSERT`,
    `UPDATE`, `DELETE`, `CREATE`, `DROP`, `ALTER`, `WITH`) y la línea
    siguiente contiene `FROM`, `INTO`, `TABLE` u otra keyword
    estructural. Una sola keyword aislada no califica.
11. **Shell command**: shebang (`#!…`) o comando con piping/redirección
    fuertes (`|`, `&&`, `||`, `>`, `<`, `2>&1`) y al menos una
    keyword de shell (`cd`, `ls`, `grep`, `cat`, `echo`, `mkdir`, `rm`,
    `cp`, `mv`, `chmod`, `chown`, `awk`, `sed`, `curl`, `wget`, `git`,
    `npm`, `cargo`, `psql`, `ssh`, `docker`, `kubectl`, `make`,
    `yarn`, `pnpm`, `brew`, `apt`, `sudo`). Comandos naturales con
    esas palabras pero sin sintaxis de shell caen al fallback.
12. **Code**: bloque fenced (` ``` ` o `~~~`) o shebang
    (`#!/usr/bin/env …`, `#!/bin/…`) con un lenguaje identificable
    (`python`, `rust`, `js`, `ts`, `tsx`, `jsx`, `go`, `java`, `c`,
    `cpp`, `rb`, `php`, `sh`, `bash`, `zsh`, `sql`, `html`, `css`,
    `json`, `yaml`, `toml`, `md`, `rs`). Lenguajes sin shebang sólo
    califican si están envueltos en un bloque fenced.
13. **File path**: rutas macOS (`/Users/…`, `/Volumes/…`,
    `/private/…`, `~/…`) o Linux (`/home/…`, `/etc/…`, `/usr/…`,
    `/var/…`, `/tmp/…`, `~`, `./`, `../`) que NO son URLs y NO
    contienen caracteres de control. Una sola palabra suelta o un
    texto con espacios no califica.
14. **Text**: fallback conservador para todo lo demás, incluyendo
    entradas vacías, sólo whitespace, Unicode no clasificado,
    mezclas ambiguas y texto natural.

El orden es estable y se documenta en este archivo y en los tests del
detector. Cualquier empate se resuelve en favor de la categoría más
específica (las primeras en la lista).

**Alternativas consideradas:**

- Clasificar por voto mayoritario entre tipos: rechazado porque
  introduce no-determinismo y ambigüedad; una sola categoría estable
  es más predecible para el usuario.
- Aplicar el detector sólo a la primera línea: rechazado porque tipos
  como SQL, HTML, code y shell_command son frecuentemente multilínea.
- Detectar tokens (palabras) dentro del texto: rechazado porque rompe
  la regla "el valor completo es una URL" / "el valor completo es
  JSON"; las detecciones son del payload entero, no de sub-fragments.

### 3. Extensión del enum `ContentType`

- Nuevas variantes en `crates/clipvault-db/src/entry.rs`:
  `Url`, `Email`, `Json`, `Jwt`, `Uuid`, `Ipv4`, `Ipv6`, `HexColor`,
  `Html`, `FilePath`, `ShellCommand`, `Sql`, `Code`.
- `#[serde(rename_all = "snake_case")]` produce cadenas estables:
  `text`, `url`, `email`, `json`, `jwt`, `uuid`, `ipv4`, `ipv6`,
  `hex_color`, `html`, `file_path`, `shell_command`, `sql`, `code`.
- `as_str()` actualizado con todas las variantes; `Display`
  reutiliza `as_str()`.
- `ContentType::is_textual()` devuelve `true` para todas las variantes
  textuales (incluyendo `Text`). Lo usa `EntryRepository::text_entries`
  y cualquier futuro filtro.

**Alternativas consideradas:**

- Modelo de árbol con discriminadores (`text/plain`, `text/url`, …):
  rechazado porque el enum plano es más pequeño y cabe en una
  columna SQLite sin migraciones.
- Almacenar como JSON: rechazado porque rompe queries SQL simples y
  el contrato existente (`EntryRecord.content_type` como string).
- Re-nombrar `Text` a `PlainText`: rechazado porque rompe la fila
  `'text'` heredada y obligaría a una migración destructiva.

### 4. Persistencia backward-compatible

- Sin migración nueva. La columna `content_type TEXT NOT NULL` ya
  acepta cualquier valor en `snake_case`; el enum sólo describe los
  valores legales.
- `parse_content_type` (en `entry_repository.rs`) acepta todas las
  variantes y rechaza valores desconocidos devolviendo el error
  tipado `EntryRepositoryError::UnknownContentType { value }` para
  que el lector decida cómo manejarlo. El `row_to_record` actual
  falla la conversión ante un valor desconocido; ese contrato se
  mantiene.
- `EntryRepository::text_entries` filtra con
  `content_type IN ('text', 'url', …)` (lista de cadenas) en lugar de
  `content_type = 'text'`, de modo que las nuevas filas clasificadas
  siguen apareciendo en la búsqueda local.
- `entry_repository` gana una constante `TEXTUAL_CONTENT_TYPES` con la
  lista canónica, compartida con el `serde_json` de tests.
- El deduplicado por hash no se toca. Un duplicado actualiza
  `updated_at` / `last_seen_at` sin tocar `content_type` (la fila ya
  tiene su clasificación original).

**Alternativas consideradas:**

- Reclasificar filas pre-existentes con `'text'` a su tipo real: rechazado
  porque es destructivo, no determinístico para payloads que ya fueron
  modificados, y porque las reglas dicen "no rompas filas antiguas".
- Crear una tabla de migraciones explícita: rechazado porque no hay
  cambio de esquema.

### 5. Cableado en el pipeline de captura

- `TextHistoryService::record_payload` consulta el detector justo
  antes de construir el `NewEntry` y usa el resultado como
  `content_type`.
- Orden: empty check → privacy gate → blacklist discard → detección →
  persistencia. La detección nunca corre para payloads vacíos o
  blacklisteados.
- No se añade un nuevo campo al `NewEntry` ni al `EntryRecord`; la
  clasificación se aloja en el campo existente `content_type`.

**Alternativas consideradas:**

- Detector en la capa Tauri: rechazado porque rompe la regla
  arquitectónica "los comandos Tauri son adaptadores delgados".
- Detector en `clipvault-db`: rechazado porque el DB no debe contener
  lógica de clasificación; debe limitarse a leer / escribir filas.
- Detector lazy (al renderizar): rechazado porque perdemos la
  clasificación en el momento del alta y cualquier cambio futuro en el
  payload obligaría a re-procesar.

### 6. UI: badge metadata-only con fallback seguro

- Helper TypeScript nuevo `contentTypeLabel(value: string): string`
  en `app/tauri/frontend/src/lib/contentType.ts` con la tabla de
  etiquetas legibles (`"URL"`, `"Email"`, `"JSON"`, `"JWT"`, `"UUID"`,
  `"IPv4"`, `"IPv6"`, `"Hex color"`, `"HTML"`, `"Ruta"`, `"Shell"`,
  `"SQL"`, `"Código"`, `"Texto"` para cualquier valor desconocido o
  vacío).
- `App.svelte` y `QuickPaste.svelte` muestran el badge junto al
  snippet cuando el registro tiene un `content_type` distinto de
  `text`. Para `text` y para valores desconocidos el badge se omite o
  se renderiza como "Texto" según la decisión de diseño documentada
  en el spec.
- `app/tauri/frontend/src/types.ts` añade `ContentTypeLabel` como tipo
  unión opcional para representar el badge ya resuelto.
- No se envía contenido a servicios externos. No se reimplementa el
  detector en JS.
- `tests/contentType.test.ts` cubre: cada variante mapea a su
  etiqueta; valor desconocido cae a `"Texto"`; valor vacío cae a
  `"Texto"`; el helper es puro y determinista.

### 7. Privacidad

- El detector no loguea nada. La captura sigue sin pasar el payload a
  un `tracing::*!` macro (regla preservada desde `clipboard-text-history`
  y `privacy-settings`).
- La etiqueta UI es metadata-only; nunca incluye payload, hash ni
  snippet. La lista de etiquetas vive en el helper TypeScript, no se
  deriva del contenido.
- `entry_repository::text_entries` no devuelve payload a logs.
- El test `record_payload_does_not_log_payload_hash_or_source_app` se
  extiende para cubrir también la ausencia del `content_type`
  clasificado (no se loguea).

## Risks / Trade-offs

- **[Riesgo: falsos positivos en categorías como SQL / shell / code]**
  → **Mitigación:** precedencia conservadora; sólo se aceptan señales
  fuertes (shebang, sintaxis estructural, palabras clave estructurales).
  Texto natural con keywords sueltas cae a `Text`. Tests cubren
  regresiones específicas.

- **[Riesgo: clasificación incorrecta de payloads ambiguos]**
  → **Mitigación:** el fallback `Text` siempre gana para entradas
  ambiguas. Si dos categorías podrían aplicar, gana la más específica
  según el orden documentado. El usuario siempre puede pegar / borrar
  / favoritar independientemente del tipo.

- **[Riesgo: cambio de comportamiento para filas pre-existentes]**
  → **Mitigación:** no se ejecuta ninguna migración; filas con
  `content_type = 'text'` siguen leyéndose como `Text` y siguen
  apareciendo en la búsqueda.

- **[Riesgo: detector acoplado a dependencias externas]**
  → **Mitigación:** el detector usa sólo `std` y `serde_json`, ambos
  ya disponibles en el workspace. No se añaden `regex`, `url`, `uuid`
  ni ningún parser / validador externo. Si `serde_json` no estuviera
  disponible (caso improbable), el detector cae conservadoramente a
  `Text`.

- **[Riesgo: el frontend muestra etiquetas inconsistentes]**
  → **Mitigación:** la tabla de etiquetas vive en un único helper y
  los tests bloquean el contrato; los valores desconocidos caen a
  `"Texto"`.

## Migration Plan

1. Publicar `clipvault_core::content_type::detect_content_type` con
   cobertura por tipo y precedencia documentada.
2. Ampliar `clipvault_db::ContentType` con las nuevas variantes y
   ajustar `as_str` / `Display` / `parse_content_type`.
3. Sustituir el `WHERE content_type = 'text'` en
   `EntryRepository::text_entries` por la lista canónica de tipos
   textuales.
4. Cablear el detector en `TextHistoryService::record_payload` y
   actualizar los tests de integración para que asertan
   `content_type` específico.
5. Añadir helper `contentTypeLabel` y mostrarlo en `App.svelte` /
   `QuickPaste.svelte` con tests.
6. Rollback: revertir el `WHERE` a `content_type = 'text'` (sólo
   afecta la búsqueda), quitar el cableado en `record_payload` y
   eliminar el helper UI. La fila legacy no se ve afectada porque no
   se introdujo migración.

## Open Questions

- ¿Vale la pena soportar `mailto:` URLs como tipo separado (no como
  `Email`)? Decidimos que NO en esta iteración: las URLs `mailto:`
  se clasifican como `Url` por la regla general y la UI puede mostrar
  la etiqueta `"URL"` o `"Email"` según convenga. Si hace falta un
  refinamiento, se introduce como spec delta sin romper el contrato.
- ¿Conviene registrar el `content_type` detectado en un log de
  auditoría local? Decidimos que NO: la clasificación es metadata-only
  y nunca debe filtrar contenido; si el usuario quiere auditar, lo
  hace desde el historial.
