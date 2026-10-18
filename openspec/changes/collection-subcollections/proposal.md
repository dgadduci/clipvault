# Proposal: collection-subcollections

## Why

Las colecciones actuales son planas. Cuando el usuario necesita separar un
conjunto dentro de una colección principal, debe crear otra colección al mismo
nivel y mantener esa relación fuera de ClipVault. Además, eliminar una
subcolección requiere decidir de forma explícita si sus capturas permanecen en
la colección principal o se eliminan de todo el historial.

## What Changes

- Permitir subcolecciones con una única colección principal y un solo nivel de
  profundidad.
- Mostrar las colecciones como una lista jerárquica con controles accesibles
  para expandir, contraer y crear subcolecciones.
- Mantener para las subcolecciones las operaciones, colores, notas, conteos,
  membresías y comportamiento de captura de las colecciones existentes.
- Permitir subcolecciones solo bajo colecciones principales locales no
  vinculadas a importaciones remotas, para conservar la regla actual de no
  asignar capturas locales a colecciones importadas.
- Al borrar una subcolección, ofrecer conservar sus capturas en la colección
  principal y demás ubicaciones existentes, o eliminarlas globalmente,
  incluyendo `Historial` y todas las colecciones.
- Al borrar una colección principal, borrar también sus subcolecciones y usar
  el flujo actual de conservación en `Historial` o eliminación global para las
  capturas asociadas al árbol completo.
- Migrar las colecciones existentes como principales y conservar las
  capturas, membresías, notas y referencias de assets.

## Capabilities

### New Capabilities

### Modified Capabilities

- `tags-and-collections`: jerarquía de un nivel, CRUD de subcolecciones, árbol
  en el sidebar y eliminación con alcance explícito.

## Impact

- Añade una migración SQLite con `parent_id` nullable; las filas preexistentes
  permanecen principales sin reescribir sus capturas ni assets.
- Extiende el repositorio y servicio de organización Rust, sus comandos Tauri,
  el snapshot de colecciones y el sidebar Svelte.
- Añade textos accesibles/localizados a `en`, `es`, `pt`, `de` y `fr`.
- Conserva el drag and drop existente, permitiendo que una subcolección sea un
  destino de captura independiente.
- No agrega dependencias, red, sincronización ni cambios al formato de assets.
