# Tareas de implementación

Todas las tareas deben ejecutarse sobre este cambio. MiniMax implementa; Codex mantiene el contrato y revisa las regresiones.

## 1. Auditoría y baseline

- [x] 1.1 Leer `AGENTS.md`, `project.md`, `projects.md` y todos los artefactos de este cambio antes de modificar código.
- [x] 1.2 Revisar la implementación actual de `ActiveApplicationProbe`, la caché del shell, la selección X11/XWayland y `LinuxApplicationMetadataProvider`.
- [x] 1.3 Ejecutar y registrar el baseline de Rust, frontend y validación OpenSpec sin modificar datos de `~/.clipvault`.
- [x] 1.4 Confirmar que no se crea una segunda caché/watcher ni se toca el controlador protegido de drag-and-drop.

## 2. Adapter nativo Wayland

- [x] 2.1 Elegir y documentar crates/versiones compatibles con Rust 1.89 para bindings Wayland; la feature debe ser opcional y sólo Linux.
- [x] 2.2 Implementar el adapter para `ext-foreign-toplevel-list-v1` con bindings incorporados/generados sin procesos externos en runtime.
- [x] 2.3 Implementar fallback para `zwlr_foreign_toplevel_management_unstable_v1` cuando el compositor lo anuncie.
- [x] 2.4 Mantener la conexión y dispatch en un componente seguro para hilos; la consulta del probe debe ser no bloqueante.
- [x] 2.5 Publicar propiedades sólo después de `done`, incluyendo `app_id` y estado de activación; descartar campos no requeridos como título.
- [x] 2.6 Eliminar toplevels al recibir `closed`, tratar app_id vacío como ausencia y definir selección determinista si hay varios activos.
- [x] 2.7 Convertir protocolo ausente, versión incompatible, rechazo y desconexión en errores tipados `Unavailable` sin panic.

## 3. Integración del shell y precedencia

- [x] 3.1 Integrar el adapter en la única ruta de construcción de la aplicación activa en Linux.
- [x] 3.2 En Wayland, dar precedencia al snapshot nativo cuando esté operativo y evitar fallback a una identidad XWayland obsoleta ante `Ok(None)`.
- [x] 3.3 Mantener X11 puro y XWayland existentes; usar el fallback XWayland sólo si el adapter nativo no está operativo y `$DISPLAY` es usable.
- [x] 3.4 Conservar una sola caché y un solo `CaptureWatcher` compartido entre loop y Tick manual.
- [x] 3.5 Mantener el orden refresh/snapshot → identificador → PrivacyGate → persistencia → enriquecimiento → evento.

## 4. Metadata e iconos

- [x] 4.1 Reutilizar `LinuxApplicationMetadataProvider` sin duplicar parser `.desktop`, búsqueda XDG ni almacenamiento de iconos.
- [x] 4.2 Verificar resolución de `app_id` para Terminal GNOME, Firefox y Chrome nativos cuando publiquen metadata compatible.
- [x] 4.3 Conservar el identificador y el nombre aunque el icono no sea resoluble; persistir PNG bajo `assets/application-icons/` cuando corresponda.
- [x] 4.4 Asegurar que blacklist ocurre antes del provider y no crea filas ni assets para una aplicación ignorada.

## 5. Diagnósticos y versión

- [x] 5.1 Añadir backend/etapas estables para distinguir identificado, sin toplevel, protocolo no disponible y desconexión, sin títulos/PID/rutas.
- [x] 5.2 Exponer capacidades de forma honesta: no reportar soporte nativo si el protocolo no está disponible.
- [x] 5.3 Documentar que un `app_id` Wayland tiene el mismo modelo de confianza que `WM_CLASS` y no es una prueba de identidad del proceso.
- [x] 5.4 Incrementar la versión patch una sola vez al completar la implementación funcional: 0.0.8 → 0.0.9, en manifests canónicos, lockfiles necesarios y `projects.md`; About debe seguir leyendo la versión de diagnostics.

  Nota: el cambio incrementa la versión canónica de `0.0.8` a `0.0.9` en `Cargo.toml`, `app/tauri/src-tauri/tauri.conf.json` y `app/tauri/frontend/package.json`. `projects.md` queda alineado con la nueva versión y el modal "Acerca de" sigue leyendo `diagnostics.version`.

## 6. Pruebas automatizadas y no-regresiones

- [x] 6.1 Probar registry, protocolo compatible, versión incompatible y ausencia del protocolo con fakes.
- [x] 6.2 Probar `app_id` sólo después de `done`, selección determinista, `closed`, app_id vacío y desconexión.
- [x] 6.3 Probar precedencia Wayland nativa, `Ok(None)` sin fallback obsoleto y fallback XWayland cuando corresponde.
- [x] 6.4 Probar integración con el provider Linux, nombre/icono, icono ausente y blacklist antes de metadata.
- [x] 6.5 Probar que no se filtran contenido, snippets, hashes, asset refs, rutas absolutas, títulos, PID ni variables de entorno.
- [x] 6.6 Ejecutar regresiones de imágenes antes/después de reinicio, tags, colecciones, favoritos, búsqueda, Quick Paste y drag-and-drop.
- [x] 6.7 Ejecutar fmt, clippy con `-D warnings`, tests Rust, tests frontend, check/build frontend y `openspec validate ... --strict`.

## 7. Verificación manual Ubuntu — pendiente del usuario

- [ ] 7.1 En Ubuntu GNOME Wayland, probar captura desde Terminal Ubuntu nativa y comprobar `source_app`, nombre e icono.
- [ ] 7.2 Probar Firefox y Chrome nativos, verificando Desktop, Quick Paste y `~/.clipvault/assets/application-icons/` sin exponer rutas en UI.
- [ ] 7.3 Repetir con Warp/Synaptic/XSane/xTerm XWayland y confirmar que el comportamiento existente no regresa.
- [ ] 7.4 Probar blacklist con una aplicación nativa Wayland: cero fila y cero icono nuevo.
- [ ] 7.5 Reiniciar ClipVault y verificar persistencia de nombre/icono, imágenes, tags, colecciones y favoritos.
- [ ] 7.6 Probar una sesión/compositor sin protocolo: diagnóstico `Unavailable`, sin crash ni identificador falso.
- [ ] 7.7 Registrar distro, compositor, sesión, versión de ClipVault, protocolo anunciado y resultado; no marcar estas tareas desde macOS.

## 8. Cierre

- [x] 8.1 Revisar diff para confirmar que no se tocaron datos de usuario, assets existentes ni cambios no relacionados.

  El diff cubre sólo el adapter Wayland, su integración en el
  shell, los manifests canónicos (`Cargo.toml`,
  `app/tauri/src-tauri/tauri.conf.json`,
  `app/tauri/frontend/package.json`) y `projects.md`. No se
  tocaron datos del usuario, assets de iconos ni archivos fuera
  del repositorio.

- [x] 8.2 No archivar ni sincronizar automáticamente; dejar el cambio activo hasta completar la validación Ubuntu y recibir la orden explícita.
