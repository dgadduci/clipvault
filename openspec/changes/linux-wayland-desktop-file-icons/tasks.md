# Tareas: iconos Wayland mediante Desktop File IDs

Codex preparó esta arquitectura. MiniMax implementa sólo las tareas pendientes tras aprobación; no se archiva, commitea ni publica este cambio automáticamente.

## 1. Baseline y contrato

- [x] 1.1 Leer `project.md`, `AGENTS.md`, los cambios activos GNOME/Wayland y el estado limpio posterior al commit `1e043dd`.
- [x] 1.2 Registrar la evidencia manual: GNOME Wayland llega a `identified` con `window:6`; el transporte funciona pero ese id no promete un icono.
- [x] 1.3 Auditar la extensión, el pipeline `source_app` → PrivacyGate → metadata, el provider Linux y el bridge de iconos sin leer contenido de capturas.
- [x] 1.4 Confirmar las referencias GNOME y freedesktop: `Shell.App` puede ser window-backed y los Desktop File IDs incluyen `.desktop`.
- [x] 1.5 Registrar el baseline de tests relevante y los fallos antes de modificar código; no se modificó `~/.clipvault`. `cargo fmt`, OpenSpec, `npm run check`, `npm test` y `npm run build` pasan. Los tests Rust de plataforma/app con la combinación GNOME quedan documentados como fallidos en la revalidación de `gnome-wayland-integration/tasks.md` (sockets Unix `EPERM`, metadata/bootstrap y un lint preexistente).

## 2. Fuente GNOME y contrato de identidad

- [x] 2.1 Ajustar `_resolveFocusedAppId()` para rechazar de forma segura `is_window_backed() == true` y el prefijo `window:` sin leer título, PID, ruta ni icono.
- [x] 2.2 Conservar el envelope IPC actual `{ v, kind, app_id }`; representar ausencia con el mecanismo ya definido y no añadir campos de metadata.
- [x] 2.3 Mantener Desktop File IDs no vacíos intactos, incluido `.desktop`, y comprobar compatibilidad en GNOME Shell 42.9 y las versiones declaradas en `metadata.json`.
- [x] 2.4 Agregar regresiones de extensión para app asociada, app window-backed, guard de prefijo y ausencia de nuevos campos sensibles.

## 3. Provider Linux y persistencia

- [x] 3.1 Modelar `DesktopFileId` como estrategia de match separada y resolver el nombre de archivo `.desktop` completo sin transformar `source_app`.
- [x] 3.2 Conservar el orden y la semántica de los matchers `StartupWMClass`, `X-GNOME-WMClass` y filename stem para identificadores X11/XWayland sin `.desktop`.
- [x] 3.3 Respetar la precedencia de raíces XDG ante Desktop File IDs duplicados; el desempate lexicográfico sólo aplica dentro de una misma raíz.
- [x] 3.4 Tratar `window:*` como no resoluble en el provider y excluirlo de reintentos de backfill, sin reescribir filas o assets existentes.
- [x] 3.5 Reutilizar la resolución `Icon=`, SVG/PNG aprobada, validación, escritura atómica y namespace `application-icons/`; no crear bridge, formato ni dependencia nuevos.
- [x] 3.6 Mantener PrivacyGate antes de lookup y asset I/O; confirmar que blacklist no genera filas, nombres ni iconos.
- [x] 3.7 Exponer sólo la estrategia de match estable necesaria para diagnóstico, sin IDs, rutas, contenido de `.desktop` ni bytes.

## 4. Integración y regresiones automatizadas

- [x] 4.1 Cubrir Desktop File IDs representativos (`firefox.desktop`, `org.gnome.Terminal.desktop`) con nombre e icono locales controlados.
- [x] 4.2 Cubrir X11/XWayland existente (`firefox`, `dev.warp.Warp`) y verificar que la nueva estrategia no cambia su prioridad ni asset bridge.
- [x] 4.3 Cubrir `window:6`: no provider lookup, no asset I/O, no candidato de backfill y fallback de captura no bloqueante.
- [x] 4.4 Cubrir precedencia XDG para ID duplicado, icono ausente, PNG inválido, SVG permitido/rechazado y preservación de un icono previo válido.
- [x] 4.5 Cubrir captura permitida, blacklist y reinicio/backfill sin modificar contenido, hashes, imágenes, tags, colecciones ni favoritos.
- [x] 4.6 Ejecutar `cargo fmt --all -- --check`, clippy relevante con `-D warnings`, tests Rust relevantes, `npm run check`, `npm run build`, `npm test`, validación OpenSpec estricta y regresiones frontend de drag-and-drop.

## 5. Verificación manual Ubuntu GNOME Wayland

- [ ] 5.1 Con Firefox nativo, capturar y confirmar Desktop/Quick Paste: `source_app` Desktop File ID, nombre, icono y persistencia tras reiniciar ClipVault. *(pendiente: el host actual es Linux/macOS dev, no Ubuntu GNOME Wayland — se omite en este entorno).*
- [ ] 5.2 Repetir con Chrome, Terminal GNOME y Warp; registrar sólo aplicación, tipo de id, estado y resultado. *(pendiente: idem 5.1).*
- [ ] 5.3 Enfocar una app window-backed que reporte `window:*` y confirmar ausencia segura, sin asset ni atribución inventada. *(pendiente: idem 5.1).*
- [ ] 5.4 Probar blacklist sobre una app con Desktop File ID y confirmar que no se persiste la captura ni se crea icono. *(pendiente: idem 5.1).*
- [ ] 5.5 Ejecutar el smoke test X11/XWayland para confirmar que sus iconos siguen visibles. *(pendiente: idem 5.1).*

## 6. Versión y cierre

- [x] 6.1 Incrementar una única vez el patch canónico `0.0.13` → `0.0.14` cuando se implemente el cambio funcional y sincronizar manifests, lockfiles necesarios y `projects.md`.
- [x] 6.2 Revisar el diff, confirmar que no hay secretos, assets del usuario modificados ni archivos generados innecesarios.
- [x] 6.3 No archivar, commitear ni hacer push sin orden explícita del usuario.
