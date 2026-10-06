## ADDED Requirements

### Requirement: Desactivar GNOME sin bloquear la aplicación

La integración GNOME SHALL desactivar la extensión cargada y cerrar su
listener sin bloquear indefinidamente la interfaz de ClipVault.

#### Scenario: Desactivar una integración GNOME conectada

- **GIVEN** GNOME Wayland tiene la extensión ClipVault habilitada y conectada
- **WHEN** la persona selecciona **Deshabilitar** en Integraciones de escritorio
- **THEN** ClipVault solicita a GNOME que desactive la extensión mediante la
  interfaz pública con el UUID fijo de ClipVault
- **AND** espera el resultado fuera del hilo de interfaz y con un tiempo límite
- **AND** el listener termina aunque estuviera leyendo una conexión persistente
- **AND** ClipVault retira los archivos locales de la extensión y guarda el
  consentimiento como `disabled`
- **AND** la interfaz vuelve a responder y ofrece habilitar la integración otra
  vez
- **AND** al reiniciar ClipVault el estado sigue siendo deshabilitado

#### Scenario: La operación GNOME de desactivación falla o vence

- **GIVEN** la integración GNOME está habilitada
- **WHEN** el ejecutable de GNOME no existe, falla o supera el tiempo límite
  de desactivación
- **THEN** ClipVault conserva los archivos de la extensión y el estado de
  consentimiento previo
- **AND** termina el proceso hijo si la operación supera el tiempo límite
- **AND** muestra un error localizado y permite reintentar
- **AND** la interfaz sigue respondiendo y el indicador de progreso termina

#### Scenario: Detener un peer GNOME que no se desconecta

- **GIVEN** el listener está procesando una conexión Unix persistente que no
  envía mensajes nuevos
- **WHEN** ClipVault solicita detenerlo
- **THEN** el listener observa la solicitud de cierre en tiempo acotado
- **AND** su hilo termina sin una espera indefinida en una lectura bloqueante

#### Scenario: Repetir la desactivación

- **GIVEN** la integración ya está desactivada o el listener ya terminó
- **WHEN** el ciclo de cierre se invoca de nuevo durante la limpieza
- **THEN** termina de forma idempotente y no bloquea ClipVault

### Requirement: Localizar los mensajes de desactivación

Todo texto de progreso, error o recuperación añadido o modificado por este
flujo SHALL estar presente en los catálogos `en`, `es`, `pt`, `de` y `fr`, con
claves y placeholders en paridad.

#### Scenario: Error de desactivación localizado

- **WHEN** GNOME no puede desactivar la extensión
- **THEN** ClipVault muestra el mensaje de error en el idioma elegido
