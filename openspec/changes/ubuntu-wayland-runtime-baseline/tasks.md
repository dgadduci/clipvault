# Tareas: baseline y runtime Ubuntu Wayland

## 1. Preparación y aislamiento

- [ ] 1.1 En Ubuntu, confirmar rama, árbol limpio, Rust 1.89, Node 20 y npm 10.
- [ ] 1.2 Crear o actualizar `fix/ubuntu-wayland-runtime-baseline` desde
  `origin/main`; no mezclar `fix/gnome-wayland-native-detection`.
- [ ] 1.3 Identificar por PID y detener sólo procesos previos de ClipVault,
  Vite y `cargo tauri dev` pertenecientes al workspace.
- [ ] 1.4 Registrar versión de Ubuntu, GNOME, tipo de sesión y ClipVault sin
  registrar variables de entorno completas.

## 2. Reproducción del baseline Ubuntu

- [ ] 2.1 Ejecutar `CARGO_BUILD_JOBS=1 cargo tauri dev` desde `app/tauri`, sin
  `GDK_BACKEND` ni features GNOME.
- [ ] 2.2 Verificar visualmente la ventana `main`; no usar `wmctrl` como prueba
  de ausencia de una superficie Wayland nativa.
- [ ] 2.3 Ocultar la ventana y verificar que Open ClipVault del tray la restaura.
- [ ] 2.4 Cerrar la aplicación, confirmar que no queda proceso del workspace y
  repetir el arranque una vez.
- [ ] 2.5 Si el baseline es visible, detener este cambio antes de tocar GNOME.

## 3. Instrumentación mínima si falla el baseline

- [ ] 3.1 Relevar la API Tauri/Wry instalada para consultas no mutantes de
  ventana y documentar el punto de ciclo de vida disponible.
- [ ] 3.2 Añadir `clipvault_window_lifecycle`, desactivada por defecto y
  habilitable sólo mediante un booleano de diagnóstico documentado.
- [ ] 3.3 Emitir etapas normalizadas para `setup`, monitor y eventos de `main`.
- [ ] 3.4 Probar la normalización y prohibir por test datos sensibles en logs.
- [ ] 3.5 Repetir en Ubuntu y registrar la primera transición que no ocurre.

## 4. Corrección mínima guiada por evidencia

- [ ] 4.1 Documentar la causa observada en Ubuntu antes de modificar el shell.
- [ ] 4.2 Aplicar una corrección acotada; no agregar múltiples fallbacks de
  `show`, foco, tamaño o posición.
- [ ] 4.3 Mantener separado el arranque y la restauración desde tray.
- [ ] 4.4 Agregar la regresión automática proporcional.
- [ ] 4.5 Si hubo cambio funcional, incrementar una sola vez el patch canónico
  en manifests y `projects.md`.

## 5. Verificación y cierre

- [ ] 5.1 En Ubuntu GNOME Wayland verificar inicio, ocultar, tray y reinicio.
- [ ] 5.2 En Ubuntu X11 ejecutar un smoke test de arranque y restauración.
- [ ] 5.3 Ejecutar fmt, tests Rust relevantes y frontend check/build/test con
  Node 20.
- [ ] 5.4 Verificar sin regresiones de imágenes, SQLite, organización, Quick
  Paste, búsqueda y drag and drop.
- [ ] 5.5 Validar OpenSpec, revisar `git diff --check` y no archivar/sincronizar
  hasta que Ubuntu confirme una ventana visible.
