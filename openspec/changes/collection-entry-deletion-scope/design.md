# Design: collection-entry-deletion-scope

## Context

La eliminación global de una entrada ya pasa por `HistoryManagementService`,
que confirma la operación, limpia assets no referenciados e invalida la
deduplicación del watcher. `OrganizationRepository::delete_collection`
actualmente sólo elimina la colección y sus asociaciones. La importación de
imágenes inserta filas directamente dentro de su propia transacción y añade la
asociación a la colección del par; ese camino debe cumplir también el
invariante de `Historial` usado por las capturas locales.

## Goals / Non-Goals

**Goals:**

- Restablecer `Historial` como asociación obligatoria también para todas las
  importaciones actuales y existentes.
- Reutilizar los servicios y comandos de gestión local para confirmar, borrar,
  refrescar y limpiar assets.
- Hacer atómica la elección de borrar una colección con o sin sus entradas.
- Usar el scope activo de la card para diferenciar quitar una membresía de
  borrar la entrada globalmente.

**Non-Goals:**

- Cambiar la confianza del par, el transporte, la recepción remota o el
  comportamiento de futuras importaciones después de borrar entradas.
- Cambiar la política de limpieza global: sigue excluyendo favoritos y
  entradas con membresías de colecciones de usuario.
- Alterar el controlador, el payload o los listeners de drag-and-drop.
- Agregar tablas, dependencias, servicios externos o contenido de clipboard a
  comandos de confirmación.

## Decisions

### Reafirmar la membresía en Historial para importaciones

La transacción de importación debe insertar o asegurar la membresía de
`Historial` tanto para una entrada nueva como para una entrada local reutilizada
por deduplicación. Una reparación de datos idempotente debe añadir esa
membresía a entradas existentes identificadas por `remote_imports`, usando el
registro de colección `Historial` ya creado por las migraciones. La reparación
no cambia el contenido, título, favorito, procedencia ni otras membresías.

Esto corrige la causa que deja importaciones sin membresía de historial y no
requiere modificar el predicado de `clear_unorganized_history`: una vez
restablecido el invariante, las entradas no favoritas cuyo único grupo restante
es `Historial` cumplen la regla existente.

### Preview de eliminación como metadatos

La UI obtiene del backend un preview con el total de entradas y cuántas son
favoritas. El preview no incluye IDs, contenido, hashes, procedencia ni assets.
Al confirmar el borrado de las entradas, el comando compara esos conteos con
los actuales dentro de la transacción. Si cambiaron, no modifica la base y
devuelve el preview actualizado para que el usuario vuelva a confirmar. Si
coinciden, elimina todas las entradas asociadas y la colección; la respuesta
informa las cantidades efectivamente borradas.

### Borrado global coordinado por gestión de historial

La ruta Tauri de borrar colección delega en un único servicio Rust que exige
confirmación explícita. Para conservar, elimina sólo la colección y sus
membresías, preservando las filas y procedencia. Para borrar las capturas,
una sola transacción elimina todas las entradas actualmente asociadas y la
colección. Las claves foráneas eliminan sus membresías, tags de entrada,
procedencia local y binding de colección; no se elimina el par conocido ni su
confianza.

Después del commit, el servicio existente de gestión de historial recolecta
assets sólo si ninguna entrada los referencia e invalida el estado del watcher
sólo cuando se borró al menos una entrada. Un fallo antes del commit no aplica
ninguna parte de la operación.

### Scope de borrado de cards

La card continúa invocando un callback delgado. `App.svelte` combina la acción
con `selectedCollectionId`: desde `Historial` conserva el diálogo de borrado
global actual; desde una colección de usuario abre una elección. Mantener la
entrada llama al comando existente de quitar de la colección activa. Borrarla
globalmente reutiliza el comando confirmado de eliminar la entrada. La opción
existente `Quitar de esta colección` se conserva.

Los modales siguen siendo accesibles y muestran sólo nombre seguro de
colección, conteos y acciones. No se agrega estado de borrado al controlador de
drag-and-drop ni se cambia ningún contrato de card.

## Risks / Trade-offs

- **El preview puede quedar desactualizado si entra o se reorganiza una
  captura antes de confirmar** → la transacción aborta la eliminación cuando
  los conteos cambiaron y exige que el usuario confirme el preview actualizado.
- **Una captura se elimina también de otras colecciones al elegir el borrado
  global** → el prompt nombra explícitamente `Historial` y todas las
  colecciones, y la alternativa de conservarla quita sólo la membresía activa.
- **Una importación reutiliza un asset compartido** → el colector existente
  consulta referencias vivas antes de borrar el archivo.
- **Rollback de reparación de datos** → la reparación sólo agrega membresías
  requeridas y es idempotente. Su rollback no elimina esas asociaciones para
  evitar borrar estado válido creado después de la actualización.

## Migration Plan

Agregar una migración de datos posterior a la creación de `remote_imports` que
inserte sólo las asociaciones `Historial` que falten para sus entradas locales.
La migración es aditiva; entradas importadas nuevas y deduplicadas también
aseguran la misma membresía en su transacción de importación. No se modifican
archivos bajo `~/.clipvault` directamente fuera de la base de datos ni se
limpian assets durante la actualización.
