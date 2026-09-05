## 1. Persistencia y migraciones

- [x] 1.1 Confirmar el esquema actual de la tabla de historial en
  `clipvault-db` y documentar si ya tiene `is_pinned` y un índice por
  `created_at`.
- [x] 1.2 Si falta, agregar una migración aditiva que añada la columna
  `is_pinned` (boolean, default false) y el índice por `created_at` sin
  tocar filas existentes.
- [x] 1.3 Escribir un test de migración que cree una base con el esquema
  anterior, aplique la nueva migración y verifique la invariante
  `is_pinned = false` y la presencia del índice.

## 2. Capa de dominio en `clipvault-core`

- [x] 2.1 Definir el trait `HistoryRepository` (o equivalente) en
  `clipvault-core` con las operaciones nuevas:
  `set_favorite`, `delete_entry`, `clear_non_favorites`,
  `delete_older_than`.
- [x] 2.2 Definir el trait `SettingsReader` (o reutilizar el existente)
  con un método para leer la política de retención efectiva.
- [x] 2.3 Implementar el servicio `HistoryManagementService` con métodos
  que devuelvan DTOs sin contenido: `SetFavoriteOutcome`,
  `DeleteOutcome`, `ClearOutcome`, `RetentionOutcome`.
- [x] 2.4 Hacer que el servicio lea la política de retención a través del
  `SettingsReader`, aplique el default documentado (30 días) cuando
  falte y persista la resolución local sin prompts.
- [x] 2.5 Implementar el `Mutex`/`try_lock` de reentrada para
  `apply_retention` y devolver un resultado idempotente.

## 3. Adaptador en `clipvault-db`

- [x] 3.1 Implementar el `HistoryRepository` en `clipvault-db` usando
  `rusqlite` y envolviendo cada mutación en una transacción explícita.
- [x] 3.2 Asegurar que el rollback deja la base idéntica al estado previo
  (test con un fallo forzado a mitad de mutación).
- [x] 3.3 Cubrir con tests de integración: marcar favorito, alternar
  favorito idempotente, borrar una entrada, borrar una entrada
  inexistente, vaciar no favoritos, purga de retención con favoritos
  intactos y `forever` que no elimina nada.

## 4. Comandos Tauri delgados

- [x] 4.1 Exponer `clipvault_set_favorite` que valida argumentos y
  delega en el servicio.
- [x] 4.2 Exponer `clipvault_delete_entry` que requiere `confirm: true`
  y delega en el servicio.
- [x] 4.3 Exponer `clipvault_clear_history` que requiere `confirm: true`
  y devuelve el conteo de filas removidas.
- [x] 4.4 Exponer `clipvault_apply_retention` que delega en el servicio
  sin requerir confirmación.
- [x] 4.5 Registrar los comandos en el `AppState`/`invoke_handler` y
  verificar que no se añade lógica de negocio dentro de los handlers.
- [x] 4.6 Añadir un test del shell (o un test de integración del crate
  Tauri) que confirme que las respuestas no contienen el contenido de
  las entradas.

## 5. Bootstrap y ciclo de vida

- [x] 5.1 Invocar `apply_retention` al iniciar la aplicación, sin
  bloquear la primera carga útil de la UI.
- [x] 5.2 Invocar `apply_retention` al cerrar la aplicación como parte
  del shutdown actual, con manejo limpio del resultado.
- [x] 5.3 Registrar un evento local de diagnóstico (sin contenido) cuando
  la purga elimina entradas.

## 6. Frontend (Svelte + TypeScript)

- [x] 6.1 Mostrar el estado de favorito en cada fila de la lista de
  historial y conectar el toggle al comando
  `clipvault_set_favorite`.
- [x] 6.2 Agregar el botón de borrado por entrada con confirmación
  accesible y envío de `confirm: true` al comando.
- [x] 6.3 Agregar el botón de vaciar historial no favorito con
  confirmación accesible y mensaje que muestre el conteo a eliminar
  (consultado al backend) sin exponer contenido.
- [x] 6.4 Reutilizar el sistema de modales o inline existente; no crear
  uno paralelo salvo que el actual no soporte la accesibilidad
  requerida.
- [x] 6.5 Manejar el resultado `confirmation_required` mostrando el
  diálogo en lugar de invocar la mutación.
- [x] 6.6 No añadir tipos nuevos de payload del clipboard a los DTOs
  serializados.

## 7. Tests

- [x] 7.1 Tests unitarios del servicio con repositorio y settings
  mockeados (sin SQLite real).
- [x] 7.2 Tests de integración de `clipvault-db` cubriendo transacciones
  y rollback.
- [x] 7.3 Tests de los comandos Tauri (argumentos inválidos, falta de
  confirmación, respuesta sin contenido).
- [x] 7.4 Tests de UI/TypeScript para el flujo de favorito, borrado y
  vaciado con confirmación.
- [x] 7.5 Test de migración reversible.
- [x] 7.6 Test que verifica que ninguna ruta emite contenido del
  clipboard (assert sobre el shape de la respuesta y sobre los logs).

## 8. Verificación

- [x] 8.1 Ejecutar `cargo fmt --all -- --check`.
- [x] 8.2 Ejecutar `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 8.3 Ejecutar `cargo test --workspace`.
- [x] 8.4 Ejecutar `npm run check` y `npm run build` en
  `app/tauri/frontend`.
- [x] 8.5 Ejecutar `openspec validate clipboard-management --strict`.
- [x] 8.6 Revisar el diff y confirmar que no se añadieron secretos,
  archivos generados innecesarios ni dependencias nuevas.
