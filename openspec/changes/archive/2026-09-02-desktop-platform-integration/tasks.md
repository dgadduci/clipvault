## 1. clipvault-platform: traits por adaptador

- [x] 1.1 Definir `Capabilities` (struct plano con flags `clipboard_read/write`, `global_hotkey`, `synthetic_paste`, `active_application`, `tray`) y `detect_capabilities(&PlatformInfo) -> Capabilities`.
- [x] 1.2 Definir `ClipboardBackend` trait con `read_text() -> Result<Option<String>, ClipboardBackendError>` y `write_text(&str) -> Result<(), ClipboardBackendError>`, junto con `ClipboardBackendError` (variants `Empty`, `Backend`, `CapabilityUnavailable`).
- [x] 1.3 Definir `HotkeyManager` trait con `register(HotkeyBinding, callback) -> HotkeyOutcome`, `unregister_all()`, y `HotkeyBinding` (id, modifiers, key). `HotkeyOutcome` enum: `Registered`, `Conflict { reason }`, `Unsupported { reason }`, `Failed { reason }`.
- [x] 1.4 Definir `ActiveApplicationProbe` trait con `active_application() -> Result<Option<ActiveApplication>, ActiveAppError>` y `ActiveApplication { name, identifier }` struct serializable.
- [x] 1.5 Definir `PasteController` trait con `paste() -> Result<(), PasteError>`; `PasteError` con `CapabilityUnavailable`, `Backend`.
- [x] 1.6 Definir `TrayController` trait con `install() -> Result<TrayHandle, TrayError>` y `TrayHandle::set_menu(menu) + shutdown()`. `TrayAction` enum con `OpenMainWindow`, `OpenQuickSearch`, `OpenFavorites`, `ClearHistory`, `OpenSettings`, `Quit`.

## 2. clipvault-platform: implementaciones reales

- [x] 2.1 Implementar `ArboardClipboard` (wrapper sobre `arboard::Clipboard`) con read/write, mapeando `arboard::Error` a `ClipboardBackendError::Backend` y la falta de soporte a `CapabilityUnavailable`.
- [x] 2.2 Implementar `GlobalHotkeyManager` (wrapper sobre `global-hotkey`) con defaults `Cmd+Shift+V` (macOS) y `Ctrl+Shift+V` (Linux), exponiendo `HotkeyOutcome` correctamente.
- [x] 2.3 Implementar `MacOsActiveApplication` (objc2 + NSWorkspace) y `MacOsPasteController` (objc2-core-graphics CGEvent) bajo feature `macos-native`.
- [x] 2.4 Implementar `X11ActiveApplication` (`_NET_ACTIVE_WINDOW` via x11rb) y `X11PasteController` (`XTestFakeKeyEvent` via x11rb-protocol/xtest) bajo feature `linux-x11`.
- [x] 2.5 Implementar `WaylandPasteStub` / `WaylandActiveAppStub` que devuelven `CapabilityUnavailable` con mensaje accionable.
- [x] 2.6 Implementar `NoopPlatform` (todo `CapabilityUnavailable` salvo lo que se indique) usado en tests y hosts `Other`.

## 3. clipvault-core: integración

- [x] 3.1 Exponer `PlatformAdapters` struct (`clipboard`, `hotkey`, `active_app`, `paste`, `tray`, `capabilities`) en `clipvault_core` y re-exportar los traits de `clipvault-platform`.
- [x] 3.2 Agregar `FakeHotkeyManager`, `FakeActiveApplicationProbe`, `FakePasteController`, `FakeTrayController` con defaults `Registered`/`Some(...)`/`Ok(())`/`Ok(handle)` y helpers para inyectar errores.
- [x] 3.3 Actualizar `Clipboard` trait en `clipvault-core` para incluir `write_text(&str)` (con default que retorne `Backend("not supported")` para mantener compat con `FakeClipboard`).
- [x] 3.4 Agregar `PasteService` con `paste_entry(context, entry_id) -> PasteOutcome` que orquesta clipboard write + paste trigger; nunca modifica el historial ante un fallo.
- [x] 3.5 Agregar `CaptureWatcher` con `start(interval)`, `stop()`, `tick()` que compara hash y delega a `TextHistoryService::record_text` cuando hay cambio.
- [x] 3.6 Extender `AppContext` con `platform_adapters()` y `paste_service()`. Actualizar `Diagnostics` con `capabilities` y `active_application_supported`.

## 4. Shell Tauri delgado

- [x] 4.1 Reemplazar el bootstrap por uno que detecta capabilities y construye los adapters reales (`ArboardClipboard`, `GlobalHotkeyManager`, etc.).
- [x] 4.2 Iniciar `CaptureWatcher` desde `setup` y exponer `clipvault_capture_tick` para que el frontend pueda forzarlo.
- [x] 4.3 Registrar el hotkey default por OS y emitir el evento `clipvault://quick-search` cuando se dispara.
- [x] 4.4 Construir el `TauriTrayController` con el menú de seis acciones cableadas a eventos/comandos delgados; las que dependen de specs futuros emiten un evento `clipvault://capability-unavailable`.
- [x] 4.5 Agregar comandos `clipvault_platform_capabilities`, `clipvault_paste_entry`, `clipvault_active_application`, `clipvault_register_hotkey`, `clipvault_shutdown`.
- [x] 4.6 Implementar shutdown limpio: `on_window_event` evita cerrar al cerrar la ventana; `RunEvent::ExitRequested` para apagar tray, hotkeys, watcher y checkpoint WAL antes de salir.

## 5. Frontend mínimo

- [x] 5.1 Extender `App.svelte` para mostrar capabilities y deshabilitar los botones de paste/quick-search/active-app cuando correspondan.
- [x] 5.2 Agregar tipos `Capabilities`, `PasteResponse`, `ActiveApplication` en `types.ts` y comandos en `lib/tauri.ts`.
- [x] 5.3 Mantener la app sin acceso directo a SQLite y sin permisos de red nuevos.

## 6. Tests obligatorios

- [x] 6.1 Tests de capabilities: macOS, Linux/X11, Linux/Wayland, `Other` y entorno desconocido (sin flags de display).
- [x] 6.2 Tests de clipboard: `FakeClipboard::write_text` round-trip; `ArboardClipboard` no propagado a logs (verificar con un test que use un logger y asserts).
- [x] 6.3 Test de formatos no textuales: el `ClipboardBackend` no debe persistir nada si el SO no devuelve texto (mapeo a `Ignored`).
- [x] 6.4 Test de aplicación activa ausente: `ActiveAppError::Unavailable` no debe bloquear la captura.
- [x] 6.5 Tests de hotkey: `Registered`, `Conflict { reason }`, `Unsupported { reason }` (bajo Wayland si el compositor no soporta).
- [x] 6.6 Tests de paste: `Pasted { id }`, `Failed { kind: "paste" }` con historial intacto, `CapabilityUnavailable` bajo Wayland.
- [x] 6.7 Tests de tray: las acciones devuelven `Ok(())` cuando el controller las procesa; `ClearHistory` requiere confirmación y sólo emite el evento; shutdown no produce panic.
- [x] 6.8 Test de errores sin panic: `PlatformError::Backend` y `CapabilityUnavailable` se propagan como resultados tipados.

## 7. Verificación final

- [x] 7.1 Ejecutar `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- [x] 7.2 Ejecutar `npm run check` y `npm run build` en `app/tauri/frontend`.
- [x] 7.3 Ejecutar `openspec validate desktop-platform-integration --strict` y resolver cualquier observación.
- [x] 7.4 Confirmar que no se agregaron secretos ni logs con contenido del clipboard.
- [x] 7.5 Actualizar el spec principal `openspec/specs/desktop-platform-integration/spec.md` si los requisitos del change lo exigen (delta spec).
