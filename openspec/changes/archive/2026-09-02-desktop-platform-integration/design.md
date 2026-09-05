# Design: desktop-platform-integration

## Context

`bootstrap-clipvault` asentó el workspace, el shell Tauri 2 y la base SQLite;
`clipboard-text-history` materializó la captura de texto con `FakeClipboard`
como adaptador inyectado por `AppBootstrap`. Falta la integración real con
el SO: clipboard OS-level, hotkey global, detección de aplicación activa,
pegado sintético y tray/menu bar. El spec `desktop-platform-integration`
codifica cuatro requisitos:

1. La lógica de negocio debe estar detrás de adaptadores testeables para
   clipboard, hotkeys, paste, tray y plataforma.
2. macOS debe soportar captura, hotkey, paste y tray/menu bar.
3. Linux debe distinguir X11 y Wayland y no asumir operaciones X11 bajo
   Wayland. Cuando una capacidad no esté disponible, el resto de la app
   debe seguir funcionando.
4. La entrada de tray/menu bar debe ofrecer seis acciones mínimas y
   permitir cerrar la app limpiamente sin perder filas en SQLite.

Hoy `clipvault-platform` sólo expone `PlatformInfo` (home/data dir, OS,
display server) y `DefaultPlatform::detect`. El resto de los traits viven
en `clipvault-core` como `Clipboard` (sólo `read_text`) y `Clock`.

## Goals / Non-Goals

**Goals:**

- Mantener `clipvault-core` libre de Tauri/SO: los nuevos traits viven en
  el core con fakes, y el shell Tauri inyecta los adapters reales vía
  `AppBootstrap`.
- Implementar los adapters reales para macOS y Linux (X11/Wayland) usando
  crates maduros y mínimos: `arboard`, `global-hotkey`, `x11rb` y
  `objc2`/`objc2-app-kit`.
- Modelar capacidades por plataforma y devolver `CapabilityUnavailable`
  con mensaje accionable cuando una operación no sea posible (especialmente
  bajo Wayland: sin pegado sintético, sin aplicación activa, hotkey
  dependiendo del compositor).
- Conectar la pipeline de captura al watcher real: `arboard` cuando
  ofrezca eventos, polling controlado como fallback (1–2 Hz).
- Exponer el estado de capacidades al frontend para que pueda deshabilitar
  controles y reportar limitaciones.
- Cerrar limpiamente al salir: detener captura, watchers, hotkeys y tray
  antes de retornar, sin perder filas confirmadas en SQLite.

**Non-Goals:**

- Búsqueda fuzzy, favoritos, retención, settings, snippets, transformaciones,
  detección de secretos, import/export. Esos specs tienen cambios
  dedicados; las acciones de tray que los invocan quedan cableadas a
  eventos/comandos delgados que reportan indisponibilidad.
- Soporte de imágenes u otros formatos de clipboard: el spec exige ignorar
  lo no textual sin cerrar la app (lo que ya cumple `TextHistoryService`).
- Linux Wayland sintético: no se intenta simular pegado. Se devuelve
  `CapabilityUnavailable` y la app sigue funcionando.
- Empaquetado de instaladores ni CI/CD.

## Decisions

### Traits nuevos en `clipvault-platform`

`clipvault-platform` deja de ser sólo un stub de `PlatformInfo` y se
reorganiza en módulos por adaptador con su trait público:

```text
clipvault-platform
├── info.rs           (PlatformInfo, OsFamily, DisplayServer)
├── capabilities.rs   (Capabilities + Capability matrix)
├── clipboard.rs      (ClipboardBackend trait + read/write + capability flags)
├── hotkey.rs         (HotkeyManager trait, HotkeyBinding, HotkeyOutcome)
├── active_app.rs     (ActiveApplicationProbe trait)
├── paste.rs          (PasteController trait)
├── tray.rs           (TrayController trait + TrayAction enum)
├── stub.rs           (NoopPlatform: implementations que devuelven
│                      CapabilityUnavailable, usadas en tests y en hosts
│                      no soportados)
└── runtime/          (impls reales por OS, gated por feature flags)
    ├── arboard_clipboard.rs
    ├── global_hotkey.rs
    ├── macos/
    │   ├── active_app.rs   (NSWorkspace via objc2)
    │   └── paste.rs        (CGEventCreateKeyboardEvent via objc2)
    └── linux/
        ├── x11/
        │   ├── active_app.rs   (_NET_ACTIVE_WINDOW via x11rb)
        │   └── paste.rs        (XTestFakeKeyEvent via x11rb-protocol/xtest)
        └── wayland.rs       (mark capabilities as unsupported)
```

Cada trait expone un único método `name()` que devuelve un identificador
estático, útil para diagnosticar y para que el frontend pueda deshabilitar
controles. Los errores son tipados (`ClipboardError`, `HotkeyError`,
`PasteError`, `ActiveAppError`, `TrayError`, `CapabilityError`) con un
variant `CapabilityUnavailable { capability, reason }` consistente.

### Capabilities

`Capabilities` es un struct plano, `Clone + Copy`, derivable:

```rust
pub struct Capabilities {
    pub clipboard_read: bool,
    pub clipboard_write: bool,
    pub global_hotkey: bool,
    pub synthetic_paste: bool,
    pub active_application: bool,
    pub tray: bool,
}
```

`detect_capabilities(info: &PlatformInfo) -> Capabilities` consulta el
entorno (`WAYLAND_DISPLAY`, `DISPLAY`, `objc2` availability) y decide
qué hay disponible. Bajo Wayland: `clipboard_read = true`,
`clipboard_write = true`, `global_hotkey = true` (best-effort vía
`global-hotkey`/wlr), `synthetic_paste = false`, `active_application =
false`, `tray = true` (status-notifier-item es compatible). Bajo
`Other`: todo `false`. Bajo macOS: todo `true`.

### Clipboard backend (`arboard`)

- Dependencia elegida: `arboard = "3"`.
- Por qué: crate único, mantenido, provee clipboard read/write cross-platform
  con backends Wayland vía XDG portal, X11 vía Xlib, macOS vía
  `NSPasteboard`. Es la opción estándar para apps Rust locales.
- Compatibilidad:
  - macOS: usa `NSPasteboard` nativo (probado y robusto).
  - Linux X11: usa Xlib directo (sin portal).
  - Linux Wayland: requiere portal (`xdg-desktop-portal`) y
    `wl-clipboard`/`clipboard-portal` corriendo en el usuario; si no
    está disponible devuelve `CapabilityUnavailable` y la pipeline cae
    al polling fallback (que también falla silenciosamente y devuelve
    `Ignored`).
- Limitaciones: imágenes/formatos no textuales son ignorados
  transparentemente; en Wayland el primer read puede requerir interacción
  con el portal.
- Alternativas descartadas:
  - `xclipboard`/`x11-clipboard`: sólo X11, no compilan en macOS.
  - `copypasta`: similar a arboard pero menos mantenido.
  - Implementación propia por OS: multiplica superficie a mantener sin
    beneficio.

### Watcher / polling

- `arboard` no expone eventos de cambio cross-platform. Se opta por un
  watcher por polling a 1.5 Hz con backoff: en cada tick compara el hash
  del último texto leído contra el anterior; sólo dispara
  `TextHistoryService::record_text` cuando cambia.
- Por qué polling y no WebSocket D-Bus: evita dependencias adicionales
  (`tokio`, `zbus`) para algo que el spec no exige y que puede fallar
  bajo sandboxing macOS.
- Limitaciones: latencia de hasta ~1.5 s entre copia y captura; el spec
  no exige captura instantánea y el MVP es aceptable con esta frecuencia.
- Alternativas descartadas:
  - `notify-rust` / `tauri-plugin-clipboard`: atan el watcher a Tauri;
    queremos que viva en el core detrás de un trait.
  - `tokio` + D-Bus: añade runtime async y una dependencia pesada para
    una mejora marginal.

### Hotkeys (`global-hotkey`)

- Dependencia elegida: `global-hotkey = "0.6"`.
- Por qué: provee registro cross-platform de hotkeys globales (macOS
  Carbon, X11, Wayland vía wlr-global-shortcuts portal), con callback
  de evento y shutdown limpio (`HotkeyManager::unregister_all`).
- Compatibilidad:
  - macOS: estable.
  - Linux X11: estable.
  - Linux Wayland: depende del compositor (Sway/KDE modernos lo
    soportan vía wlr-global-shortcuts; GNOME requiere extensión).
- Limitaciones: si el atajo ya está tomado por otra app, `register`
  devuelve un error específico que se mapea a `HotkeyError::Conflict`.
- Alternativas descartadas:
  - `tauri-plugin-global-shortcut`: ata la hotkey al shell, contradice
    la separación core/Tauri.
  - `device_query`: hotkeys globales pero con API menos estable.

### Detección de aplicación activa y paste sintético

- macOS (dependencias `objc2`, `objc2-app-kit`, `objc2-core-graphics`):
  - Active app: `NSWorkspace::sharedWorkspace().frontmostApplication()`
    vía `objc2`.
  - Paste: `CGEventCreateKeyboardEvent` con `keycode 9` (V) y flags
    `kCGEventFlagMaskCommand` (macOS) / `kCGEventFlagMaskControl` (Linux
    no aplica).
- Linux X11 (dependencia `x11rb = "0.13"` con feature `allow-unsafe-code`
    desactivada, sólo API síncrona):
  - Active app: `EWMH._NET_ACTIVE_WINDOW` + `_NET_WM_NAME` /
    `WM_CLASS` sobre la ventana root.
  - Paste: `XTestFakeKeyEvent` con keycode 55 (`v`) y modifier Control.
- Linux Wayland: `CapabilityUnavailable` en ambas capabilities; no se
  intenta fallback X11 (la spec lo prohíbe explícitamente).
- Otros: `CapabilityUnavailable`.

Limitaciones documentadas: pegar en Wayland requiere un portal de
"input emulation" que no está estandarizado; declarar el límite es
preferible a fingir funcionalidad. En macOS, si la app destino es
sandboxed y rechaza el Cmd+V, devolveremos `PasteError::Backend`.

### Tray / menu bar

- La responsabilidad del shell Tauri: `tauri::tray::TrayIconBuilder`
  ofrece un API multiplataforma (macOS: menu bar item; Linux: status
  notifier item) y no añade dependencias nuevas.
- `clipvault-platform` expone `TrayController` (trait) y `TrayAction`
  (enum). El shell Tauri implementa `TrayController` con una
  `TauriTrayController` que arma el menú en `setup` y traduce
  `TrayAction::OpenQuickSearch` → emisión del evento `clipvault://quick-search`,
  `TrayAction::OpenFavorites` → `clipvault://favorites` (reporta
  indisponibilidad mientras no exista el spec), etc.
- Las acciones que dependen de specs no entregados quedan conectadas a
  eventos/comandos delgados y devuelven `TrayOutcome::Unavailable { reason }`
  cuando el frontend aún no las implementa.

### Paste pipeline (`PasteService` en el core)

`PasteService::paste_entry(context, entry_id) -> PasteOutcome`:

1. Carga la `EntryRecord` por id (sin modificarla). Si no existe, devuelve
   `PasteOutcome::Failed { kind: "not_found", message }`.
2. Llama al `ClipboardBackend` para escribir el texto. Si falla, devuelve
   `PasteOutcome::Failed { kind: "clipboard", message }`. **No se
   modifica el historial.**
3. Llama al `PasteController::paste()`. Si la capability no está
   disponible, devuelve `PasteOutcome::CapabilityUnavailable`. Si falla,
   devuelve `PasteOutcome::Failed { kind: "paste", message }`. **No se
   modifica el historial.**
4. Si todo OK, devuelve `PasteOutcome::Pasted { id }`.

El servicio no usa `tracing` para loggear contenido; sólo categorías
y errores genéricos.

### Shell Tauri

- `setup` ahora:
  1. Construye `PlatformAdapters::detect(&platform_info)` con los
     adapters reales (clipboard `arboard`, hotkey `global-hotkey`, etc.).
  2. Construye `AppContext` vía `AppBootstrap` con esos adapters.
  3. Crea un `CaptureWatcher` (1.5 Hz) que llama a `record_text`.
  4. Registra el hotkey default por OS.
  5. Construye el `TrayController` y cablea las acciones a eventos Tauri.
  6. `app.manage(AppState { context, watcher, hotkey, tray })`.
- Shutdown (`on_window_event` con `WindowEvent::CloseRequested`,
  interceptado para evitar salir cuando se cierra la ventana — la app
  sigue en tray):
  - `tray.shutdown()`.
  - `hotkey.unregister_all()`.
  - `watcher.stop()`.
  - `db.flush()` — sqlite cierra al hacer `drop`; no requiere flush
    explícito, pero se llama `connection().execute("PRAGMA wal_checkpoint(TRUNCATE)")`
    en `Drop` para confirmar el WAL.
- Comandos Tauri delgados nuevos:
  - `clipvault_platform_capabilities`: devuelve `Capabilities`.
  - `clipvault_paste_entry { id }`: delega en `PasteService`.
  - `clipvault_active_application`: devuelve `Option<ActiveApplication>`.

### Frontend

- `tauri.conf.json` queda igual (sin nuevos permisos: el menu bar es
  nativo y los hotkeys se registran desde Rust; no se necesita ningún
  capability extra de Tauri).
- `App.svelte` agrega una sección de capacidades (lee
  `clipvault_platform_capabilities`) y deshabilita los botones cuyo
  capability sea `false`.
- TypeScript `types.ts` agrega `Capabilities`, `PasteResponse`,
  `ActiveApplication`.

### Testing

- Tests unitarios nuevos en `clipvault-platform` (sin GUI, sin X11/Wayland):
  - `detects_macos_capabilities` (con `OsFamily::Macos`).
  - `detects_linux_x11_capabilities` (con `DisplayServer::X11`).
  - `detects_linux_wayland_capabilities` (paste/active_app en false).
  - `detects_unknown_environment_capabilities` (todo false salvo
    `clipboard_read` si hay un backend disponible).
- Tests en `clipvault-core`:
  - `PasteService::paste_entry` con `FakeClipboard` y un
    `FakePasteController`; verifica que ante fallo de paste el
    historial no se modifica.
  - `CaptureWatcher` con `FakeClipboard`; verifica que dispara el
    callback sólo cuando cambia el contenido.
  - `HotkeyManager::register` → `Registered`, `Conflict`,
    `Unsupported` con un `FakeHotkeyManager`.
- Tests de integración (en `crates/clipvault-core/tests/`):
  - `platform_capabilities` con un `NoopPlatform` configurado para
    simular cada entorno.

### Dependencias nuevas (todas justificadas)

| Dependencia               | Uso                                  | Compatibilidad                  | Alternativa descartada                         |
|---------------------------|--------------------------------------|----------------------------------|------------------------------------------------|
| `arboard` 3               | Clipboard read/write cross-platform  | macOS, Linux X11, Linux Wayland  | `xclipboard`/`copypasta`/propia                |
| `global-hotkey` 0.6       | Hotkeys globales cross-platform      | macOS, Linux X11, Wayland (best-effort) | `tauri-plugin-global-shortcut`, `device_query` |
| `x11rb` 0.13              | Active app + paste en X11            | Linux X11                       | shell-out a `xdotool`/`xprop`: menos testeable |
| `objc2`, `objc2-app-kit`, `objc2-core-graphics` | Active app + paste en macOS | macOS                      | `cocoa`/`core-graphics`: crates antiguos       |

Ninguna dependencia nueva introduce red, telemetría o servicios
externos.

## Risks / Trade-offs

- **Wayland sintético**: se devuelve `CapabilityUnavailable`; los usuarios
  con Wayland estricto no tendrán pegado automático hasta que aparezca
  un portal. → Documentado en el comando de capabilities y en el frontend.
- **`global-hotkey` en Wayland depende del compositor**: si el usuario
  corre uno sin soporte, el registro falla y la app sigue funcionando
  con hotkey no disponible. → `HotkeyOutcome::Unsupported { reason }`
  reportado al `Diagnostics`.
- **Polling a 1.5 Hz puede perder capturas rápidas**: aceptable para el
  MVP; el spec no exige captura instantánea y el código es trivialmente
  actualizable a D-Bus en un cambio futuro.
- **macOS `objc2` es un crate grande**: ~80K LOC, pero es el reemplazo
  oficial mantenido por Apple del antiguo `objc`. La alternativa
  `cocoa` está abandonada.
- **`x11rb` requiere `unsafe` mínimo**: usamos sólo APIs síncronas
  seguras; el código se concentra en `clipvault-platform::runtime::linux::x11`
  y queda aislado detrás del trait.

## Migration Plan

- No hay DB migrations nuevas: este cambio no toca el esquema.
- El cambio es compatible hacia atrás para tests existentes:
  `FakeClipboard` mantiene `read_text` y agrega `write_text` opcional
  con un default que sólo persiste en memoria.
- El shell Tauri pasa de `FakeClipboard` a `arboard` automáticamente
  al actualizar; los tests de `clipvault-core` siguen usando fakes.

## Open Questions

- ¿Vale la pena un canal D-Bus de eventos de clipboard en Wayland para
  reducir latencia? (fuera de alcance MVP; lo dejamos como follow-up).
- ¿Conviene centralizar la configuración de hotkeys en una `Settings`
  table? (pertenece al spec `privacy-settings`, lo dejamos como evento
  pendiente).
