# Tasks: linux-x11-quick-paste-thread-safety

## 1. Relevamiento y contexto

- [x] 1.1 Revisar el bootstrap actual, el contrato de `HotkeyManager`,
  `GlobalHotkeyManagerAdapter`, el listener `clipvault://quick-search` y la
  secuencia de activación de Quick Paste.
- [x] 1.2 Confirmar en la versión bloqueada de `global-hotkey` que el backend
  Linux crea un hilo Xlib dedicado y que el callback corre en ese hilo.
- [x] 1.3 Reproducir en un host Linux X11 real el aborto exacto con
  `Ctrl+Shift+V`, registrando sólo backend, versión/build y stderr técnico;
  no guardar contenido del portap ni datos de ventanas. Reproducido
  nuevamente tras la corrección de la ABI (`XInitThreads` no-cero →
  `Initialized`, retorno cero → `InitFailed`).
- [x] 1.4 Verificar que el cambio no se solapa con la corrección existente de
  compatibilidad `x11rb` ni cambia la autoridad entre Wayland nativo y
  XWayland.

## 2. Preflight de plataforma y orden de arranque

- [x] 2.1 Agregar la dependencia Xlib directa y mínima, si es necesaria, en
  `clipvault-platform`, con feature/cfg Linux explícito y justificación del
  límite de plataforma.
- [x] 2.2 Implementar el helper/adaptador process-wide para cargar Xlib y
  llamar `XInitThreads`, con resultado tipado, estado de inicialización única
  e información de error sin datos sensibles. La sección crítica cubre
  **completamente** el chequeo del resultado, la carga de Xlib, la llamada a
  `XInitThreads` y el almacenamiento: `XInitThreads` se ejecuta como máximo
  una vez por proceso.
- [x] 2.3 Invocar el preflight antes de `tauri::Builder::default()`, antes de
  `build_state()` y antes de construir o registrar el manager global. La
  build Linux normal incluye `linux-xlib-init` en `default` para que el
  `cfg(feature = "linux-xlib-init")` del `main.rs` compile.
- [x] 2.4 Hacer que un fallo o ausencia de Xlib seleccione el fallback
  no-op/indisponible, mantenga vivo el shell y no reporte `global_hotkey` como
  operativo cuando el manager fue omitido.
- [x] 2.5 Mantener la compilación fuera de macOS, Windows y builds sin backend
  Xlib; en Wayland no declarar soporte nativo adicional por esta corrección.
- [x] 2.6 Confirmar que no se añaden `unsafe impl Send/Sync`, llamadas Xlib
  tardías ni dependencia del frontend o del core sobre Tauri.

## 3. Contrato de activación Quick Paste

- [x] 3.1 Conservar el evento `clipvault://quick-search`, su payload vacío o
  metadata-only y el listener único del frontend.
- [x] 3.2 Confirmar que el callback del hilo X11 no llama APIs nativas de
  ventana; show, focus, centering y la emisión de apertura quedan en el flujo
  existente de Quick Paste.
- [x] 3.3 Preservar captura de la aplicación activa, target de pegado,
  selección, pegado, ocultamiento y retorno de foco.
- [x] 3.4 No modificar cards, iconos, assets persistidos, previews, búsqueda,
  drag and drop, edición de títulos ni lógica de clipboard.

## 4. Pruebas automatizadas

- [x] 4.1 Tests puros con loader/call fake:
  - retorno no-cero de `XInitThreads` → `Initialized`;
  - retorno cero de `XInitThreads` → `InitFailed`;
  - fallo del loader (`LibraryNotFound`) → `LibraryUnavailable`;
  - idempotencia (3 llamadas, una sola invocación nativa);
  - concurrencia: 16 hilos llaman `xlib_init_once` y el conteo de
    invocaciones nativas queda en 1.
- [x] 4.2 Tests de contrato:
  - `build_hotkey_returns_noop_when_xlib_preflight_fails` (loader no
    disponible → `NoopHotkeyManager`);
  - `build_hotkey_returns_noop_when_xinit_threads_rejects` (preflight
    fallido → `NoopHotkeyManager`);
  - `capabilities_drop_global_hotkey_when_xlib_preflight_failed` (la matriz
    de capabilities baja `global_hotkey` cuando el preflight falla);
  - smoke test con el loader real en este host X11: `Initialized`.
- [x] 4.3 Regresión de orden:
  - `build_hotkey_pins_source_order_invariant` (asserts estructurales sobre
    `main.rs` y `bootstrap.rs` para que el preflight preceda a
    `tauri::Builder`, `build_state` y `GlobalHotkeyManagerAdapter::new`).
- [x] 4.4 Tests del evento Quick Paste (`emit_helper_carries_no_payload` y
  las aserciones de payload `()` en el emit de `clipvault://quick-search`).
- [x] 4.5 No añadir tests de imagen que escriban en `~/.clipvault`. Los tests
  del preflight no abren displays reales ni tocan el sistema de archivos del
  usuario.

## 5. Verificación y entrega

- [x] 5.1 `cargo fmt --all -- --check` pasa sin diff.
- [x] 5.2 `cargo check -p clipvault-platform --features
  "linux-x11,hotkey-global,linux-xlib-init"` y `cargo check -p clipvault-app
  --bin clipvault-app --features
  clipboard-arboard,hotkey-global,linux-xlib-init` compilan limpio.
- [x] 5.3 Tests Rust relevantes ejecutados:
  - `cargo test -p clipvault-platform --features "linux-xlib-init" --lib
    linux_xlib_init::` (8/8 pasan, incluido el smoke test en este host
    X11);
  - `cargo test -p clipvault-app --features
    clipboard-arboard,hotkey-global,linux-xlib-init --bin clipvault-app
    bootstrap::tests::build_hotkey` (3/3 pasan);
  - `cargo test -p clipvault-app --features
    clipboard-arboard,hotkey-global,linux-xlib-init --bin clipvault-app
    bootstrap::tests::capabilities_drop` (1/1 pasa);
  - `cargo test -p clipvault-app --features
    clipboard-arboard,hotkey-global,linux-xlib-init --bin clipvault-app
    window_lifecycle` (1/1 pasa);
  - `cargo test -p clipvault-app --features
    clipboard-arboard,hotkey-global,linux-xlib-init --bin clipvault-app
    emit_helper` (1/1 pasa).
- [x] 5.4 Prueba manual Linux X11 ejecutada y aprobada:
  - instancia anterior cerrada (no había ninguna);
  - build actual arrancada desde el checkout en este workspace
    (`target/debug/clipvault-app`);
  - preflight real informa `Initialized` (`PREFLIGHT_KIND=initialized`,
    `PREFLIGHT_INITIALIZED=true`, smoke test del loader real sobre este
    host X11 confirma `Initialized`);
  - `default hotkey outcome kind="registered"` → backend Xlib real (no
    fallback no-op);
  - `Ctrl+Shift+V` en frío → `global hotkey activated id=quick_search`,
    `quick-search event emitted`;
  - dos activaciones repetidas tras ocultar → mismo log, sin aborto;
  - proceso sigue vivo al cabo de las tres activaciones;
  - stderr NO contiene `Unknown sequence number`, `xcb_io.c:278` ni
    `XInitThreads has not been called`.
- [ ] 5.5 Verificación no-regresión en Wayland/macOS: pendiente. No se ha
  ejecutado manualmente en esos hosts por ausencia de host Wayland y de
  host macOS en este entorno. No marcar por inferencia.
- [x] 5.6 `openspec validate linux-x11-quick-paste-thread-safety --strict
  --type change` → `Change 'linux-x11-quick-paste-thread-safety' is valid`;
  `git diff --check` no reporta whitespace issues.
- [x] 5.7 Sin secretos, logs con contenido, assets ni archivos generados
  fuera del alcance. Los diagnósticos del preflight son sólo
  `initialized`/`library_unavailable`/`init_failed` con razones
  sanitizadas (`libx11_unavailable`, `XInitThreads returned zero`); no se
  registran contenidos, hashes, rutas ni bytes de imágenes.

## Notas sobre la corrección aplicada

La implementación anterior tenía dos defectos que se corrigieron:

1. **Semántica de `XInitThreads` invertida**: la ABI real de Xlib
   (`/usr/include/X11/Xlib.h:1734`, `x11-dl` 2.21.0
   `pub fn XInitThreads() -> c_int`) declara `Status` no-cero = éxito,
   cero = fallo. El código previo trataba retorno cero como éxito y
   seleccionaba `NoopHotkeyManager` en hosts válidos. Se invirtió la
   comparación y se actualizaron mensajes, tests y bootstrap test que
   esperaba la razón incorrecta.

2. **Carrera entre el chequeo y la llamada**: el mutex se liberaba antes
   de `run_preflight`, así que dos hilos concurrentes podían invocar
   `XInitThreads` por separado. Ahora la sección crítica cubre todo el
   preflight (chequeo → carga → llamada → almacenamiento). Se añadió un
   test de concurrencia (16 hilos) que verifica que el contador de
   invocaciones nativas queda en 1.

3. **Feature no activado en la build por defecto**: `linux-xlib-init` se
   habilitaba en `clipvault-platform` por la dependencia target-specific,
   pero Cargo no propaga features del dependiente al `[features]` del
   consumidor. El `#[cfg(feature = "linux-xlib-init")]` del `main.rs`
   quedaba inactivo. Se agregó `linux-xlib-init` al `default` del shell.
   El `cfg(target_os = "linux")` del call site garantiza que sólo se
   ejecuta en Linux.
