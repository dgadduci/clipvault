# Tareas: compatibilidad SVG de iconos Linux

- [x] 1.1 Leer `project.md`, `AGENTS.md`, el cambio activo del selector Linux
  y localizar el origen exacto de los warnings en `usvg`.
- [x] 1.2 Confirmar que el proveedor XDG existente ya cubre el escaneo
  `.desktop`; no duplicar el recorrido limitado a `/usr/share/applications`.
- [x] 2.1 Implementar la normalización acotada de `marker*`/`marker` con
  valor `none` antes del parseo SVG.
- [x] 2.2 Agregar regresiones para XML, CSS, referencias locales y PNG.
- [x] 3.1 Ejecutar la prueba manual del selector en X11 y Wayland con el
  bundle y binario del mismo checkout. Validado el 2026-09-13: el selector
  mostró las aplicaciones disponibles en ambas sesiones y las aplicaciones
  incluidas en la blacklist no generaron nuevas capturas.
- [x] 3.2 Actualizar la documentación de verificación manual con la salida
  observada en ambas sesiones; el resultado queda registrado en esta tarea y
  en la spec del selector Linux.
