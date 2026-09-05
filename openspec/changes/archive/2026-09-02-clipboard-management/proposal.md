## Why

El MVP ya captura texto del portapapeles y lo persiste en SQLite, pero todavía
no existe una forma de gestionar ese historial desde la aplicación. Sin
operaciones de gestión, el usuario no puede marcar favoritos, borrar entradas
que no le interesan ni aplicar la política de retención documentada en
`project.md`. Esto bloquea los flujos cotidianos de organización, limpieza y
privacidad que sustentan la propuesta de valor de ClipVault.

Este cambio implementa la capacidad `clipboard-management` definida en
`openspec/specs/clipboard-management/spec.md` y la conecta con la GUI y, a
futuro, con la CLI compartida.

## What Changes

- Agregar un servicio de gestión de historial en `clipvault-core` que cubra
  favoritos, borrado individual, borrado masivo no favorito y aplicación de la
  política de retención local (7, 30, 90 días o permanente).
- Exponer las operaciones de gestión como comandos Tauri delgados en
  `app/tauri/src-tauri` que delegan en el core, sin duplicar lógica ni acceder
  directamente a SQLite desde el frontend.
- Añadir endpoints SQL transaccionales en `clipvault-db` para activar/desactivar
  favoritos, borrar entradas (idempotente), vaciar el historial no favorito y
  purgar entradas vencidas.
- Leer la política de retención desde la configuración local
  (`privacy-settings`) y aplicarla al iniciar y al cierre de la aplicación, así
  como después de operaciones que cambien el estado de las entradas.
- Mantener el orden de favoritos por encima del historial normal cuando el
  resto de factores de ranking sean comparables, según lo documentado en la
  especificación.
- Agregar UI accesible en `app/tauri/frontend` para mostrar el estado de
  favorito, alternarlo, borrar entradas y vaciar el historial, con confirmación
  explícita para las acciones destructivas.
- Añadir pruebas unitarias y de integración que cubran favoritos, borrado
  idempotente, vaciado preservando favoritos, retención configurable y la
  invariante de que los favoritos nunca expiran automáticamente.

## Capabilities

### New Capabilities

- (ninguna)

### Modified Capabilities

- `clipboard-management`: se añaden requisitos para el contrato de comandos
  Tauri, la confirmación de acciones destructivas, la idempotencia del
  alternado de favoritos, la lectura de la política de retención desde la
  configuración local, la atomicidad de las mutaciones en SQLite y la
  prohibición de registrar o emitir contenido del portapapeles durante
  cualquier operación de gestión.

## Impact

- `crates/clipvault-core`: nuevo servicio `HistoryManagementService` o
  equivalente, con operaciones `set_favorite`, `delete_entry`, `clear_history`
  y `apply_retention`. No debe depender de Tauri ni de la GUI.
- `crates/clipvault-db`: nuevas queries SQL con transacciones explícitas e
  índices necesarios para favoritos y `created_at`. Migración aditiva si la
  tabla actual no soporta la columna `is_pinned` o el índice de retención.
- `crates/clipvault-platform`: sin cambios directos; las mutaciones se hacen
  en SQLite local y la plataforma solo aporta contexto de aplicación cuando
  esté disponible.
- `app/tauri/src-tauri`: comandos delgados como `clipvault_set_favorite`,
  `clipvault_delete_entry`, `clipvault_clear_history` y
  `clipvault_apply_retention`. Reutilizar el `AppState` y la configuración
  existentes; no añadir cientos de líneas de lógica.
- `app/tauri/frontend`: componentes Svelte para acciones de gestión,
  confirmación accesible (modal o inline) y mensajes neutros que no expongan
  el contenido del portapapeles.
- Configuración: la política de retención se lee desde la capacidad
  `privacy-settings`; este cambio no introduce un nuevo sistema de
  preferencias.
- Sin red, telemetría, cuentas ni dependencias nuevas. Las migraciones deben
  ser reversibles y cubiertas por tests, según las reglas de `AGENTS.md`.
