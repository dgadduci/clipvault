# Tareas: baseline y runtime Ubuntu Wayland

## 1. Preparación y aislamiento

- [x] 1.1 En Ubuntu, confirmar rama, árbol limpio, Rust 1.89, Node 20 y npm 10.
- [x] 1.2 Crear o actualizar `fix/ubuntu-wayland-runtime-baseline` desde
  `origin/main`; no mezclar `fix/gnome-wayland-native-detection`.
- [x] 1.3 Identificar por PID y detener sólo procesos previos de ClipVault,
  Vite y `cargo tauri dev` pertenecientes al workspace.
- [x] 1.4 Registrar versión de Ubuntu, GNOME, tipo de sesión y ClipVault sin
  registrar variables de entorno completas.

## 2. Reproducción del baseline Ubuntu

- [x] 2.1 Ejecutar `CARGO_BUILD_JOBS=1 cargo tauri dev` desde `app/tauri`, sin
  `GDK_BACKEND` ni features GNOME.
- [ ] 2.2 Verificar visualmente la ventana `main`; no usar `wmctrl` como prueba
  de ausencia de una superficie Wayland nativa.
  - Regresión reabierta el 2026-09-13: el binario inicia en GNOME Wayland y
    registra `no primary monitor reported; keeping conf defaults`, pero no
    aparece el desktop. La marca previa no representa el estado actual.
- [x] 2.3 Ocultar la ventana y verificar que Open ClipVault del tray la restaura.
- [x] 2.4 Cerrar la aplicación, confirmar que no queda proceso del workspace y
  repetir el arranque una vez.
- [x] 2.5 Si el baseline es visible, detener este cambio antes de tocar GNOME.

## 3. Instrumentación mínima si falla el baseline

- [x] 3.1 Relevar la API Tauri/Wry instalada para consultas no mutantes de
  ventana y documentar el punto de ciclo de vida disponible.
  - Tauri 2.11.5 expone `is_visible`; `primary_monitor()` puede devolver
    `None`, mientras que la configuración de ventana usa `visible = true` por
    defecto. Por tanto, el warning de monitor no es evidencia suficiente de
    que la ventana no se haya creado o mapeado.
- [x] 3.2 Añadir `clipvault_window_lifecycle`, desactivada por defecto y
  habilitable sólo mediante un booleano de diagnóstico documentado.
- [x] 3.3 Emitir etapas normalizadas para `configured`, `setup`, monitor,
  `runtime_ready` y eventos de `main`, más `main_present` y `visible` cuando
  sus consultas no mutantes respondan.
- [x] 3.4 Probar la normalización y prohibir por test datos sensibles en logs.
- [x] 3.5 Repetir en Ubuntu y registrar la primera transición que no ocurre.
  - La traza llegó a `layout_completed` con `visible = true`, pero no a
    `state_built`; el bloqueo estaba dentro de `build_state`, no en el monitor.

## 4. Corrección mínima guiada por evidencia

- [x] 4.1 Documentar la causa observada en Ubuntu antes de modificar el shell.
  - El handshake nativo Wayland podía dejar un `read_exact` bloqueado mientras
    `IoThread::shutdown` esperaba su `join`, reteniendo el `setup` de Tauri.
- [x] 4.2 Aplicar una corrección acotada, elegida desde la primera transición
  ausente; no agregar múltiples fallbacks de `show`, foco, tamaño o posición.
- [x] 4.3 Mantener separado el arranque y la restauración desde tray.
- [x] 4.4 Agregar la regresión automática proporcional a la corrección que
  resulte de la traza.
- [x] 4.5 Si hubo cambio funcional, incrementar una sola vez el patch canónico
  en manifests y `projects.md`.
  - Versión canónica incrementada de `0.0.14` a `0.0.15`.

## 5. Verificación y cierre

- [ ] 5.1 En Ubuntu GNOME Wayland verificar inicio, ocultar, tray y reinicio.
  - El inicio corregido llegó a `runtime_ready`, `moved`, `resized` y
    `focused` con `visible = true`; quedan por repetir ocultar, tray y reinicio.
- [ ] 5.2 En Ubuntu X11 ejecutar un smoke test de arranque y restauración.
- [x] 5.3 Ejecutar fmt, tests Rust relevantes y frontend check/build/test con
  Node 20.
- [x] 5.4 Verificar sin regresiones de imágenes, SQLite, organización, Quick
  Paste, búsqueda y drag and drop.
- [x] 5.5 Validar OpenSpec, revisar `git diff --check` y no archivar/sincronizar
  hasta que Ubuntu confirme una ventana visible.
  - `openspec validate ubuntu-wayland-runtime-baseline --strict --type change`
    pasó con OpenSpec 1.13.0; la prueba manual Wayland ya había confirmado una
    ventana visible antes de sincronizar.
