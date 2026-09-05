## Why

`bootstrap-clipvault` y `clipboard-text-history` dejaron una pipeline completa
sobre `FakeClipboard` pero todavía sin conectar al portapapeles real del
sistema operativo. El spec `desktop-platform-integration` exige que el MVP
materialice los adaptadores reales para macOS y Linux (X11/Wayland), registre
hotkeys globales, soporte quick-paste, exponga una entrada de tray/menu bar,
detecte la aplicación activa cuando sea posible y modele explícitamente las
capacidades disponibles por plataforma. Sin este cambio, la pipeline de
captura queda desconectada del SO y la experiencia diaria (atajo → buscar →
pegar) no puede existir.

## What Changes

- Reorganizar `clipvault-platform` para exponer traits separados y estables
  para `Clipboard`, `Hotkey`, `PasteController`, `ActiveApplicationProbe`,
  `TrayController` y detección de `Capabilities`, detrás de implementaciones
  reales para macOS, Linux+X11, Linux+Wayland y entornos desconocidos.
- Reemplazar el `FakeClipboard` por defecto en el shell Tauri por una
  implementación real (`arboard`) y conectar un watcher por polling
  controlado que alimenta `TextHistoryService::record_text` cuando el SO
  no ofrece eventos de cambio.
- Implementar detección de aplicación activa: `NSWorkspace` en macOS,
  `_NET_ACTIVE_WINDOW` con `x11rb` en X11, y `CapabilityUnavailable` bajo
  Wayland. Ningún flujo debe propagar el contenido del portapapeles a logs.
- Registrar hotkeys globales con defaults `Cmd+Shift+V` (macOS) y
  `Ctrl+Shift+V` (Linux). Manejar éxito, conflicto y sesión sin soporte
  sin abortar la app, y exponer la configuración interna para cambiarlos.
- Implementar pegado sintético con primitives de plataforma: escribir
  temporalmente el texto seleccionado, ejecutar la acción de pegado nativa
  (CGEvent en macOS, XTest en X11) y devolver errores tipados. Bajo Wayland
  devolver `CapabilityUnavailable` sin caer en operaciones X11 silenciosas.
- Agregar tray/menu bar con seis acciones (abrir ventana, abrir quick
  search, favoritos, limpiar historial con confirmación, settings, salir);
  las acciones que dependan de specs pendientes quedan conectadas vía
  eventos/comandos delgados y reportan indisponibilidad explícita.
- Apagar captura, watchers, hotkeys y tray limpiamente al cerrar la app
  sin perder filas ya confirmadas en SQLite.
- Actualizar el setup de Tauri para usar adapters reales, mantener un
  `AppState` compartido y registrar únicamente comandos delgados. La
  detección de capacidades se expone por comando para que el frontend
  pueda deshabilitar controles cuando una capacidad no esté disponible.

## Capabilities

### New Capabilities

- `desktop-platform-integration`: define la frontera de adaptadores, el
  soporte explícito para macOS, Linux+X11 y Linux+Wayland, y la entrada
  de tray/menu bar con acciones mínimas.

### Modified Capabilities

- (ninguna: este cambio implementa el spec ya existente sin modificar
  requisitos).

## Impact

- `crates/clipvault-platform`: se reorganiza en módulos por adaptador y se
  agregan dependencias maduras y justificadas (`arboard`, `global-hotkey`,
  `x11rb` para Linux X11, `objc2`/`objc2-app-kit` para macOS nativo). Las
  dependencias nuevas y sus alternativas se documentan en `design.md`.
- `crates/clipvault-core`: nuevos traits, fakes y servicios
  (`PasteService`, `CaptureWatcher`, `PlatformAdapters`). El `AppContext`
  pasa a exponer los adapters a través de `Arc<dyn ...>` para que el shell
  pueda consumirlos sin filtrar SQL o platform APIs al frontend.
- `app/tauri/src-tauri`: setup con adapters reales, registro de hotkey,
  construcción del tray, manejo del watcher y comandos delgados adicionales
  (`clipvault_platform_capabilities`, `clipvault_paste_entry`).
- Sin nuevas llamadas de red, telemetría, cloud, LLMs ni servicios
  externos. El frontend sigue sin acceso directo a SQLite.
