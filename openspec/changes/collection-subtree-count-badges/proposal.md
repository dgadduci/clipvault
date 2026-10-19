# Proposal: collection-subtree-count-badges

## Why

El badge de una colección principal cuenta hoy solo las membresías directas.
Por eso no coincide con las capturas visibles al considerar sus
subcolecciones. El total se calcula desde SQLite y no se persiste, de modo que
la causa no es un contador guardado que haya quedado desactualizado: falta
incluir las membresías de las subcolecciones en la proyección de la colección
principal.

## What Changes

- Cambiar el total del badge de una colección principal para sumar sus
  capturas directas que no estén en ninguna subcolección más los conteos de
  cada subcolección directa.
- Mantener en cada subcolección el conteo de sus propias membresías.
- Asegurar que altas, retiros, vaciados y eliminaciones se reflejen al cargar
  el snapshot actualizado de colecciones, sin mantener un contador separado.
- Si una captura está en varias subcolecciones, contarla una vez en el badge
  de cada subcolección y sumar esas contribuciones en el badge principal.

## Capabilities

### Modified Capabilities

- `tags-and-collections`: definir el conteo de badges para colecciones
  principales y subcolecciones.

## Impact

- Actualiza la agregación metadata-only de `OrganizationRepository` usada por
  la lista y la consulta individual de colecciones.
- No requiere migración, estado adicional, cambios en los eventos existentes
  ni dependencias nuevas. Las membresías actuales son la fuente del conteo y
  ya se refrescan tras las operaciones de organización e historial.
