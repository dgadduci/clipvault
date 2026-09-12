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

- [x] 5.1 Verificar manualmente Firefox en la sesión Linux probada: la fuente,
  el nombre y el icono fueron detectados. xterm conserva el fallback y se
  acepta explícitamente como limitación.
- [x] 5.2 Cerrar la verificación manual X11/Wayland como decisión de producto:
  Firefox queda aceptado; no se abre trabajo adicional para xterm.
- [x] 5.3 Cerrar por aceptación explícita la verificación adicional de
  persistencia para esta entrega; los assets existentes se preservan y la
  cobertura automatizada permanece vigente.
- [x] 5.4 Cerrar por aceptación explícita la prueba manual adicional de una
  ruta fuera del allowlist; la regresión automatizada confirma fallback sin
  asset nuevo.

## 6. Versionado y cierre

- [x] 6.1 Decidir no incrementar el patch: Firefox queda aceptado y xterm se
  cierra como limitación explícita de producto.
- [x] 6.2 Revisar diff, secretos, assets del usuario y archivos generados.
- [x] 6.3 Marcar tareas verificadas, validar OpenSpec y preparar handoff sin
  commit/push automático.

## Resultado manual parcial

Verificación realizada en Linux: Firefox Snap fue detectado con nombre e
icono. El icono de xterm no fue detectado y conserva el fallback; esta
limitación queda documentada y no se considera resuelta por este cambio.

Las tareas 5.1–5.4 quedan cerradas por decisión explícita del usuario: Firefox
fue aceptado manualmente, xterm permanece como limitación conocida, y las
verificaciones adicionales quedan cubiertas por la aceptación de alcance o
por las regresiones automatizadas. No se incrementa el patch.
