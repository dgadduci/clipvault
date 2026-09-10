# Cambios de integración de plataforma

## MODIFIED Requirements

### Requirement: Identificación de aplicaciones nativas Wayland

El sistema SHALL intentar identificar la aplicación activa en una sesión Linux Wayland mediante un protocolo público de toplevels compatible, usando su `app_id` como identificador de origen cuando el snapshot esté comprometido.

#### Scenario: Aplicación nativa Wayland identificada

- GIVEN una sesión Wayland cuyo registry anuncia un protocolo compatible
- AND el toplevel activo publica un `app_id` no vacío
- WHEN el capture loop consulta la aplicación activa
- THEN el probe devuelve ese `app_id` como `ActiveApplication.identifier`
- AND la captura entrega el identificador al PrivacyGate antes de persistir

#### Scenario: Snapshot aún no comprometido

- GIVEN que el adapter recibió propiedades parciales de un toplevel
- WHEN todavía no recibió el evento de commit `done`
- THEN no publica una identidad parcialmente construida
- AND conserva el último snapshot válido o devuelve `Ok(None)`

### Requirement: Precedencia segura entre Wayland nativo y XWayland

En una sesión Linux Wayland el sistema MUST usar el adapter nativo como fuente autoritativa cuando esté operativo y MUST NOT reutilizar una identidad XWayland obsoleta después de que el adapter nativo informe que no existe un toplevel activo.

#### Scenario: Wayland nativo tiene prioridad

- GIVEN un snapshot XWayland anterior con un identificador
- AND el adapter nativo Wayland está operativo
- AND el snapshot nativo informa otro toplevel activo
- WHEN se resuelve el origen
- THEN se usa exclusivamente el `app_id` nativo

#### Scenario: Fallback XWayland por protocolo no disponible

- GIVEN una sesión Wayland con `$DISPLAY` usable
- AND ningún protocolo nativo compatible está disponible
- WHEN se resuelve el origen
- THEN se puede usar la sonda XWayland existente
- AND no se modifica la conducta actual de aplicaciones XWayland

#### Scenario: Wayland nativo sin aplicación activa

- GIVEN que el adapter nativo está operativo
- AND no existe un toplevel activo
- WHEN se resuelve el origen
- THEN devuelve `Ok(None)`
- AND no cae a un snapshot XWayland anterior

### Requirement: Resolución de nombre e icono reutilizada

Cuando el probe obtiene un identificador Wayland, el sistema SHALL reutilizar `LinuxApplicationMetadataProvider` para resolver nombre localizado e icono, guardando el icono PNG en el namespace de assets de aplicaciones existente cuando sea resoluble.

#### Scenario: app_id coincide con metadata Linux

- GIVEN una captura con `source_app` proveniente de Wayland
- AND existe un archivo `.desktop` correspondiente
- WHEN se completa el enriquecimiento
- THEN la fila contiene `source_app_name` y, si el icono es válido, `source_app_icon_ref`
- AND el icono se guarda bajo `assets/application-icons/`

#### Scenario: Icono no resoluble

- GIVEN un `app_id` válido cuyo `.desktop` no tiene un icono compatible
- WHEN se completa el enriquecimiento
- THEN se conserva el identificador y el nombre si está disponible
- AND la ausencia del icono no rechaza la captura ni la regla de blacklist

### Requirement: Degradación tipada

El sistema SHALL devolver un resultado tipado y no bloqueante cuando el protocolo Wayland no esté anunciado, sea incompatible, la conexión falle o el compositor impida la enumeración.

#### Scenario: Protocolo ausente

- GIVEN una sesión Wayland sin los protocolos soportados
- WHEN se inicializa el adapter
- THEN el diagnóstico informa backend Wayland y estado `Unavailable`
- AND el capture loop continúa funcionando
- AND no se fabrica `source_app` desde título, PID, proceso o contenido

#### Scenario: Desconexión del compositor

- GIVEN que la conexión Wayland se cierra durante el funcionamiento
- WHEN el probe consulta el snapshot
- THEN devuelve `Unavailable` de forma segura
- AND no bloquea ni termina la aplicación

### Requirement: Privacidad y blacklist

El sistema MUST evaluar el identificador Wayland en el PrivacyGate antes de escribir la fila, el contenido o cualquier asset asociado a una captura.

#### Scenario: Captura de aplicación blacklistada

- GIVEN un `app_id` Wayland incluido en la lista ignorada
- WHEN se procesa una captura
- THEN no se persiste la entrada
- AND no se escribe un icono de aplicación por ese intento
- AND no se registra contenido, hash, ruta ni payload en logs

### Requirement: Compatibilidad multiplataforma

La funcionalidad Wayland SHALL estar limitada a Linux y MUST NOT cambiar los adapters nativos de macOS, el comportamiento X11/XWayland existente ni los contratos de imágenes, tags, colecciones, favoritos, búsqueda, Quick Paste y drag-and-drop.

#### Scenario: Build no Linux

- GIVEN una compilación macOS
- WHEN se compila el workspace
- THEN no se enlazan dependencias ni símbolos exclusivos del adapter Wayland
- AND los tests y capacidades macOS existentes continúan pasando
