# Seguridad de hilos X11 al abrir Quick Paste

## Problema

En una sesión Linux X11, al pulsar el atajo global `Ctrl+Shift+V` para abrir
ClipPaste/Quick Paste el proceso puede abortar con:

```text
[xcb] Unknown sequence number while processing queue
[xcb] Most likely this is a multi-threaded client and XInitThreads has not been called
[xcb] Aborting, sorry about that.
clipvault-app: ../../src/xcb_io.c:278: poll_for_event:
La declaración `!xcb_xlib_threads_sequence_lost' no se cumple.
```

La ruta actual combina un backend de atajo global que mantiene un hilo de
eventos basado en Xlib con el runtime gráfico de Tauri/GTK/WebView y los
adapters Linux que usan X11. El proceso no está habilitando el soporte de
concurrencia de Xlib antes de que esos componentes puedan compartir el
proceso. El resultado es un aborto nativo, no un error recuperable de la
apertura de la ventana.

## Por qué

`XInitThreads` debe ejecutarse una sola vez y antes de la primera llamada Xlib
del proceso. Inicializarlo dentro del callback del atajo, después de arrancar
Tauri o dentro del constructor tardío del manager no corrige la carrera: para
ese momento otro componente ya puede haber usado Xlib.

La corrección debe vivir en la frontera Linux de plataforma y en el orden de
arranque del shell. El core no debe conocer Xlib, el frontend no debe invocar
APIs nativas y el callback del atajo debe seguir entregando sólo una señal de
activación al flujo existente de Quick Paste.

## Qué cambia

- Agregar un preflight/adaptador Linux pequeño que habilite el soporte de
  hilos de Xlib mediante `XInitThreads` antes de iniciar el runtime gráfico.
- Ejecutar ese preflight antes de `tauri::Builder::default()` y antes de
  construir el manager global de atajos.
- Hacer que el resultado sea idempotente y tipado. Si Xlib no puede cargarse o
  `XInitThreads` falla, ClipVault debe continuar vivo sin el atajo global y
  mostrar el estado como indisponible mediante los contratos existentes, sin
  abortar ni simular éxito.
- Conservar la entrega del evento `clipvault://quick-search` con payload
  metadata-only (`null`/`()`), dejando que la UI ejecute la secuencia existente
  de captura de aplicación activa, centering, show, focus y apertura.
- Añadir pruebas puras para el preflight, pruebas de orden de inicialización y
  una prueba manual real en X11 que cubra aperturas repetidas.

## Fuera de alcance

- Reemplazar `global-hotkey` por otro backend o implementar un protocolo nuevo
  de atajos nativos Wayland.
- Reescribir los adapters X11 de aplicación activa o pegado, cambiar XTEST,
  cambiar la detección de XWayland o alterar el flujo de clipboard.
- Cambiar la ventana, el layout, los iconos, previews, assets persistidos,
  cards, búsqueda, drag and drop o edición de títulos.
- Agregar cuentas, red, telemetría, servicios externos o almacenamiento nuevo.

## Capacidades y compatibilidad

La inicialización debe estar compilada sólo para el camino Linux que puede
usar el backend Xlib del atajo global. En macOS, Windows y builds sin ese
backend no se debe introducir una llamada X11. En una sesión Wayland, el
preflight no debe presentarse como soporte de atajos Wayland nativos; si la
sesión expone XWayland y el backend Xlib existente se usa, comparte la misma
regla de inicialización segura.

El fallo del preflight es degradable: la base local y el resto de capacidades
que sí funcionen permanecen disponibles. Los logs y diagnósticos pueden
contener únicamente el backend y la causa técnica, nunca contenido del
portapapeles, secretos, hashes, rutas absolutas, bytes de assets ni títulos de
ventana.

## Criterio de aceptación

En un host Linux X11 real, una build nueva puede abrir Quick Paste con
`Ctrl+Shift+V` repetidamente sin emitir el aborto de XCB ni terminar el
proceso. La ventana recibe foco, mantiene el target de la aplicación activa y
el flujo de selección/pegado existente sigue funcionando. La verificación
debe ejecutarse con cualquier instancia anterior cerrada y el resultado debe
quedar en un commit antes de iniciar otro cambio.
