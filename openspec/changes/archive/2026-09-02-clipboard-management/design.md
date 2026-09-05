# Design: clipboard-management

## Context

`clipboard-text-history` ya define la captura, persistencia y dedupe de
entradas de texto, y `clipboard-search` define la búsqueda. Falta la capa
de gestión: el usuario todavía no puede marcar favoritos, borrar entradas
apuntadas ni aplicar la política de retención documentada en `project.md`.

La especificación `clipboard-management` cubre los requisitos de producto
(favoritos, borrado, retención) y este cambio la implementa respetando las
reglas arquitectónicas de `AGENTS.md`: la lógica vive en
`clipvault-core` y `clipvault-db`, Tauri queda como adaptador delgado y el
frontend nunca toca SQLite.

## Goals / Non-Goals

**Goals:**

- Implementar favoritos, borrado individual, vaciado no favorito y retención
  configurables contra SQLite local con transacciones explícitas.
- Exponer operaciones a través de comandos Tauri delgados, con respuestas
  serializables que no incluyan contenido del portapapeles.
- Aplicar la política de retención al iniciar, al cerrar y bajo demanda
  (`clipvault_apply_retention`) leyendo la preferencia local definida por
  `privacy-settings`.
- Mantener los favoritos por encima del historial normal cuando los demás
  factores de ranking sean comparables, sin duplicar reglas de búsqueda
  (la lógica de ranking sigue viviendo en `clipvault-search`).
- Cubrir el comportamiento con tests unitarios del core y tests de
  integración de DB que no requieren clipboard real ni GUI.

**Non-Goals:**

- No se introduce una lista negra de aplicaciones; eso es una capacidad
  aparte (`clipboard-text-history` o un cambio futuro) que afecta la captura,
  no la gestión.
- No se introducen snippets, reglas automáticas, transformaciones,
  import/export, detección de secretos ni CLI; esas son capacidades
  v0.2/v0.3 que se abordarán en cambios posteriores con su propio
  OpenSpec.
- No se añade red, telemetría ni dependencias nuevas.
- No se modifica el contrato de `clipboard-search` salvo para que los
  resultados reflejen los cambios de favorito y borrado en la siguiente
  consulta; el ranking se mantiene donde está.
- No se rediseña el shell ni se cambian atajos globales.

## Decisions

### Capa de servicio en `clipvault-core`

Se agrega un módulo `history_management` (o equivalente) que expone un
servicio con métodos tipados:

- `set_favorite(id, pinned) -> HistoryEntrySummary`
- `delete_entry(id) -> DeleteOutcome` (con `confirmation_required` cuando
  no venga `confirm: true`)
- `clear_non_favorites(confirm) -> ClearOutcome`
- `apply_retention() -> RetentionOutcome` (lee la política del
  `SettingsProvider` o equivalente ya existente en el core)

El servicio recibe un `HistoryRepository` (trait en `clipvault-core` con
implementación en `clipvault-db`) y un `SettingsReader` para la política
de retención. Esta indirección permite mockear ambos en tests.

**Alternativas consideradas:**

- Poner la lógica en `clipvault-db`: se descartó porque las reglas de
  negocio (confirmación, lectura de settings, aplicación de retención al
  cierre) no deben vivir en la capa de persistencia.
- Poner la lógica en Tauri: se descartó porque rompe la separación
  core/adaptador y duplica lógica cuando llegue la CLI compartida.

### Comandos Tauri como adaptadores delgados

Cada operación se expone como un comando corto en
`app/tauri/src-tauri/commands/` que:

1. Parsea y valida los argumentos (id numérico, `confirm: bool`).
2. Llama al servicio del core.
3. Devuelve un DTO serializable.

Los nombres objetivo son `clipvault_set_favorite`,
`clipvault_delete_entry`, `clipvault_clear_history` y
`clipvault_apply_retention`. Ninguno acepta el contenido de la entrada ni
devuelve contenido; solo metadatos seguros.

**Alternativas consideradas:**

- Reutilizar `clipvault_update_entry` genérico: se descartó porque mezcla
  semánticas y dificulta imponer la confirmación obligatoria para
  acciones destructivas.
- Exponer la API como REST local: fuera de alcance y no alineado con la
  regla "sin red" del proyecto.

### Persistencia transaccional en `clipvault-db`

`clipvault-db` ya realiza migraciones explícitas. Las operaciones nuevas
se ejecutan en una sola transacción SQLite (`BEGIN ... COMMIT`/`ROLLBACK`)
para garantizar la invariante "todo o nada". Si la tabla de historial
todavía no tiene `is_pinned` o el índice por `created_at`, se agrega una
migración aditiva (nunca destructiva), con su test de migración
reversible.

**Alternativas consideradas:**

- Una migración por operación: se descartó por overhead; una migración
  aditiva con todas las columnas/índices nuevos es suficiente.
- Usar `rusqlite` con `unchecked_transaction` para mejorar rendimiento: se
  rechazó para mantener la legibilidad y la detección temprana de errores.

### Lectura de la política de retención

La política vive en la configuración local gestionada por
`privacy-settings`. El servicio `history_management` recibe un
`SettingsReader` (trait) que devuelve la política efectiva. Si la clave
no está presente, se aplica el default documentado (30 días) y se persiste
localmente para futuras ejecuciones.

**Alternativas consideradas:**

- Hardcodear el default en el servicio: se descartó para que un futuro
  cambio de default no requiera tocar la capa de gestión.
- Consultar el sistema de configuración desde Tauri y pasarlo al comando:
  se descartó porque rompe la simetría core/CLI y obliga a los tests a
  montar el shell.

### UI y confirmación

El frontend muestra el estado de favorito en cada fila de la lista,
ofrece alternar el pin, borrar la entrada y vaciar el historial no
favorito. Las acciones destructivas pasan por un diálogo de confirmación
accesible (modal o inline) que envía `confirm: true` al comando. La UI
no añade un nuevo sistema de modales si ya existe uno reutilizable.

**Alternativas consideradas:**

- Confirmación inline sin modal: se descartó por accesibilidad y por
  consistencia con otros diálogos existentes.
- Modal con texto del clipboard para "verificar": rechazado por
  privacidad; el diálogo solo muestra conteos y metadatos.

## Risks / Trade-offs

- **Carrera entre captura y purga de retención** → Mitigation: la
  retención se aplica en transacción y nunca toca favoritos; la captura
  usa inserciones idempotentes, por lo que una purga concurrente solo
  podría afectar a una fila que esté siendo insertada con el mismo hash
  (dedupe ya la maneja).
- **Pérdida accidental de historial al cambiar la retención** → Mitigation:
  el cambio de política se documenta y el frontend pide confirmación
  explícita cuando se baja la retención por debajo de un umbral o cuando
  se pasa de `forever` a un valor finito.
- **Reentradas en `apply_retention`** → Mitigation: la función toma un
  `Mutex` o un `try_lock` para evitar dos aplicaciones simultáneas; las
  pruebas cubren el caso de doble invocación.
- **Tamaño de la respuesta al vaciar** → Mitigation: la respuesta solo
  incluye el conteo de filas eliminadas, nunca el contenido.
- **Cambio de esquema durante la implementación** → Mitigation: la
  migración es aditiva, reversible y se prueba de forma explícita; si el
  cambio rompe una base existente, se pausa y se actualiza el
  OpenSpec antes de continuar.

## Migration Plan

1. Agregar la migración aditiva en `clipvault-db` (columna `is_pinned` si
   falta, índice por `created_at`).
2. Implementar el servicio en `clipvault-core` con traits mockeables.
3. Exponer los comandos Tauri delgados.
4. Conectar la UI con los comandos, reutilizando el sistema de
   confirmación accesible existente.
5. Aplicar la retención al iniciar y al cerrar la app desde el bootstrap
   ya existente, sin bloquear la primera carga útil.
6. Ejecutar la suite de tests relevante (`cargo test --workspace`) y
   `openspec validate clipboard-management --strict`.

## Open Questions

- ¿La confirmación de "vaciar historial no favorito" debe ser una modal
  dedicada o basta con un `confirm()` nativo del frontend? Decisión a
  tomar durante la implementación, manteniendo la accesibilidad.
- ¿La purga de retención debe ejecutarse también después de cada borrado
  individual o solo en los puntos ya documentados (inicio, cierre,
  manual)? Por defecto se mantiene solo en esos tres puntos para evitar
  sorpresas; se puede ajustar en una iteración posterior sin romper el
  contrato.
- ¿Se necesita un endpoint de "contar cuántas entradas se borrarían con
  la retención actual" para mostrarlo en la configuración? Se considera
  nice-to-have; este cambio no lo introduce para mantener el alcance.
