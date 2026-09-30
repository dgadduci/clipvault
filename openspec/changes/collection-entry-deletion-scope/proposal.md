# Proposal: collection-entry-deletion-scope

## Why

Las importaciones de imágenes pueden quedar fuera de `Historial` porque su
transacción sólo agrega la colección vinculada al equipo. Al borrar esa
colección, las capturas quedan sin la asociación de historial que requiere la
limpieza de capturas no organizadas. Además, los flujos actuales no permiten
elegir explícitamente si una eliminación desde una colección también debe
borrar sus capturas del historial.

## What Changes

- Garantizar que toda captura importada, nueva o deduplicada, pertenezca a
  `Historial` y a la colección del equipo de origen.
- Incluir en la limpieza de capturas no organizadas las capturas importadas no
  favoritas que sólo permanecen en `Historial` después de borrar su colección
  de origen.
- Al borrar una colección, permitir conservar sus capturas en `Historial` o
  eliminar explícitamente del historial todas las capturas que estaban en ella.
  La confirmación mostrará cantidades totales y favoritas; el sistema
  `Historial` seguirá protegido.
- Al usar la acción `Eliminar` en una card dentro de una colección de usuario,
  permitir quitarla sólo de esa colección conservándola en `Historial` y en
  otras colecciones, o eliminarla globalmente del historial. La acción desde
  `Historial` mantiene su confirmación global actual.
- Mantener las operaciones de borrado transaccionales y limpiar assets sólo
  cuando ninguna captura restante los referencie.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `clipboard-management`: limpieza de importaciones huérfanas de colección y
  elección del alcance al eliminar una captura desde una colección.
- `clipboard-history-cards`: acción de eliminación con alcance explícito en
  colecciones de usuario.
- `peer-import-collection-visibility`: membresía obligatoria en `Historial` y
  opciones de conservación o borrado al eliminar una colección vinculada.
- `tags-and-collections`: elección explícita sobre el historial al borrar una
  colección.

## Impact

Cambios en los repositorios SQLite y servicios Rust de importación e historial,
los comandos Tauri y los flujos de confirmación en `App.svelte`,
`OrganizationSidebar.svelte` y `HistoryCard.svelte`. No se requieren
migraciones ni dependencias nuevas. La eliminación de una importación afecta
sólo la copia local y su procedencia local; no modifica el par remoto ni la
relación de confianza.
