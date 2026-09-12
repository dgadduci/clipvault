# Tareas: iconos Linux en variantes de empaquetado

Codex preparó esta arquitectura a partir del resultado manual X11/Wayland.
MiniMax implementa sólo las tareas pendientes tras aprobación; no debe
commitear, hacer push ni archivar este cambio automáticamente.

## 1. Baseline y contrato

- [x] 1.1 Leer `project.md`, `AGENTS.md`, el cambio activo
  `linux-wayland-desktop-file-icons` y el estado del repositorio.
- [x] 1.2 Registrar el resultado manual: captura reconocida en X11 y Wayland,
  mayoría de iconos visibles, Firefox y xterm sin icono.
- [x] 1.3 Auditar entradas locales equivalentes: Firefox Snap con
  `firefox_firefox.desktop` + icono absoluto bajo `/snap`, y xterm con
  `debian-xterm.desktop` + `Exec=xterm` + `Icon=mini.xterm`.
- [x] 1.4 Confirmar que el bridge de assets y la UI ya tienen fallback y que no
  se requiere un DTO o componente nuevo.

## 2. Matching seguro de entradas

- [x] 2.1 Agregar al modelo interno de `DesktopEntry` la extracción segura del
  basename del primer token de `Exec=`; rechazar shell, códigos de campo y
  tokens ambiguos sin ejecutar nada.
- [x] 2.2 Agregar `MatchStrategy::ExecBasename` con string estable
  `exec_basename` y conservar la precedencia existente.
- [x] 2.3 Resolver `xterm` contra `debian-xterm.desktop` por igualdad exacta
  del alias ejecutable, sin aceptar prefijos o substrings.
- [x] 2.4 Mantener Desktop File IDs `.desktop` estrictos y `source_app`
  inmutable.
- [x] 2.5 Cubrir precedencia entre WM_CLASS, filename stem y Exec, además de
  raíces XDG duplicadas.

## 3. Raíces de iconos de paquetes

- [x] 3.1 Extender el allowlist local del provider/adaptador Linux para raíces
  de payload de paquetes detectadas explícitamente, incluyendo Snap y el
  equivalente Flatpak disponible, sin permitir `/`, `/tmp` o el home completo.
- [x] 3.2 Reutilizar canonicalización, validación PNG/SVG, rasterización y
  escritura atómica existentes para `Icon=/snap/...`.
- [x] 3.3 Rechazar symlinks fuera de raíz, formatos inválidos y rutas ausentes
  sin borrar ni reemplazar assets existentes.
- [x] 3.4 Mantener `IconFailureKind::OutOfRoots` y no exponer rutas ni bytes en
  diagnósticos.

## 4. Regresiones e integración

- [x] 4.1 Test unitario Firefox Snap: ID exacto, ruta absoluta permitida,
  nombre, icono y estrategia `desktop_file_id`.
- [x] 4.2 Test unitario xterm: alias `ExecBasename`, `mini.xterm` desde PNG o
  SVG de tema y persistencia controlada.
- [x] 4.3 Test de rechazo para `/tmp`, symlink escapado, `xterm-extra` y
  `other-firefox.desktop`.
- [x] 4.4 Confirmar PrivacyGate/blacklist antes de lookup y asset I/O; no tocar
  contenido, hash, imágenes, tags, colecciones, favoritos ni assets previos.
- [x] 4.5 Ejecutar tests Rust relevantes, `cargo fmt`, clippy relevante,
  `npm run check`, `npm run build`, `npm test`, OpenSpec estricto y las
  regresiones frontend de drag-and-drop.

## 5. Verificación manual

- [ ] 5.1 X11: capturar desde Firefox y xterm, confirmar fuente, nombre e icono
  en Desktop y Quick Paste.
- [ ] 5.2 Wayland/XWayland: repetir con los mismos procesos y registrar sólo
  aplicación, identificador, estrategia y resultado.
- [ ] 5.3 Reiniciar ClipVault y verificar persistencia del icono sin limpiar
  `~/.clipvault/assets`.
- [ ] 5.4 Probar una ruta de icono fuera de allowlist y confirmar fallback sin
  asset nuevo.

## 6. Versionado y cierre

- [ ] 6.1 Incrementar patch sólo si la implementación funcional se completa;
  no incrementar durante la preparación de esta propuesta.
- [x] 6.2 Revisar diff, secretos, assets del usuario y archivos generados.
- [x] 6.3 Marcar tareas verificadas, validar OpenSpec y preparar handoff sin
  commit/push automático.

## Resultado manual parcial

Verificación realizada en Linux: Firefox Snap fue detectado con nombre e
icono. El icono de xterm no fue detectado y conserva el fallback; esta
limitación queda documentada y no se considera resuelta por este cambio.

Las tareas 5.1–5.4 permanecen pendientes porque el criterio manual del cambio
incluye también xterm, persistencia tras reinicio y el rechazo interactivo de
rutas fuera del allowlist. La tarea 6.1 también permanece pendiente porque no
se incrementa el patch mientras el cambio completo no esté funcionalmente
cerrado.
