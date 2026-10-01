# Propuesta: documentos de texto y notas para capturas y colecciones

## Why

Las colecciones sólo pueden organizar capturas que ya provienen del
portapapeles o de una importación. No hay una forma de crear desde ClipVault un
documento de texto multilinea dentro de una colección. Tampoco se pueden
adjuntar anotaciones persistentes a una colección o a una captura.

Las capturas existentes ya cuentan con persistencia, búsqueda, clasificación,
historial, cards y edición textual. La solución debe integrarse con esas
entidades sin convertir las notas en contenido del portapapeles ni alterar los
datos de una captura.

## What Changes

- Crear y editar documentos de texto plano con saltos de línea desde
  Historial o cualquier colección local que no esté vinculada a importaciones
  de un par.
- Guardar esos documentos como entradas textuales normales, respetando la
  pertenencia obligatoria a Historial y asociándolos también a la colección
  local activa cuando corresponda.
- Permitir una nota multilinea independiente por captura y por colección.
- Mostrar un icono de nota en las cards y en la lista de colecciones cuando
  exista una nota; activar el icono abre el modal para leer y editarla.
- Usar un mismo editor de notas para notas de captura y de colección.

## Alcance

- La creación manual está disponible en Historial y en colecciones de usuario
  locales. No está disponible en colecciones vinculadas a importaciones de
  pares.
- El editor de documento permite crear y editar contenido multilinea. La
  captura creada sigue el flujo de persistencia, clasificación, hash,
  deduplicación, búsqueda y mantenimiento ya establecido para texto.
- Cada captura de texto, texto enriquecido o imagen puede tener una nota local.
- Cada colección, incluida Historial y las colecciones vinculadas a pares,
  puede tener una nota local.
- La primera nota puede crearse desde la acción de la card o de la fila de
  colección. Una vez guardada, el icono de nota sirve también para abrir su
  editor.
- Las notas se almacenan localmente y no se incorporan al contenido de la
  captura, al portapapeles, a la búsqueda ni a las transferencias entre pares.
- La edición de notas actualiza únicamente la nota; no modifica el contenido,
  el hash, los timestamps, las asociaciones ni los assets de la captura o de
  la colección.

## Fuera de alcance

- Notas con formato enriquecido, archivos adjuntos, recordatorios o
  sincronización remota.
- Notas anidadas dentro de otras notas.
- Crear texto manual dentro de una colección importada/vinculada a un par.
- Cambiar las reglas existentes de deduplicación, clasificación, búsqueda,
  captura del portapapeles, expiración o eliminación de capturas.
- Compartir notas con otros equipos o añadir su contenido a eventos,
  diagnósticos, logs o payloads de drag-and-drop.

## Criterios de aceptación

- Se puede crear texto multilinea en Historial y en una colección local; el
  texto persiste al reiniciar y se muestra como captura textual con el icono
  determinado por la clasificación existente.
- Una colección vinculada a importaciones no permite crear documentos
  manuales; sus notas locales y las notas de sus capturas sí se pueden editar.
- Se puede agregar, reabrir, editar, borrar y persistir una nota de captura o
  colección desde un modal con textarea multilinea.
- El icono de una captura con nota abre esa nota, y el icono de una colección
  con nota hace lo mismo desde la lista de colecciones.
- Guardar o quitar una nota no altera el contenido de la captura ni los datos
  de organización relacionados.
