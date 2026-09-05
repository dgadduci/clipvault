# Tasks: quick-paste

## 1. Tauri: ventana `quick-paste`

- [x] 1.1 Declarar la segunda ventana en `app/tauri/tauri.conf.json`
  (oculta al iniciar, ≈ 640×420, sin decoraciones, no redimensionable,
  siempre encima, fuera del taskbar).
- [x] 1.2 Crear `app/tauri/frontend/quick-paste.html` que monta
  `QuickPaste.svelte`.
- [x] 1.3 Agregar `"quick-paste"` a `windows` en
  `app/tauri/src-tauri/capabilities/default.json`.
- [x] 1.4 Confirmar que la ventana no carga lógica de negocio ni
  accede a SQLite directamente.

## 2. Hotkey por plataforma

- [x] 2.1 Corregir `map_modifiers` en
  `crates/clipvault-platform/src/runtime/hotkey_global.rs` para que
  `cmd_or_ctrl` mapee a `GhModifiers::SUPER` en macOS y
  `GhModifiers::CONTROL` en el resto.
- [x] 2.2 Cubrir con tests el mapeo para ambas plataformas y
  verificar que `default_macos_binding` y `default_linux_binding`
  siguen iguales.
- [x] 2.3 Confirmar que los tests existentes de PasteService y
  guidance siguen pasando sin cambios.

## 3. Frontend: bridge, listener único y show/focus

- [x] 3.1 Crear `app/tauri/frontend/src/lib/quickPasteBridge.ts`
  con `registerQuickSearchListener(callback)` idempotente y helpers
  para mostrar, enfocar, ocultar la ventana y emitir
  `clipvault://quick-paste-opened`.
- [x] 3.2 En `App.svelte`, dentro de `onMount`, llamar al registrar
  para suscribirse al evento del hotkey. La callback debe:
  invocar `activeApplicationCommand`, mostrar y enfocar la ventana
  y emitir `clipvault://quick-paste-opened`.
- [x] 3.3 Asegurar que `App.svelte` no envía query, snippet ni
  contenido del clipboard en eventos.
- [x] 3.4 Cubrir con tests el orden `target → show → focus → emit`
  y la idempotencia del registrar.

## 4. QuickPaste.svelte

- [x] 4.1 Crear `app/tauri/frontend/src/QuickPaste.svelte` con
  campo de búsqueda y lista.
- [x] 4.2 Mostrar `recentEntriesCommand` cuando la query está vacía
  y delegar a `runSearch` + `searchEntriesCommand` cuando no lo
  está.
- [x] 4.3 Selección por índice con `selectedIndex` y resaltado.
- [x] 4.4 Soporte de teclado: ArrowUp/ArrowDown circulares,
  Home/End, Enter para confirmar, Escape para ocultar.
- [x] 4.5 Renderizar estado vacío explícito cuando no hay
  historial.
- [x] 4.6 No llamar `pasteEntryCommand` si no hay selección.
- [x] 4.7 Evitar handlers/listeners duplicados en `onMount` y
  `onDestroy`.

## 5. Pegado

- [x] 5.1 Antes de invocar `pasteEntryCommand`, ocultar la ventana
  `quick-paste`.
- [x] 5.2 Si la respuesta es `pasted`, dejar la ventana oculta y
  limpiar la selección.
- [x] 5.3 Si la respuesta es `failed` o `capability_unavailable`,
  re-mostrar la ventana y abrir `PlatformGuidanceModal` con la
  guidance recibida (reutilizando el modal existente).
- [x] 5.4 Si la promesa rechaza (error de IPC), mismo manejo que
  `failed`.
- [x] 5.5 Verificar que la entrada histórica permanece intacta
  ante cualquier error.

## 6. runSearch revisado

- [x] 6.1 Actualizar `app/tauri/frontend/src/lib/search.ts` para
  que las cancelaciones no dejen promesas colgadas y los errores
  del backend se rechacen correctamente.
- [x] 6.2 Garantizar que los resultados obsoletos no sobrescriban
  a los nuevos.
- [x] 6.3 Actualizar `app/tauri/frontend/src/App.svelte` y
  `QuickPaste.svelte` para manejar el rechazo (try/catch).
- [x] 6.4 Cubrir los tres comportamientos con tests en
  `tests/search.test.ts`.

## 7. Tests frontend (node:test)

- [x] 7.1 Hotkey / evento abre quick-paste con el orden correcto.
- [x] 7.2 Escape cierra y cancela la búsqueda.
- [x] 7.3 Búsqueda con resultados y render correcto.
- [x] 7.4 Historial vacío muestra estado claro.
- [x] 7.5 Navegación circular ArrowUp/ArrowDown + Home/End.
- [x] 7.6 Selección de índice y Enter confirman.
- [x] 7.7 Pegado exitoso deja la ventana oculta.
- [x] 7.8 Error y `capability_unavailable` re-muestran la ventana
  con guidance.
- [x] 7.9 Enter sin selección no ejecuta paste.
- [x] 7.10 Idempotencia del registrar (no listeners duplicados).
- [x] 7.11 Privacidad: ningún evento lleva query, snippet o
  contenido.

## 8. Tests Rust

- [x] 8.1 Tests de mapeo de modificadores por plataforma en
  `clipvault-platform` (incluida la rama macOS con `cfg`).
- [x] 8.2 Tests existentes de `PasteService` y guidance siguen
  pasando.

## 9. Verificación

- [x] 9.1 Ejecutar `cargo fmt --all -- --check`.
- [x] 9.2 Ejecutar `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 9.3 Ejecutar `cargo test --workspace`.
- [x] 9.4 Ejecutar `npm run check` y `npm run build` en el frontend.
- [x] 9.5 Ejecutar `npm test` en el frontend.
- [x] 9.6 Ejecutar `openspec validate quick-paste --strict`.
- [x] 9.7 Reportar archivos modificados, decisiones, tests
  ejecutados y limitaciones manuales. **No archivar el cambio**.
