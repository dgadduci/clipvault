# Integración GNOME Shell

## ADDED Requirements

### Requirement: Integración opcional y consentida

En una sesión GNOME Wayland, ClipVault SHALL detectar el estado de la integración GNOME y MUST request explicit user consent before installing or enabling its GNOME Shell extension.

#### Scenario: Usuario acepta la integración

- GIVEN que la sesión es GNOME Wayland
- AND la extensión no está instalada o habilitada
- WHEN el usuario activa "Activar integración GNOME"
- THEN ClipVault instala la extensión incluida en sus propios recursos para el usuario actual
- AND solicita su activación mediante un mecanismo soportado por GNOME
- AND persiste la decisión del usuario
- AND no requiere privilegios de root

#### Scenario: Usuario rechaza la integración

- GIVEN que la integración no está activa
- WHEN el usuario selecciona "Ahora no"
- THEN ClipVault no instala ni habilita la extensión
- AND conserva el funcionamiento de captura y pegado
- AND no muestra el mismo aviso en cada arranque

#### Scenario: Integración incompatible

- GIVEN una versión de GNOME Shell incompatible
- WHEN ClipVault comprueba la integración
- THEN muestra un estado de incompatibilidad comprensible
- AND no instala código incompatible
- AND mantiene el origen desconocido sin bloquear capturas

### Requirement: Identidad de aplicación GNOME

La extensión SHALL comunicar únicamente el `app_id` o desktop id de la aplicación enfocada y MUST NOT communicate window title, PID, process path, clipboard data or window contents.

#### Scenario: Aplicación nativa identificada

- GIVEN que la extensión está conectada
- AND GNOME Shell informa un identificador de aplicación válido
- WHEN cambia la aplicación enfocada
- THEN ClipVault actualiza su snapshot con ese identificador
- AND el siguiente intento de captura puede evaluarlo en PrivacyGate

#### Scenario: Sin identificador

- GIVEN que GNOME Shell no puede obtener un identificador válido
- WHEN cambia el foco
- THEN la extensión comunica ausencia
- AND ClipVault no fabrica un identificador a partir del título, PID u orden de ventanas

### Requirement: Canal metadata-only no bloqueante

El canal entre la extensión y ClipVault SHALL ser local a la sesión del usuario, versionado, no bloqueante y capaz de informar conexión, desconexión, ausencia y `app_id`.

#### Scenario: Desconexión

- GIVEN que ClipVault o la extensión se desconecta
- WHEN el adapter consulta el estado
- THEN devuelve `Unavailable` de forma tipada
- AND el capture loop continúa funcionando

#### Scenario: Reintento controlado

- GIVEN una desconexión temporal
- WHEN se reintenta la conexión
- THEN se aplica backoff o coalescencia
- AND no se genera una tormenta de conexiones ni se bloquea GNOME Shell

### Requirement: Persistencia y ciclo de vida

La instalación SHALL ser atómica, reversible y limitada al directorio de extensiones del usuario; MUST NOT sobrescribir extensiones ajenas ni borrar recursos que no pertenezcan a ClipVault.

#### Scenario: Instalación incompleta

- GIVEN que falla la escritura o validación de la extensión
- WHEN ClipVault instala la integración
- THEN no queda una extensión parcial habilitable
- AND el diagnóstico informa el fallo sin incluir rutas absolutas

#### Scenario: Deshabilitar o desinstalar

- GIVEN que la integración está activa
- WHEN el usuario la deshabilita o desinstala
- THEN ClipVault deja de usar el snapshot GNOME
- AND vuelve al fallback disponible
- AND las capturas continúan funcionando

### Requirement: Precedencia y compatibilidad

La integración GNOME SHALL ser autoritativa sólo cuando esté conectada; MUST preserve X11/XWayland and other Wayland adapters when it is absent.

#### Scenario: GNOME nativo sobre XWayland

- GIVEN una sesión GNOME Wayland con la integración GNOME conectada
- AND existe una identidad XWayland antigua en la caché
- WHEN se resuelve el origen
- THEN se usa exclusivamente el snapshot GNOME actual

#### Scenario: Fallback XWayland

- GIVEN que la integración GNOME no está conectada
- AND `$DISPLAY` es usable
- WHEN se resuelve el origen
- THEN se puede utilizar el adapter XWayland existente
- AND no se modifica su comportamiento actual

### Requirement: Privacidad y blacklist

El identificador GNOME SHALL enter PrivacyGate before persistence or application-icon enrichment.

#### Scenario: Aplicación blacklistada

- GIVEN un `app_id` comunicado por la extensión que está en la lista ignorada
- WHEN se procesa una captura
- THEN no se persiste la entrada
- AND no se crea un asset de icono por ese intento
- AND los logs siguen siendo metadata-only

### Requirement: Arranque no intrusivo de la ventana principal en Wayland

ClipVault MUST let Tauri create the configured `main` toplevel using its
native default visibility. Mientras la ventana inicial de GNOME Wayland aún
no tiene monitor primario, el shell MUST NOT emitir solicitudes adicionales
de `show`, `unminimize`, foco, tamaño o posición: esas solicitudes previas al
mapeo pueden dejar el proceso vivo sólo en la bandeja.

#### Scenario: Wayland sin monitor primario informado

- GIVEN una sesión gráfica Wayland donde `primary_monitor()` devuelve ausencia
- WHEN ClipVault inicia
- THEN conserva la creación y geometría declaradas en `tauri.conf.json`
- AND no consulta monitores alternativos ni muta la ventana antes del mapeo
- AND Tauri presenta la ventana principal con su comportamiento configurado

#### Scenario: Abrir desde la bandeja

- GIVEN que la ventana principal fue ocultada previamente
- WHEN el usuario selecciona `Open ClipVault` desde la bandeja
- THEN ClipVault intenta desminimizar, mostrar y enfocar la misma ventana principal
- AND no altera el ciclo de arranque de la ventana inicial

#### Scenario: Solicitudes previas al mapeo

- GIVEN una ventana `main` recién creada en GNOME Wayland
- WHEN `primary_monitor()` devuelve ausencia durante `setup`
- THEN el shell retorna del ajuste opcional de layout sin llamar a `show`,
  `unminimize`, `set_focus`, `set_size` ni `set_position`
- AND no usa `RunEvent::Ready` ni un montaje Svelte para repetir esas solicitudes
