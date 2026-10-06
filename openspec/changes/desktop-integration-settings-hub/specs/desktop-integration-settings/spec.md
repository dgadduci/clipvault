## ADDED Requirements

### Requirement: Encontrar la configuración de escritorio

ClipVault SHALL ofrecer un acceso llamado **Integraciones de escritorio** en
Configuración general cuando la sesión detectada tenga una integración
GNOME/KDE compatible y aplicable. El acceso
SHALL estar disponible también al abrir Configuración desde el menú de la
bandeja del sistema.

#### Scenario: GNOME Wayland compatible

- **GIVEN** ClipVault detecta GNOME Wayland compatible
- **WHEN** la persona abre Configuración general
- **THEN** puede abrir Integraciones de escritorio
- **AND** encuentra el estado de GNOME y la siguiente acción disponible

#### Scenario: KDE Plasma Wayland compatible

- **GIVEN** ClipVault detecta KDE Plasma Wayland compatible
- **WHEN** la persona abre Configuración general
- **THEN** puede abrir Integraciones de escritorio
- **AND** encuentra el estado de KWin y la siguiente acción disponible

#### Scenario: Entorno sin integración aplicable

- **GIVEN** ClipVault se ejecuta en macOS, Linux X11 o un escritorio Wayland
  sin una integración compatible
- **WHEN** la persona abre Configuración general
- **THEN** ClipVault no presenta una opción de instalación que no aplique
- **AND** conserva las guías existentes para las funciones que tengan una
  limitación o un permiso pendiente

### Requirement: Presentar estados y acciones comprensibles

La vista de Integraciones de escritorio SHALL mostrar el estado actual en
lenguaje cotidiano y SHALL ofrecer únicamente acciones válidas para ese
estado. SHALL reutilizar los servicios y comandos existentes sin duplicar el
estado de consentimiento.

#### Scenario: Estado consultado al abrir

- **WHEN** la persona abre Integraciones de escritorio
- **THEN** ClipVault consulta el estado actualizado de la integración
- **AND** permite volver a consultarlo sin reiniciar la aplicación

#### Scenario: Estado requiere una acción del usuario

- **GIVEN** GNOME o KDE indica que falta completar una acción
- **WHEN** la persona abre Integraciones de escritorio
- **THEN** ClipVault explica el siguiente paso con palabras sencillas
- **AND** no muestra un estado conectado antes de verificar la conexión

#### Scenario: Diagnóstico técnico

- **WHEN** la persona abre la vista de usuario
- **THEN** ClipVault no muestra backend, versión de protocolo, identificadores
  técnicos, rutas internas, variables de entorno ni errores sin localizar
- **AND** Development conserva el diagnóstico detallado disponible para
  soporte y desarrollo

### Requirement: Localizar todo texto de producto

Todo texto visible o accesible añadido o modificado por esta capacidad SHALL
estar en los catálogos `en`, `es`, `pt`, `de` y `fr`, con claves y
placeholders en paridad.

#### Scenario: Cambio de idioma

- **WHEN** la persona cambia el idioma de ClipVault
- **THEN** el título, estado, instrucciones, acciones, errores y etiquetas
  accesibles de Integraciones de escritorio usan el idioma elegido
