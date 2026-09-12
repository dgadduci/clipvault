# Metadata Linux desde Desktop File IDs Wayland

## ADDED Requirements

### Requirement: Resolución de Desktop File ID de GNOME Wayland

Cuando la integración GNOME Wayland entrega un Desktop File ID válido, ClipVault SHALL conservarlo como `source_app` y SHALL usar el provider Linux existente para resolver metadata local. El provider MUST comparar el ID completo, incluido el sufijo `.desktop`, contra el Desktop File ID de una entrada local; MUST NOT tratarlo como un `WM_CLASS` ni mutar el identificador persistido.

#### Scenario: Aplicación GNOME con Desktop File ID

- GIVEN una sesión GNOME Wayland con la integración conectada
- AND GNOME Shell publica `org.mozilla.firefox.desktop`
- AND existe una entrada local correspondiente con `Name` e `Icon`
- WHEN se procesa una captura permitida
- THEN `source_app` conserva `org.mozilla.firefox.desktop`
- AND el provider puede persistir `source_app_name` y una referencia de icono relativa controlada
- AND el blacklist se evalúa antes de leer la entrada o crear el asset

#### Scenario: Prioridad de raíz XDG para Desktop File ID duplicado

- GIVEN dos entradas locales con el mismo Desktop File ID
- AND una pertenece a una raíz XDG de mayor precedencia
- WHEN el provider resuelve ese ID
- THEN usa la entrada de la raíz de mayor precedencia
- AND no deja que el path lexicográficamente menor de otra raíz la reemplace

### Requirement: Identificadores window-backed no atribuibles

La extensión GNOME MUST publicar ausencia cuando `Shell.App` sea window-backed o cuando su id use el formato especial `window:*`. ClipVault MUST NOT inferir una app, nombre o icono desde ese valor.

#### Scenario: Shell publica un id especial de ventana

- GIVEN que el foco GNOME corresponde a una `Shell.App` window-backed
- AND su id es `window:6`
- WHEN la extensión actualiza el foco
- THEN comunica ausencia de `app_id`
- AND el snapshot no queda `identified` con `window:6`
- AND no se transportan título, PID, ruta, icono ni contenido como fallback

#### Scenario: Fila legacy con identificador especial

- GIVEN una fila existente cuyo `source_app` es `window:6`
- WHEN corre el backfill de metadata Linux
- THEN la fila no genera lookup ni I/O de iconos repetido
- AND conserva sus campos persistidos sin renombrar ni borrar assets

### Requirement: Compatibilidad del matcher X11 existente

La incorporación de Desktop File IDs SHALL conservar el matcher de `WM_CLASS` para X11 y XWayland, incluidos sus órdenes de prioridad y sus fallbacks.

#### Scenario: Identificador WM_CLASS sin sufijo desktop

- GIVEN una captura X11 o XWayland con identificador `firefox`
- WHEN el provider resuelve metadata
- THEN conserva la prioridad `StartupWMClass` → `X-GNOME-WMClass` → filename stem
- AND no requiere ni fabrica el sufijo `.desktop`
