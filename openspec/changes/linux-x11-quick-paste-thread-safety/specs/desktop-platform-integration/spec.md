# Integración segura del runtime X11

## ADDED Requirements

### Requirement: Inicializar Xlib antes de la concurrencia gráfica

La integración Linux que pueda crear el backend Xlib de atajos SHALL llamar a
`XInitThreads` una única vez, con resultado observable y tipado, antes de
`tauri::Builder::default()`, de cualquier constructor de
`GlobalHotkeyManagerAdapter` y de cualquier otra llamada Xlib iniciada por el
shell. La llamada SHALL vivir detrás de la frontera de plataforma y no en el
core Rust ni en el frontend.

#### Scenario: Arranque X11 con soporte de hilos disponible

- **GIVEN** una build Linux con el backend global de atajos habilitado
- **AND** Xlib puede cargarse y `XInitThreads` devuelve éxito
- **WHEN** ClipVault inicia una sesión X11
- **THEN** el runtime gráfico y el manager global se crean después del
  preflight
- **AND** el proceso puede compartir Xlib entre el hilo del atajo y el resto
  del runtime sin perder la secuencia XCB

#### Scenario: XWayland comparte la misma precondición

- **GIVEN** una sesión Linux Wayland que expone un camino XWayland/Xlib para
  el backend existente
- **WHEN** ese camino se habilita
- **THEN** se aplica la misma inicialización process-wide antes de usar Xlib
- **AND** esto no se presenta como soporte de atajos nativos Wayland

#### Scenario: Inicialización idempotente

- **GIVEN** el preflight ya devolvió un resultado
- **WHEN** otra parte del bootstrap consulta o solicita la inicialización
- **THEN** recibe el mismo resultado
- **AND** `XInitThreads` no se invoca una segunda vez

### Requirement: Fallo del preflight no aborta la aplicación

Si Xlib no puede cargarse o `XInitThreads` devuelve fallo, ClipVault SHALL
continuar con la base local y las demás capacidades disponibles, SHALL evitar
crear el manager Xlib de atajos y SHALL reportar la capacidad global como
indisponible mediante los contratos tipados existentes. El fallo SHALL quedar
limitado a esa capacidad y no SHALL terminar el proceso ni recomendar un
permiso que no corresponde.

#### Scenario: Xlib no disponible

- **WHEN** el loader no puede abrir Xlib durante el preflight
- **THEN** el shell continúa su arranque
- **AND** usa el fallback de hotkey no-op/indisponible
- **AND** el diagnóstico contiene sólo la categoría de backend y una causa
  técnica segura

#### Scenario: XInitThreads rechazado

- **WHEN** Xlib carga pero `XInitThreads` devuelve cero
- **THEN** el shell no crea el manager global Xlib
- **AND** Quick Paste sigue disponible desde sus rutas no globales
- **AND** no se intenta “reparar” el estado llamando Xlib desde el callback

#### Scenario: Plataforma no Linux

- **WHEN** ClipVault compila en macOS, Windows u otra plataforma
- **THEN** el código X11 no se compila ni se ejecuta
- **AND** los backends y permisos de esa plataforma conservan su contrato

### Requirement: Adaptadores de plataforma permanecen aislados

La corrección SHALL mantener la lógica de inicialización X11 en un adaptador
pequeño, testeable e independiente de Tauri, Svelte y SQLite. El shell SHALL
consumir sólo su resultado y los comandos Tauri SHALL seguir siendo
adaptadores delgados. La solución MUST NOT introducir `unsafe impl Send` o
`unsafe impl Sync` para ocultar la concurrencia.

#### Scenario: Prueba sin display real

- **WHEN** los tests del helper usan loader y resultado nativos falsos
- **THEN** pueden cubrir éxito, fallo e idempotencia sin display gráfico
- **AND** no escriben en `~/.clipvault` ni generan assets
