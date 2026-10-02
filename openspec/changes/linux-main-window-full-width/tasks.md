# Tareas: ventana principal Linux a ancho completo

## 1. Relevamiento y diagnóstico

- [x] 1.1 Revisar el tamaño inicial de Tauri, el cálculo de `work_area`, el
  factor de escala y el ancho CSS del workspace; documentar que el código ya
  intenta llenar el área pero Linux muestra un resultado distinto.
- [x] 1.2 Añadir medición local de los límites, la escala, el ancho objetivo y
  el ancho efectivo que informa Tauri en la primera configuración del
  compositor. La verificación manual por backend queda en 3.5.

## 2. Corrección de geometría

- [x] 2.1 Corregir el cálculo o la aplicación del ancho inicial de `main` para
  llenar el área horizontal disponible en Linux.
- [x] 2.2 Usar límites del monitor como fallback cuando `work_area` no sea
  válido, y conservar el fallback configurado si no hay geometría disponible.
- [x] 2.3 Ajustar las restricciones de ancho de Linux para que el mínimo no
  impida ocupar pantallas más estrechas; conservar el reflow usable del
  contenido.
- [x] 2.4 Asegurar que el tamaño correcto se aplique antes de mostrar el primer
  layout estable, sin loops que sobrescriban cambios manuales posteriores.
- [x] 2.5 Mantener la altura compacta, la posición vertical inicial, macOS y
  QuickVault sin cambios.

## 3. Regresiones y verificación

- [x] 3.1 Añadir pruebas puras de conversión de geometría para escala,
  work-area, fallback y anchos inferiores al mínimo.
- [x] 3.2 Añadir una regresión que compare el ancho reportado por el compositor
  con el ancho objetivo y cubra cuándo corresponde corregirlo.
- [x] 3.3 Confirmar que el workspace usa el ancho del WebView y no genera
  overflow horizontal; añadir un test frontend sólo si se modifica esa capa.
- [x] 3.4 Ejecutar los checks afectados y las regresiones frontend de
  drag-and-drop; registrar aparte los fallos del conjunto Rust completo si no
  pertenecen a este cambio. `cargo check` y las pruebas específicas de
  geometría pasaron; las cuatro suites DnD pasaron (85 casos). El conjunto Rust
  completo tuvo 14 fallos en pruebas de captura/probe ajenas al layout.
- [ ] 3.5 Verificar manualmente el ancho completo en Ubuntu GNOME Wayland,
  KDE Plasma Wayland y Linux/X11; cubrir GNOME/KDE sobre X11 cuando estén
  disponibles y probar una escala HiDPI.
- [ ] 3.6 Confirmar manualmente que macOS no cambia, QuickVault conserva su
  tamaño y el usuario puede redimensionar la ventana después del arranque.
- [x] 3.7 Ejecutar `openspec validate linux-main-window-full-width --strict`
  y revisar `git diff --check`.
