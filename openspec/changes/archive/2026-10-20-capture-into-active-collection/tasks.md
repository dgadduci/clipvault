# Tasks: capture-into-active-collection

## 1. Core y persistencia

- [x] 1.1 Agregar una operación aditiva y transaccional para asociar una
  entrada existente a una colección local válida, reutilizando la validación
  de destinos locales y permitiendo subcolecciones.
- [x] 1.2 Mantener el destino activo en el `CaptureWatcher` compartido y
  asociar resultados `Stored` y `Duplicate` de capturas automáticas de texto e
  imagen sin cambiar el resultado si el destino dejó de ser válido.
- [x] 1.3 Agregar casos de prueba representativos para destino seleccionado,
  duplicado, ausencia de destino y destino inválido.

## 2. Adaptador Tauri y frontend

- [x] 2.1 Exponer un comando Tauri delgado para establecer o limpiar el destino
  activo de captura.
- [x] 2.2 Sincronizar el comando al seleccionar una colección/subcolección,
  `Historial` o la vista de equipo vinculado, y restaurarlo al volver a la
  vista local.
- [x] 2.3 Aplicar la misma asignación a la captura directa de texto y conservar
  el flujo de texto manual existente.
- [x] 2.4 Confirmar que los eventos existentes actualizan las membresías y
  conteos de colección sin exponer datos de captura.

## 3. Verificación

- [x] 3.1 Ejecutar pruebas relevantes de DB/core/Tauri y regresiones de
  selección frontend.
- [x] 3.2 Ejecutar `npm run check`, build frontend y regresiones de
  drag-and-drop afectadas.
- [x] 3.3 Ejecutar validación OpenSpec estricta y `git diff --check`; revisar
  que los cambios previos del usuario y los assets persistidos permanezcan
  intactos.
