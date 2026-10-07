# contextual-desktop-setup-guidance Specification

## Purpose
Define cuándo y cómo ClipVault presenta una sugerencia no modal para configurar
integraciones GNOME/KDE en escritorios Wayland compatibles, sin bloquear el
uso de la aplicación y manteniendo un acceso permanente a la configuración.

## Requirements

### Requirement: Mostrar ayuda inicial solo donde sea útil

ClipVault SHALL mostrar una guía contextual no modal únicamente cuando
detecte GNOME Wayland o KDE Plasma Wayland compatible y el usuario aún no
haya configurado o descartado la integración correspondiente. La guía MUST
NOT retrasar ni bloquear la ventana principal.

#### Scenario: Primer inicio en GNOME Wayland compatible

- **GIVEN** la ventana principal está lista
- **AND** ClipVault detecta GNOME Wayland compatible
- **AND** el consentimiento GNOME es desconocido y el aviso no fue descartado
- **WHEN** termina la consulta local de estado
- **THEN** ClipVault muestra una tarjeta pequeña con Configurar y Ahora no
- **AND** búsqueda, captura e historial siguen disponibles

#### Scenario: Primer inicio en KDE Plasma Wayland compatible

- **GIVEN** la ventana principal está lista
- **AND** ClipVault detecta KDE Plasma Wayland compatible
- **AND** el consentimiento KDE es desconocido y el aviso no fue descartado
- **WHEN** termina la consulta local de estado
- **THEN** ClipVault muestra una tarjeta que abre la configuración KDE correcta
- **AND** no bloquea el resto de la aplicación

#### Scenario: Configurar desde la tarjeta

- **WHEN** la persona selecciona Configurar
- **THEN** ClipVault abre Integraciones de escritorio enfocada en la integración
  detectada
- **AND** el panel consulta el estado actual
- **AND** la tarjeta no acepta consentimiento, instala componentes ni concede
  permisos por sí misma

### Requirement: Recordar una tarjeta descartada

ClipVault SHALL recordar localmente, por integración, que la persona descartó
la tarjeta. Ese estado MUST ser diferente de aceptar o rechazar el
consentimiento de una extensión.

#### Scenario: Ahora no

- **WHEN** la persona selecciona Ahora no
- **THEN** ClipVault oculta y recuerda el aviso de esa integración
- **AND** no instala ni habilita componentes
- **AND** la integración sigue disponible desde Configuración general

#### Scenario: Rechazo explícito previo

- **GIVEN** la persona ya rechazó el consentimiento desde Integraciones de
  escritorio
- **WHEN** ClipVault inicia después
- **THEN** no vuelve a mostrar la tarjeta inicial
- **AND** la persona puede cambiar su decisión desde la configuración

#### Scenario: Integración activa o deshabilitada explícitamente

- **GIVEN** la integración ya está conectada, o la persona la deshabilitó
- **WHEN** ClipVault consulta el estado al iniciar
- **THEN** no muestra la tarjeta de primera configuración

### Requirement: Respetar la configuración de las demás plataformas

ClipVault MUST NOT mostrar una guía inicial GNOME/KDE en macOS, Linux X11, un
compositor Wayland no compatible o una sesión no reconocida.

#### Scenario: macOS requiere Accesibilidad

- **GIVEN** macOS informa que falta el permiso de Accesibilidad para pegar
- **WHEN** la persona intenta una acción de pegado que lo necesita
- **THEN** ClipVault utiliza la guía de plataforma en ese contexto
- **AND** no convierte el permiso en requisito para abrir o usar el historial

#### Scenario: Linux X11 no requiere instalar una integración Wayland

- **GIVEN** ClipVault detecta Linux X11
- **WHEN** la ventana principal queda lista
- **THEN** no aparece una tarjeta para instalar GNOME o KDE

#### Scenario: Wayland sin integración compatible

- **GIVEN** ClipVault detecta un compositor Wayland sin integración compatible
- **WHEN** la ventana principal queda lista
- **THEN** no aparece una acción que prometa instalar una integración
- **AND** cualquier limitación se explica junto a la función solicitada

### Requirement: Localizar todos los textos de la guía

Todo texto visible o accesible de la tarjeta SHALL pertenecer a los catálogos
`en`, `es`, `pt`, `de` y `fr`, con claves y placeholders en paridad.

#### Scenario: Idioma configurado

- **WHEN** ClipVault muestra la tarjeta en la ventana principal
- **THEN** su título, explicación, botones y labels accesibles se presentan en
  el idioma elegido
