# Diseño: baseline del runtime Ubuntu Wayland

## Baseline aislado

La investigación comienza desde una rama creada sobre `main`.
Antes de iniciar se confirma árbol limpio y se detienen por PID explícito sólo
instancias anteriores de ClipVault, Vite y `cargo tauri dev` del workspace.

La primera ejecución es:

```bash
CARGO_BUILD_JOBS=1 cargo tauri dev
```

No se fuerza `GDK_BACKEND` ni features GNOME. La aparición se evalúa
visualmente: `wmctrl` es una herramienta X11/XWayland y no demuestra la
ausencia de una superficie Wayland nativa.

## Diagnóstico seguro

Si el baseline falla, se habilita una única traza
`clipvault_window_lifecycle`, desactivada por defecto.
Informa sólo etapas normalizadas de la ventana `main`: creación configurada,
entrada/salida de `setup`, disponibilidad booleana de monitor y eventos
`focused`, `resized`, `moved`, `destroyed` y `close_requested`.

La traza nunca incluye contenido de clipboard, títulos, PID, rutas absolutas,
hashes, `asset_ref`, `source_app`, identificadores externos de ventana ni
variables de entorno. Se activa con un booleano documentado.

## Política de corrección

No se agregan `show`, `unminimize`, foco, tamaño, posición ni bridge Svelte
por especulación. La traza Ubuntu decide la corrección:

1. sin ventana creada: revisar configuración o builder;
2. ventana creada pero oculta: una sola solicitud idempotente en el punto
   probado;
3. superficie no mapeada: conservar el baseline y documentar una alternativa
   respaldada por Tauri/GTK;
4. sin `primary_monitor` durante `setup`: no mutar la ventana antes de tener
   evidencia de que esa operación es requerida.

La restauración desde el tray queda separada del arranque inicial. El cambio
preserva macOS, X11, Quick Paste, captura, assets, imágenes, tags, colecciones,
favoritos, búsqueda y drag and drop.

### Causa observada en Ubuntu

La ventana inicial sí se muestra. Al recibir `CloseRequested`, el shell la
oculta y cancela el cierre, por lo que el proceso debe permanecer activo para
el tray. Sin embargo, Tauri 2.11.5 elimina el icono cuando se descarta la última
instancia de `TrayIcon`. El shell construía ese valor como temporal y también
descartaba su `TauriTrayController` al terminar `setup`; después de ocultar la
ventana no quedaba una superficie para restaurarla ni una acción de tray para
salir.

La corrección conserva el `TrayIcon` dentro de `TauriTrayController` y mantiene
el controlador gestionado por la aplicación durante toda su vida. No cambia el
orden de creación, la visibilidad inicial, foco, tamaño ni posición de la
ventana; la restauración existente desde `Open ClipVault` sigue siendo el único
punto que llama a `show` y `set_focus`.

La comprobación posterior confirmó que GNOME conserva el indicador registrado
y activo después de ocultar la ventana. La acción `Open ClipVault` seguía sin
restaurarla porque `on_menu_event` la enviaba al adaptador de tray del core,
que se construye como stub y no es el controlador Tauri que posee la ventana.
El handler debe delegar en el `TauriTrayController` gestionado por la
aplicación. De este modo la acción de menú usa el mismo adaptador Tauri que
implementa `show`/`set_focus`, sin introducir lógica de ventana en el core.

### Regresión reabierta: ausencia de desktop con monitor primario no informado

La prueba manual posterior en GNOME Wayland vuelve a iniciar el binario sin
mostrar el desktop y registra:

```text
WARN no primary monitor reported; keeping conf defaults
```

La observación invalida la marca previa de arranque visible, pero **no**
autoriza a deducir que `primary_monitor() == None` es la causa del toplevel
ausente. En Tauri 2.11.5 la opción `visible` de una ventana tiene valor por
defecto `true`, y el retorno temprano de `resize_main_window_to_monitor` sólo
omite el cálculo y las mutaciones opcionales de geometría. El warning sí es un
correlato estable que permite activar una traza local segura.

La traza confirmó `configured`, `setup_entered`, `monitor_available = false` y
`layout_completed` con `visible = true`, pero no `state_built`. Por tanto, el
monitor no era la causa y la ventana ya estaba configurada como visible: el
bloqueo estaba dentro de `build_state`, antes de que Tauri llegara a `Ready`.

El adaptador nativo Wayland abre un socket y espera su handshake de registro
durante `build_state`. Aunque el receptor de ese handshake tiene un plazo de
500 ms, cuando éste vence invoca `IoThread::shutdown()`, que hacía `join()` de
un hilo detenido en `UnixStream::read_exact()`. Sin un timeout de lectura, el
hilo nunca observa el flag de cierre y bloquea todo el `setup`.

La corrección instala un timeout de lectura de 250 ms sólo durante el
handshake. Un timeout se traduce en el estado `Unavailable` ya soportado y el
hilo termina antes del `join`; al enlazar un protocolo compatible, el socket
vuelve al modo de lectura normal para el loop de despacho. Así el adaptador
puede degradarse a sus alternativas existentes sin cambiar la semántica de
detección de aplicación activa. No se agrega `show`, foco, tamaño, posición ni
otra mutación de presentación.

La traza no forma parte del core ni del frontend: es un adaptador de shell
Tauri, está desactivada por defecto y no afecta Quick Paste. El timeout queda
detrás del adaptador de plataforma. X11 continúa usando su layout de inicio
existente; este trabajo no vuelve a introducir posicionamiento absoluto para
Wayland.

## Validación

La validación manual ocurre en Ubuntu: primero se obtiene la traza del
arranque invisible; después de la corrección demostrada se confirma primer
inicio visible, ocultar, restaurar desde tray, cierre sin proceso residual y
reinicio. Después se hace un smoke test X11. Una corrección funcional
incrementa una sola vez el patch; este plan no cambia versión.
