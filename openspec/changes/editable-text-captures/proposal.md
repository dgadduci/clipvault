# Propuesta: edición persistente de capturas de texto

## Problema

Una captura textual queda inmutable después de entrar al historial; sólo se
puede editar su título. Para corregir un typo, quitar información accidental o
adaptar una captura reutilizable el usuario debe crear otra captura y borrar la
original, perdiendo su identidad, organización y estado.

## Objetivo

Permitir que el usuario edite el contenido de una captura de texto desde su
card y que el cambio quede persistido en SQLite, sobreviva al reinicio y se
refleje en historial, búsqueda y Quick Paste.

## Decisión aprobada

Usar un `<textarea>` HTML nativo dentro de un modal existente o compatible con
el shell de modales de ClipVault. No agregar CodeMirror, Monaco, Tiptap ni otra
dependencia de editor en esta primera versión. El alcance es texto plano; si
en el futuro se requieren highlighting, múltiples cursores o texto enriquecido,
se abrirá otro cambio OpenSpec.

## Alcance

- Agregar una acción `Editar captura` al menú de las cards elegibles.
- Editar sólo entradas textuales sin assets de imagen ni metadata de rich text.
- Mostrar un modal accesible con el contenido actual, `textarea`, Guardar,
  Cancelar, Escape y atajos de teclado documentados.
- Persistir el texto editado mediante el core Rust y una transacción SQLite.
- Recalcular `content_size`, `content_hash`, `content_type` y `updated_at`
  usando las mismas reglas determinísticas de captura.
- Conservar el mismo `entry_id`, `created_at`, `last_seen_at`, título,
  favoritos, tags, colecciones, source-app metadata y referencias de assets.
- Rechazar contenido vacío y conflictos con otra entrada que ya tenga el mismo
  hash, sin fusionar ni borrar entradas automáticamente.
- Actualizar la card, el historial, la búsqueda local y Quick Paste sólo después
  de un guardado exitoso.

## Fuera de alcance

- Edición de imágenes o del payload de rich text/HTML/RTF.
- Historial de versiones, undo persistente, colaboración o sincronización.
- Autosave por cada tecla, nube, red o almacenamiento en `localStorage`.
- Cambios en drag and drop, tamaño de cards, captura del portapapeles o dedupe
  de nuevas capturas.

## Criterio de aceptación

Una captura textual elegible puede abrirse desde su menú, editarse y guardarse.
El mismo ID conserva tags, colecciones, favoritos, título, source-app metadata
y assets; hash, tamaño, tipo y `updated_at` representan el nuevo texto. El
cambio aparece después de reiniciar, buscar y usar Quick Paste. Un texto vacío,
una captura no editable o un hash duplicado no modifican la base y muestran un
error seguro.
