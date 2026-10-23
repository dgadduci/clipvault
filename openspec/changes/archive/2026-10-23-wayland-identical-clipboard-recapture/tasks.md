# Tareas

## OpenSpec

- [x] 1.1 Documentar la causa y la selección de fuente de revisión por
  transporte Wayland/X11.
- [x] 1.2 Añadir los escenarios de recaptura de texto e imagen después de una
  nueva copia idéntica y de estabilidad tras borrar.

## Revisión de portapapeles Linux

- [x] 2.1 Añadir un monitor metadata-only para `selection` en los protocolos
  Wayland data-control soportados, ignorando la notificación inicial.
- [x] 2.2 Resolver el origen de revisión junto con el transporte efectivo de
  `arboard`; usar XFixes en Wayland solo cuando data-control no se publique y
  `arboard` por ello use X11/XWayland.
- [x] 2.3 Mantener `UNKNOWN` ante fallos no verificables y reflejar el backend
  seleccionado correctamente en los diagnósticos.
- [x] 2.4 Documentar las dependencias directas opcionales Wayland y sus
  alternativas en este diseño.

## Regresiones

- [x] 3.1 Probar el tratamiento de eventos iniciales y posteriores del monitor
  Wayland, así como el selector entre data-control, XFixes y `UNKNOWN`.
- [x] 3.2 Probar que una nueva revisión recaptura el mismo texto eliminado,
  mientras una revisión estable no lo recrea.
- [x] 3.3 Añadir la misma regresión para una imagen, verificando una fila nueva
  y el uso normal del asset store temporal.
- [x] 3.4 Ejecutar tests Rust afectados, formato, validación estricta de
  OpenSpec y `git diff --check`; revisar que no se modificaron los cambios
  locales ajenos a esta corrección.
- [x] 3.5 Verificar manualmente en GNOME Wayland que volver a copiar una misma
  selección de texto o imagen después de borrarla crea una fila nueva, y que
  dejar esa selección sin cambios no recrea la fila.

La prueba manual en GNOME Wayland fue aprobada por el usuario.
