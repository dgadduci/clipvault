# Tareas de implementación

Todas las tareas deben ejecutarse sobre este cambio. MiniMax implementa; Codex mantiene el contrato y revisa las regresiones.

## 1. Auditoría y baseline

- [x] 1.1 Leer `AGENTS.md`, `project.md`, `projects.md` y todos los artefactos de este cambio antes de modificar código.
- [x] 1.2 Revisar la implementación actual de `ActiveApplicationProbe`, la caché del shell, la selección X11/XWayland y `LinuxApplicationMetadataProvider`.
- [x] 1.3 Ejecutar y registrar el baseline de Rust, frontend y validación OpenSpec sin modificar datos de `~/.clipvault`.
- [x] 1.4 Confirmar que no se crea una segunda caché/watcher ni se toca el controlador protegido de drag-and-drop.

## 2. Wire protocol y adapter nativo Wayland

- [x] 2.1 Corregir el handshake Wayland para que `wl_display` sea el
  id 1, `wl_registry.get_registry` (opcode 1) cree un nuevo id para
  el registry, y `wl_registry.bind` (opcode 0) acepte argumentos en
  el orden `name` (u32, global name), `new_id` (u32), `interface`
  (string) y `version` (u32). Tests byte-level pin cada layout.
- [x] 2.2 Aceptar y respetar la versión anunciada por el compositor,
  rechazando bind por debajo de la versión mínima soportada.
- [x] 2.3 Decodificar correctamente el evento `toplevel` de
  `ext-foreign-toplevel-list-v1` como un `new_id` (no como un
  `(handle, app_id)`), registrar el handle y consumir los eventos
  `closed`, `done`, `title`, `app_id` e `identifier` del handle.
  Confirma mediante tests que ext nunca se usa para inferir foco.
- [x] 2.4 Decodificar correctamente el evento `state` de
  `zwlr_foreign_toplevel_handle_v1` como un `wl_array` cuyo tamaño
  va en bytes (no en elementos), validar alineación, e interpretar
  el estado `activated == 2`.
- [x] 2.5 Hacer que la sonda sólo devuelva `Operational` cuando el
  handshake completo y el enlace de un protocolo capaz de resolver
  foco (zwlr o ambos) hayan tenido éxito. Sin zwlr, devolver
  `Unavailable` con causa `registry_without_protocol`.
- [x] 2.6 Eliminar el camino `WAYLAND_SOCKET` (`/proc/self/fd/<fd>`).
  La sonda sólo abre `$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY`.
- [x] 2.7 Publicar la causa granular (`socket_unavailable`,
  `handshake_failed`, `registry_without_protocol`,
  `incompatible_version`, `bind_rejected`, `connected`,
  `compositor_disconnected`, `snapshot_uncommitted`,
  `no_active_toplevel`, `app_id_identified`) en el snapshot para
  que la tarjeta de diagnósticos la muestre.
- [x] 2.8 Mantener la selección determinista (handle más bajo entre
  los activados) sólo dentro de zwlr; nunca usar orden de creación,
  título, PID ni `/proc` como sustituto de foco.

## 3. Integración del shell y precedencia

- [x] 3.1 Integrar la sonda en la única ruta de construcción de la
  aplicación activa en Linux.
- [x] 3.2 En Wayland, dar precedencia al snapshot nativo cuando
  esté operativo y evitar fallback a una identidad XWayland
  obsoleta ante `Ok(None)`.
- [x] 3.3 Mantener X11 puro y XWayland existentes; usar el fallback
  XWayland sólo si la sonda nativa no está operativa y `$DISPLAY`
  es usable.
- [x] 3.4 Conservar una sola caché y un solo `CaptureWatcher`
  compartido entre loop y Tick manual.
- [x] 3.5 Mantener el orden refresh/snapshot → identificador →
  PrivacyGate → persistencia → enriquecimiento → evento.

## 4. Metadata e iconos

- [x] 4.1 Reutilizar `LinuxApplicationMetadataProvider` sin
  duplicar parser `.desktop`, búsqueda XDG ni almacenamiento de
  iconos.
- [x] 4.2 Conservar el identificador y el nombre aunque el icono
  no sea resoluble; persistir PNG bajo
  `assets/application-icons/` cuando corresponda.
- [x] 4.3 Asegurar que la blacklist ocurre antes del provider y no
  crea filas ni assets para una aplicación ignorada.

## 5. Diagnósticos y versión

- [x] 5.1 Añadir backend/etapas estables para distinguir
  identificado, sin toplevel, protocolo no disponible y
  desconexión, sin títulos/PID/rutas.
- [x] 5.2 Exponer capacidades de forma honesta: no reportar soporte
  nativo si el compositor no publica zwlr.
- [x] 5.3 Documentar que un `app_id` Wayland tiene el mismo modelo
  de confianza que `WM_CLASS` y no es una prueba de identidad del
  proceso.
- [x] 5.4 Incrementar la versión patch una sola vez al completar la
  corrección funcional: 0.0.9 → 0.0.10, en manifests canónicos,
  lockfiles necesarios y `projects.md`; About debe seguir leyendo
  la versión de diagnostics.

  Nota: el cambio incrementa la versión canónica de `0.0.9` a
  `0.0.10` en `Cargo.toml`,
  `app/tauri/src-tauri/tauri.conf.json` y
  `app/tauri/frontend/package.json`. `projects.md` queda alineado
  con la nueva versión y el modal "Acerca de" sigue leyendo
  `diagnostics.version`.

## 6. Pruebas automatizadas y no-regresiones

- [x] 6.1 Tests de byte-level para el handshake: `wl_display` id 1,
  `get_registry` con `new_id` correcto, `bind` con orden exacto y
  versión anunciada.
- [x] 6.2 Tests de protocolo: registry anuncia protocolo, anuncia
  versión incompatible, no anuncia ninguno, `global_remove` no
  rompe el flujo, compositor cierra la conexión mid-round.
- [x] 6.3 Tests ext: `toplevel` como `new_id`, `app_id`/`done`/
  `closed`/`identifier` del handle, garantía de que ext no se usa
  para inferir foco.
- [x] 6.4 Tests zwlr: `app_id`, `state` con `wl_array` de bytes,
  `activated == 2`, snapshot sólo después de `done`, `closed`
  elimina el handle, varios toplevels con selección determinista.
- [x] 6.5 Tests de precedencia: Wayland nativo autoritativo sobre
  XWayland, `Ok(None)` no reutiliza una identidad XWayland
  obsoleta, fallback XWayland sólo cuando el nativo no está
  operativo y `$DISPLAY` es usable.
- [x] 6.6 Tests de integración con
  `LinuxApplicationMetadataProvider`, blacklist antes del provider
  y sin assets para entradas rechazadas.
- [x] 6.7 Tests de privacidad: ningún log con contenido, snippets,
  hashes, `asset_ref`, rutas, títulos, PID ni variables de entorno.
- [x] 6.8 Regresiones: imágenes antes/después de reinicio, tags,
  colecciones, favoritos, búsqueda, Quick Paste y drag-and-drop.
- [x] 6.9 `cargo fmt --all -- --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace`,
  `cargo check -p clipvault-platform --features
  linux-wayland-active-app --target x86_64-unknown-linux-gnu
  --tests`, `npm ci`, `npm run check`, `npm run build`,
  `npm test`, `openspec validate linux-native-wayland-app-detection
  --strict --type change`.

## 7. Verificación manual Ubuntu — pendiente del usuario

- [ ] 7.1 En Ubuntu GNOME Wayland con un compositor que anuncie
  `zwlr_foreign_toplevel_management_unstable_v1` (por ejemplo
  sway, Wayfire, Hyprland), abrir una aplicación nativa y
  comprobar que el `app_id` se publica. **No** marcar como pasada
  en macOS.
- [ ] 7.2 Repetir con Firefox y Chrome nativos en Wayland bajo
  zwlr, verificando que el .desktop, el icono y Quick Paste
  funcionen.
- [ ] 7.3 Repetir con una aplicación XWayland (Warp, synaptic,
  xTerm) y confirmar que el comportamiento existente no regresa.
- [ ] 7.4 Probar blacklist sobre un `app_id` nativo Wayland: la fila
  y el icono no deben escribirse.
- [ ] 7.5 Reiniciar ClipVault y verificar persistencia de
  nombre/icono, imágenes, tags, colecciones y favoritos.
- [ ] 7.6 En una sesión donde ningún compositor anuncia zwlr ni
  ext (por ejemplo Ubuntu GNOME Wayland con Mutter), comprobar
  que el diagnóstico es `Unavailable` sin crash y sin identidad
  inventada.
- [ ] 7.7 Registrar distro, compositor, sesión, versión de
  ClipVault, protocolo anunciado y resultado. **No** marcar estas
  tareas desde macOS.

## 8. Cierre

- [x] 8.1 Revisar diff: el cambio cubre el reescrito del adapter
  nativo (wire protocol, eventos ext/zwlr, granular
  `ConnectionOutcome`, `WAYLAND_SOCKET` eliminado,
  `LinuxApplicationMetadataProvider` reutilizado, manifestos
  canónicos a `0.0.10`, `projects.md`). No se tocaron datos del
  usuario, assets existentes, ni archivos fuera del repositorio.
- [x] 8.2 No archivar ni sincronizar automáticamente; dejar el
  cambio activo hasta completar la validación Ubuntu y recibir la
  orden explícita.
