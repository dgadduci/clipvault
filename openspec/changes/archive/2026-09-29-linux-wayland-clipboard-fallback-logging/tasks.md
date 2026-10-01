# Tareas de implementación

El usuario autorizó directamente las correcciones el 2026-09-28.

## 1. Confirmar ciclo de vida

- [x] 1.1 Confirmar en `arboard 3.6.1` que una negociación Wayland no soportada
  emite el warning al crear el cliente y que ClipVault construye un cliente por
  operación.
- [x] 1.2 Registrar la verificación manual del 2026-09-28: macOS y Ubuntu/X11
  sin regresiones; Ubuntu/Wayland captura sin regresiones observables pero
  repite el warning de fallback.
- [x] 1.3 Diseñar la reutilización con un `parking_lot::Mutex` para mantener el
  contrato `Send`/`Sync`; descartar el cliente tras `ArboardError::Unknown` para
  permitir que la siguiente operación restablezca la conexión.

## 2. Reutilización y recuperación

- [x] 2.1 Mantener un cliente `arboard` Linux inicializado de forma perezosa y
  sincronizada, reutilizado entre sondeos y operaciones.
- [x] 2.2 Conservar el cliente durante el sondeo, mantener visibles los errores
  y permitir la recuperación que ofrece el backend sin recrear el wrapper ni
  volver a negociar data-control en cada sondeo.
- [x] 2.3 Conservar selección Wayland/X11, tokens de backend existentes,
  revisionado, errores tipados y adaptador macOS sin caché. Verificado con
  `cargo check -p clipvault-app`.

## 3. Regresiones

- [x] 3.1 Verificar que la ruta Wayland sin data-control no emita el warning en
  cada poll y conserve los campos de diagnóstico existentes. Verificado por el
  usuario el 2026-09-29.
- [x] 3.2 Verificar la ruta Wayland compatible con data-control, Linux X11 y
  macOS sin cambiar su backend ni su comportamiento. Verificado por el usuario
  el 2026-09-29.
- [x] 3.3 Verificar que los errores inesperados sigan visibles, que una
  desconexión permita recuperación y que los logs continúen sin contenido ni
  datos sensibles. Verificado por el usuario el 2026-09-29.

## 4. Verificación manual y cierre

- [x] 4.1 En Ubuntu GNOME Wayland, confirmar la ausencia del warning repetido
  durante sondeos sucesivos. Aprobado por el usuario el 2026-09-29: el warning
  de fallback ya no se repite.
- [x] 4.2 Repetir el control de regresión en Ubuntu/X11, Arch KDE Plasma
  Wayland y macOS; mantener el backend nativo de KDE cuando data-control esté
  disponible. El usuario aprobó las pruebas manuales en macOS y Ubuntu el
  2026-09-29, y la prueba de Arch KDE Wayland el 2026-09-29.
- [x] 4.3 Revisar diff, warnings, privacidad, assets y validar OpenSpec
  estrictamente antes de cerrar el cambio. Revisión completada el 2026-09-29:
  `git diff --check` y OpenSpec pasaron; no se modificaron assets persistidos
  ni se añadieron logs con contenido del portapapeles. El diseño quedó alineado
  con la instancia retenida por `with_arboard` y la recuperación verificada.
