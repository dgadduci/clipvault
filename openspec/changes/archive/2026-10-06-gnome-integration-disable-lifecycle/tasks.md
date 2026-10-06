# Tareas

## 1. Apagado seguro del listener

- [x] 1.1 Hacer que la lectura de un peer GNOME persistente pueda observar la
  solicitud de cierre sin esperar indefinidamente.
- [x] 1.2 Mantener el apagado y el `join` del listener acotados e idempotentes.
- [x] 1.3 Agregar una prueba representativa con un peer conectado que deja de
  enviar datos y verificar que el hilo termina al solicitar el cierre.

## 2. Desactivación GNOME

- [x] 2.1 Ampliar el controller testeable con una operación `disable` que invoque
  únicamente `gnome-extensions disable` para el UUID fijo de ClipVault.
- [x] 2.2 Ejecutar la operación fuera del hilo de interfaz y aplicar un tiempo
  límite que termine el proceso hijo al vencer.
- [x] 2.3 Ordenar la desactivación para descargar primero la extensión de GNOME,
  detener el listener y retirar los archivos locales después del éxito.
- [x] 2.4 En errores del ejecutable o de GNOME, conservar el consentimiento y la
  instalación previos y devolver un error recuperable.
- [x] 2.5 Asegurar que el estado `disabled` y la acción de reactivación sigan
  funcionando después de reiniciar ClipVault.

## 3. Interfaz y verificación

- [x] 3.1 Asegurar que la interfaz termina el estado ocupado en éxito y error, y
  localizar los mensajes modificados en los cinco idiomas.
- [x] 3.2 Ejecutar las pruebas relevantes de controller, listener y frontend;
  validar OpenSpec y revisar el diff.
- [x] 3.3 Repetir la prueba manual en Ubuntu GNOME Wayland: desactivar una
  integración conectada, confirmar que ClipVault sigue respondiendo, cerrar y
  volver a abrir ClipVault, verificar `disabled` y habilitarla nuevamente.
  **Prueba manual aprobada por el usuario en Ubuntu GNOME Wayland (2026-10-06).**
- [x] 3.4 Conservar como aprobada la prueba manual KDE ya reportada; repetirla
  solo si los cambios terminan afectando KDE.
