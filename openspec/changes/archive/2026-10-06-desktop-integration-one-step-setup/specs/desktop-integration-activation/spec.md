## ADDED Requirements

### Requirement: Presentar un flujo de activación sencillo y consentido

La vista Integraciones de escritorio SHALL completar las operaciones locales
de activación en la menor cantidad de acciones soportada por el escritorio,
sin saltarse el consentimiento explícito ni afirmar un estado que el adapter
no confirmó.

#### Scenario: Configurar GNOME por primera vez

- **GIVEN** GNOME Wayland es compatible y aún no existe consentimiento
- **WHEN** la persona confirma Configurar integración después de leer la
  explicación
- **THEN** ClipVault guarda su decisión e instala localmente la extensión en
  el mismo recorrido iniciado por esa acción
- **AND** intenta habilitarla mediante la interfaz pública de GNOME
- **AND** no requiere una segunda pulsación para comenzar la instalación
- **AND** muestra la conexión solo después de confirmarla y explica cualquier
  paso manual restante

#### Scenario: Configuración GNOME requiere volver a iniciar sesión

- **GIVEN** la extensión se instaló pero GNOME todavía no la cargó o conectó
- **WHEN** ClipVault consulta el estado
- **THEN** explica en lenguaje cotidiano si la persona debe habilitar la
  extensión o cerrar y volver a iniciar sesión
- **AND** ofrece abrir Extensiones solo si existe un destino seguro conocido
- **AND** no muestra la integración como activa antes de confirmar la conexión
- **AND** permite abrir Extensiones si existe la aplicación del sistema
- **AND** conserva el consentimiento y ofrece reintentar sin repetirlo

#### Scenario: Configurar KDE Plasma Wayland

- **GIVEN** KDE Plasma Wayland es compatible y no se aceptó la integración
- **WHEN** la persona confirma Configurar integración
- **THEN** ClipVault guarda el consentimiento e inicia la instalación y
  activación existentes dentro de esa acción
- **AND** muestra el éxito únicamente después de confirmar el estado habilitado
  y conectado

#### Scenario: La instalación falla después del consentimiento

- **GIVEN** la persona aceptó la integración
- **WHEN** una operación local de instalación o activación falla
- **THEN** ClipVault indica que la integración no quedó lista
- **AND** permite reintentar sin pedir otra vez el consentimiento ya guardado
- **AND** conserva los datos y el estado previo seguros
- **AND** al volver a consultar el estado muestra la decisión guardada

#### Scenario: Rechazar, desactivar o retirar

- **WHEN** la persona rechaza la configuración, desactiva o desinstala una
  integración
- **THEN** ClipVault no realiza operaciones adicionales sin consentimiento
- **AND** conserva accesibles las acciones para cambiar de decisión o volver
  a configurar desde Integraciones de escritorio

### Requirement: Localizar todo texto de producto

Todo texto de producto añadido o modificado por el flujo de configuración
SHALL estar en los catálogos `en`, `es`, `pt`, `de` y `fr`, con claves y
placeholders en paridad.

#### Scenario: Resultado de configuración localizado

- **WHEN** la persona configura, reintenta o encuentra un error de integración
- **THEN** progreso, consentimiento, pasos pendientes, errores y acciones se
  presentan en el idioma elegido
