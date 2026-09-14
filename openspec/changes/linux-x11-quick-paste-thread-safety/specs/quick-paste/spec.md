# Activación segura de Quick Paste en X11

## ADDED Requirements

### Requirement: Ctrl+Shift+V no debe abortar el proceso X11

Cuando el usuario pulse el atajo Linux existente `Ctrl+Shift+V` en una sesión
X11, ClipVault SHALL abrir o enfocar la ventana Quick Paste existente sin
provocar un aborto de XCB/Xlib. La activación SHALL conservar el listener
único y la señal metadata-only del flujo actual. Si el preflight Xlib no está
disponible, la aplicación SHALL permanecer viva y reportar el atajo como
indisponible, sin simular una apertura exitosa.

#### Scenario: Primera apertura en X11

- **GIVEN** ClipVault está ejecutándose en Linux X11 y el preflight Xlib fue
  exitoso
- **WHEN** el usuario pulsa `Ctrl+Shift+V`
- **THEN** la ventana Quick Paste existente se muestra y recibe foco
- **AND** no aparece el aborto `[xcb] Unknown sequence number`
- **AND** el proceso permanece vivo

#### Scenario: Aperturas repetidas

- **GIVEN** Quick Paste fue abierta, ocultada o cerrada varias veces
- **WHEN** el usuario vuelve a pulsar `Ctrl+Shift+V`
- **THEN** la activación sigue siendo idempotente y no agrega listeners ni
  hilos de inicialización Xlib
- **AND** la ventana vuelve a mostrarse/enfocarse sin aborto nativo

#### Scenario: Activación mantiene el target anterior

- **GIVEN** una aplicación X11 era la ventana activa antes de abrir Quick Paste
- **WHEN** el usuario selecciona y pega una entrada por el flujo existente
- **THEN** el target capturado sigue siendo esa aplicación
- **AND** Quick Paste conserva su comportamiento actual de ocultarse y
  devolver el foco cuando el backend lo permite

#### Scenario: Preflight no disponible

- **GIVEN** Xlib no puede inicializarse de forma segura
- **WHEN** el usuario intenta depender de `Ctrl+Shift+V`
- **THEN** ClipVault no inicia un manager Xlib que pueda abortar el proceso
- **AND** la aplicación, la base local y las funciones no dependientes del
  atajo permanecen utilizables
- **AND** el estado mostrado distingue indisponibilidad técnica de conflicto
  de atajo o permiso de macOS

### Requirement: El callback del hotkey no usa la ventana nativa directamente

El callback que recibe el evento desde el hilo de `global-hotkey` SHALL
publicar únicamente la señal existente de apertura de Quick Paste y SHALL
deferir show, focus, centering, lectura de UI y cualquier operación de
ventana al flujo de aplicación correspondiente. El payload SHALL continuar
vacío o metadata-only y no SHALL transportar contenido, hashes, rutas,
identificadores de ventana ni bytes de imagen.

#### Scenario: Evento desde el hilo X11

- **WHEN** `global-hotkey` entrega el evento desde su hilo de eventos
- **THEN** el callback publica la señal de Quick Paste sin invocar APIs nativas
  de ventana desde ese hilo
- **AND** el listener único ejecuta la secuencia existente de activación
- **AND** una segunda señal concurrente no crea un listener duplicado ni una
  segunda ruta de apertura
