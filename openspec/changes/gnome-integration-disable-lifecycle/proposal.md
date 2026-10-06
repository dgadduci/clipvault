# Propuesta: desactivación GNOME sin bloquear ClipVault

## Por qué

En la prueba manual de Ubuntu GNOME Wayland, la acción **Deshabilitar** dejó
ClipVault sin responder hasta que hubo que forzar su cierre. Al volver a
abrirlo, el consentimiento aparecía desactivado y permitía habilitar la
integración otra vez, que es el estado persistido esperado. La prueba manual
de KDE Plasma fue aprobada.

El flujo GNOME elimina los archivos de la extensión y después espera a que
termine el hilo del listener. La extensión puede mantener una conexión Unix
abierta mientras GNOME Shell la tenga cargada; el listener procesa esa
conexión con una lectura bloqueante. El código actual no solicita primero a
GNOME que desactive la extensión y el cierre del listener puede esperar sin
límite.

## Cambios

- Solicitar a GNOME que desactive la extensión mediante `gnome-extensions`
  y el UUID fijo de ClipVault antes de retirar sus archivos locales.
- Ejecutar y esperar esa operación fuera del hilo de interfaz, con un límite
  de tiempo y cancelación del proceso si vence.
- Hacer que el listener pueda observar la solicitud de cierre aun cuando esté
  atendiendo una conexión persistente, y que su apagado no espere
  indefinidamente.
- Mantener disponible el estado **Deshabilitada** y la acción para volver a
  habilitar la integración.
- Si GNOME no puede desactivar la extensión, devolver el control a la interfaz,
  mantener recuperable el estado anterior y presentar un error localizado.
- Conservar la privacidad, la operación local, el consentimiento explícito y
  el adaptador testeable de escritorio.

## Impacto

- Ciclo de vida GNOME en `clipvault-platform` y el adaptador Tauri.
- El flujo de configuración y desactivación de Integraciones de escritorio.
- No cambia la integración KDE ni el comportamiento de captura del
  portapapeles.
- No agrega dependencias ni operaciones privilegiadas.
