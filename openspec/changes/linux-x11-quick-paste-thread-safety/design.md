# Diseño: seguridad de hilos X11 para Quick Paste

## Diagnóstico confirmado

El backend Linux de `global-hotkey` crea un hilo dedicado que abre Xlib y
procesa `XPending`/`XNextEvent` y los grabs del atajo. El callback del
dispatcher se ejecuta desde ese hilo. El proceso Tauri también contiene
componentes gráficos que usan Xlib/XCB, además de los adapters X11 basados en
`x11rb`. El callback no debe operar directamente una ventana nativa desde el
hilo del backend.

La aserción observada es coherente con usar Xlib desde más de un hilo sin
haber llamado `XInitThreads` antes de las primeras llamadas Xlib. `XInitThreads`
es la inicialización requerida para Xlib; no se sustituye con un lock local,
con `x11rb` ni con `XLockDisplay` aplicado sólo al adapter de ClipVault,
porque la concurrencia incluye librerías gráficas externas.

## Frontera de plataforma

El crate `clipvault-platform` debe exponer un helper/adaptador de inicialización
de proceso, compilado únicamente en Linux y detrás de la feature que habilita
el camino Xlib del atajo. El helper debe:

1. Cargar la tabla de símbolos Xlib con la dependencia directa y explícita
   necesaria para invocar `XInitThreads`. Aunque la misma biblioteca ya pueda
   aparecer transitivamente por Tauri o `global-hotkey`, la frontera de
   plataforma no debe depender de un import transitivo accidental.
2. Invocar `XInitThreads` antes de cualquier `XOpenDisplay` o uso de Xlib por
   el manager, sin abrir una ventana ni leer contenido del usuario.
3. Guardar el resultado en un estado process-wide de inicialización única
   (`OnceLock` o equivalente). Las llamadas posteriores deben devolver el
   mismo resultado sin repetir la inicialización.
4. Devolver un resultado tipado que distinga, como mínimo, éxito, biblioteca
   no disponible y rechazo/error de `XInitThreads`. El resultado no debe
   guardar punteros Xlib ni exponer detalles sensibles.

La dependencia nueva, si hace falta declararla directamente, queda justificada
porque el adapter invoca una API Xlib concreta y debe controlar su versión de
compilación. No se debe introducir una dependencia de red, un servicio externo
ni una segunda implementación de atajos.

## Orden de arranque

El shell debe llamar al helper Linux antes de `tauri::Builder::default()` y,
por tanto, antes de la inicialización del runtime gráfico. También debe quedar
antes de `build_state()` y de cualquier constructor de
`GlobalHotkeyManagerAdapter`.

El orden requerido es:

```text
main
  → preflight Xlib/XInitThreads (Linux + backend Xlib)
  → init tracing permitido sin X11
  → tauri::Builder / runtime gráfico
  → build_state
  → GlobalHotkeyManagerAdapter::new
  → registro de Ctrl+Shift+V
```

Si el preflight falla, el shell no debe devolver un error fatal sólo por esta
capacidad. Debe conservar el estado de inicialización fallido, evitar crear el
manager Xlib y usar el `NoopHotkeyManager`/resultado de indisponibilidad ya
existente. El diagnóstico debe distinguir una indisponibilidad técnica de un
conflicto de atajo o de un permiso de macOS. La matriz de capabilities no debe
afirmar que el atajo está operativo cuando el manager fue omitido.

El preflight puede ejecutarse en Linux antes de conocer el foco o abrir una
conexión de display: `XInitThreads` prepara Xlib a nivel de proceso. Si la
configuración no enlaza el backend Xlib, el código queda compilado fuera. En
Wayland, esta corrección no habilita atajos nativos; sólo protege el camino
XWayland/Xlib existente cuando éste participa.

## Flujo de activación

El evento global debe conservar el contrato existente:

- el callback del hilo de `global-hotkey` sólo publica la señal de activación
  metadata-only;
- no llama `show`, `set_focus`, `set_position` ni otras APIs nativas de ventana
  desde el hilo X11;
- el listener único del frontend recibe `clipvault://quick-search` y ejecuta
  la secuencia vigente: capturar la aplicación activa con timeout,
  centrar best-effort, mostrar, enfocar y emitir la apertura;
- no se agrega otro listener ni se transporta contenido, hashes, rutas,
  identificadores de ventana o bytes de imagen en el evento.

La corrección sólo modifica la precondición de seguridad del runtime; no
modifica la semántica de selección, copia, pegado, ocultamiento, foco ni
retorno a la aplicación anterior.

## Estrategias descartadas

- **Inicializar Xlib en `GlobalHotkeyManagerAdapter::new`:** demasiado tarde,
  porque Tauri/GTK puede haber realizado la primera llamada Xlib y la regla es
  process-wide.
- **Poner un mutex alrededor de los adapters X11:** no sincroniza llamadas de
  Xlib realizadas por GTK, Tao, WebView o el propio backend de terceros.
- **Cambiar todo el backend de atajos:** resolvería potencialmente más casos,
  pero amplía el alcance hacia Wayland y no es necesario para corregir el
  aborto reproducido.
- **Usar una inicialización de XCB como sustituto:** la aserción proviene de la
  coordinación Xlib/XCB; el requisito de Xlib es `XInitThreads` antes de usar
  Xlib.

## Pruebas y verificación

Las pruebas unitarias no deben abrir un display real ni ejecutar la
inicialización global del proceso. El helper debe separar la decisión/control
de flujo de la llamada nativa para poder inyectar un loader/call fake y cubrir:

- éxito de `XInitThreads`;
- retorno cero de `XInitThreads`;
- fallo al cargar Xlib;
- idempotencia y ausencia de segunda llamada;
- que el resultado fallido evita la construcción del manager Xlib.

Debe existir además una verificación del orden de bootstrap que falle si el
preflight se mueve después de `tauri::Builder` o `build_state`. Las pruebas
existentes del evento de Quick Paste deben conservar payload vacío, listener
único y activación idempotente.

La prueba manual Linux X11 debe cerrar cualquier instancia anterior, arrancar
la build del mismo checkout y pulsar `Ctrl+Shift+V` en frío y en aperturas
repetidas. Debe comprobar ausencia del mensaje XCB, proceso vivo, ventana
visible/enfocada, escritura en el campo de búsqueda, target activo correcto y
pegado exitoso. Una sesión Wayland sin backend Xlib nativo y macOS sólo se
validan para no regresión según los hosts disponibles; no se infiere una
verificación manual no ejecutada.
