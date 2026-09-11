# Diseño: integración GNOME Wayland

## Relación con los adapters existentes

El adapter `linux-native-wayland-app-detection` cubre protocolos públicos que algunos compositores anuncian. GNOME/Mutter puede no anunciar un protocolo público que permita identificar el toplevel enfocado, por lo que este cambio añade un proveedor específico de GNOME.

La selección de fuentes debe ser:

1. GNOME Shell Extension cuando la sesión sea GNOME Wayland y la integración esté conectada;
2. adapter nativo Wayland público cuando no haya integración GNOME y el protocolo sí permita foco;
3. adapter XWayland cuando el nativo no esté operativo y `$DISPLAY` sea usable;
4. `Unavailable`/`Ok(None)` cuando no exista una fuente válida.

La fuente GNOME conectada es autoritativa para esa sesión. Cuando comunica `Ok(None)`, no se debe reutilizar una identidad XWayland obsoleta. Debe existir una única caché y un único `CaptureWatcher` compartido.

## Componentes

### 1. Recursos de la extensión

Agregar un recurso versionado dentro del paquete de la aplicación con, como mínimo:

- `metadata.json` con UUID, nombre, descripción y versiones de GNOME soportadas;
- código JavaScript de la extensión;
- manifiesto o recurso de comunicación, si el diseño lo requiere;
- tests aislados del código de extracción y normalización de identidad.

La extensión debe suscribirse al cambio de ventana enfocada usando APIs públicas de GNOME Shell, preferentemente mediante el tracker de aplicaciones (`Shell.WindowTracker`/`Shell.App`) y la identidad de aplicación que GNOME ya conoce. No debe basarse en el título.

La extracción debe probar, en orden documentado, las APIs públicas disponibles para obtener el desktop id/app id. Si no existe un identificador válido, debe comunicar ausencia. Nunca debe elegir una aplicación por posición, orden de ventanas o título.

Al habilitarse, la extensión se conecta al canal de ClipVault y publica el estado actual; ante cambios de foco publica sólo el nuevo identificador. Al deshabilitarse o perder la conexión, limpia su estado y libera todas las señales.

### 2. Canal de comunicación

Usar el bus D-Bus de sesión del usuario o el mecanismo IPC de GNOME soportado por la versión objetivo. El diseño final debe justificar:

- qué lado posee el nombre D-Bus;
- cómo se detecta que el peer pertenece a la misma sesión de usuario;
- cómo se negocia la versión;
- cómo se notifica conexión, desconexión, app_id presente y ausencia;
- cómo se evita bloquear GNOME Shell;
- cómo se reintenta sin tormenta de conexiones.

El protocolo debe ser metadata-only. Un contrato mínimo puede ser:

- `GetStatus() -> protocol_version, enabled, connected`;
- `SetActiveApplication(app_id)` o señal equivalente;
- `ClearActiveApplication()`;
- `GetVersion()`.

No se debe transportar título, PID, ruta, contenido ni lista completa de ventanas. Si se utiliza un nonce o token de sesión para evitar peers accidentales, debe ser efímero, no persistirse como secreto del proyecto y nunca aparecer en logs.

### 3. Adapter en ClipVault

Crear un adapter de plataforma, por ejemplo `GnomeShellActiveApplication`, detrás de feature target-specific Linux. Debe implementar `ActiveApplicationProbe` mediante un snapshot concurrente.

El adapter debe distinguir:

- GNOME no detectado;
- sesión no Wayland;
- integración no instalada;
- integración instalada pero deshabilitada;
- versión incompatible;
- integración conectada sin app activa;
- integración conectada con app_id;
- desconexión;
- error de comunicación.

La consulta del capture loop debe ser no bloqueante y leer la última identidad comprometida. El hilo de comunicación debe terminar limpiamente al cerrar ClipVault. No debe existir un segundo watcher ni una segunda fuente de deduplicación.

### 4. Instalador y ciclo de vida

Crear un servicio de instalación de usuario con estas propiedades:

- sólo se ejecuta tras consentimiento explícito;
- verifica que el recurso incluido sea el esperado;
- escribe en un directorio temporal dentro del directorio padre de extensiones;
- valida UUID y metadata antes de activar;
- renombra atómicamente al directorio final;
- no sigue symlinks fuera del directorio de instalación;
- no borra una extensión que no pertenezca a ClipVault;
- permite deshabilitar y desinstalar de forma reversible;
- detecta cambios de versión y puede actualizar la extensión sólo si el usuario ya había consentido.

La activación debe usar una API soportada por GNOME. Si la versión instalada no permite activar sin reinicio o requiere una acción del usuario, la UI debe indicarlo y el diagnóstico debe reflejar `activation_pending`, no declarar éxito prematuramente.

La preferencia de consentimiento debe guardarse en la configuración local de ClipVault, separada del estado técnico de la extensión:

- `unknown`: aún no preguntado;
- `accepted`: el usuario aceptó;
- `declined`: el usuario rechazó;
- `disabled`: el usuario la deshabilitó explícitamente.

Un rechazo no debe generar un modal repetitivo en cada arranque; la pantalla de configuración debe permitir reintentar.

### 5. Enriquecimiento y blacklist

El flujo de captura conservará el orden:

`lectura de snapshot GNOME → source identifier → PrivacyGate → persistencia → metadata enrichment → history-updated`.

El `app_id` recibido alimenta el `LinuxApplicationMetadataProvider` existente. No se duplica el parser `.desktop`, la búsqueda de iconos ni el almacenamiento bajo `assets/application-icons/`.

Una aplicación GNOME blacklistada debe bloquearse antes de crear la fila o consultar/persistir su icono. Una captura no identificada mantiene el contrato vigente de origen desconocido.

### 6. UI y diagnósticos

La UI debe mostrar sólo información útil al usuario:

- integración no necesaria;
- disponible para activar;
- activa;
- requiere acción del usuario;
- deshabilitada;
- incompatible o no disponible.

No mostrar rutas internas, UUID técnicos innecesarios, PID, títulos ni variables de entorno.

El diagnóstico metadata-only debe incluir backend estable, estado, versión de protocolo y etapa. No debe incluir secretos, contenido ni rutas absolutas.

## Compatibilidad y mantenimiento

La extensión GNOME debe declarar las versiones de Shell soportadas. El código debe fallar de manera segura ante una API ausente o cambios de GNOME Shell. No se debe informar `active_application = true` como garantía de disponibilidad si la integración no está realmente conectada.

La integración no debe afectar macOS, X11, XWayland, otros compositores Wayland, imágenes, tags, colecciones, favoritos, búsqueda, Quick Paste ni drag-and-drop.

## Pruebas

### Automatizadas

- detección de GNOME Wayland y de otras sesiones;
- estado inicial sin instalación;
- consentimiento, rechazo persistente y reintento manual;
- instalación atómica y validación de UUID;
- no sobrescritura de extensión ajena;
- activación exitosa, activación pendiente e incompatibilidad;
- negociación del canal y versión;
- app_id válido, vacío y cambios de foco;
- limpieza al deshabilitar y desconectar;
- snapshot no bloqueante y único;
- prioridad sobre identidad XWayland obsoleta;
- fallback XWayland cuando corresponde;
- integración con LinuxApplicationMetadataProvider;
- blacklist antes de metadata y cero assets para capturas rechazadas;
- no filtración de títulos, PID, rutas, contenido o tokens;
- regresiones de imágenes, tags, colecciones, favoritos, búsqueda, Quick Paste y drag-and-drop.

### Manuales Ubuntu GNOME

Se deben verificar en una sesión real:

1. primer inicio en GNOME Wayland muestra la propuesta una sola vez;
2. aceptar instala y activa la integración sin root;
3. Terminal, Firefox y Chrome nativos entregan nombre e icono;
4. una aplicación blacklistada no genera fila ni icono;
5. reiniciar ClipVault conserva la integración y los metadatos;
6. deshabilitar/desinstalar vuelve a `Unavailable` sin romper capturas;
7. XWayland sigue funcionando;
8. cerrar sesión o reiniciar GNOME no deja la aplicación bloqueada;
9. rechazo persistente no genera prompts repetitivos;
10. un GNOME no compatible informa la limitación claramente.
