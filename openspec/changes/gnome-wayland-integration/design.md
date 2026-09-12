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

#### Compatibilidad de la llamada GI de conexión

La extensión crea el cliente con la propiedad GObject
`type: Gio.SocketType.STREAM` y abre el socket de sesión con
`Gio.SocketClient.connect_async(connectable, cancellable, callback)`. GNOME
Shell 42 expone esa propiedad y la firma de tres argumentos mediante GI, y esa
versión está incluida en `metadata.json`; no debe usarse `socket_type`, ni
pasarse una prioridad o un argumento placeholder de APIs asíncronas diferentes.
El callback obtiene el resultado con `connect_finish` y recién entonces escribe
el `hello`.

Una excepción al iniciar la conexión debe conservar el backoff existente y no
bloquear GNOME Shell, pero no debe silenciar una incompatibilidad de API o
propiedad en las versiones declaradas soportadas. La regresión protege el
constructor y la forma de la llamada y, en GNOME Shell 42 real, verifica que
una extensión habilitada y un listener existente transicionen de
`activation_pending` a `connected` o `identified`.

### 3. Adapter en ClipVault

Crear un adapter de plataforma, por ejemplo `GnomeShellActiveApplication`, detrás de feature target-specific Linux. Debe implementar `ActiveApplicationProbe` mediante un snapshot concurrente.

#### Contrato de activación de la feature del shell

La feature que controla los módulos y comandos condicionales del shell es
`clipvault-app/linux-gnome-shell-integration`. Activar la feature homónima
solamente en la dependencia `clipvault-platform` **no** activa
`cfg(feature = "linux-gnome-shell-integration")` dentro de `clipvault-app`:
las features de Cargo no se propagan desde una dependencia hacia su paquete
consumidor.

Por tanto, toda build Linux normal de producto debe activar explícitamente la
feature del paquete `clipvault-app`, tanto en `cargo tauri dev` como en el
build empaquetado. La configuración debe conservar los `cfg` del código en
Linux: que la feature esté seleccionada por la configuración por defecto no
debe compilar, enlazar ni presentar la integración en macOS u otros targets.

`feature_disabled` queda reservado para una build intencionalmente reducida,
no para el comando de desarrollo o el artefacto Linux que se entrega al
usuario. La comprobación de runtime debe inspeccionar la línea `DevCommand`
de Tauri y confirmar que incluye `linux-gnome-shell-integration`; después,
en GNOME Wayland, `clipvault_gnome_integration_status` debe devolver el
estado listo para consentimiento, no `not_configured` / `feature_disabled`.

Esta aclaración nació de la validación real del 2026-09-11: `cargo tauri dev`
ejecutó `cargo run --no-default-features --features
clipboard-arboard,hotkey-global`, dejando fuera la feature del paquete shell.
Aunque `clipvault-platform` sí la tenía en su bloque de dependencias Linux,
la interfaz reportó `Integración no disponible (feature_disabled)` y no pudo
mostrar el consentimiento. Esa ejecución no valida la integración.

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

#### Identificador opaco e iconos

El valor publicado por GNOME Shell es metadata opaca. En la validación de
runtime actual el diagnóstico alcanzó `identified` con `window:6`; ese valor
confirma el transporte, pero no constituye por sí solo una referencia de asset
ni una ruta de icono. La extensión no debe ampliar el protocolo con iconos,
rutas, títulos, PID ni contenido para resolverlo.

El provider Linux y la UI pueden conservar el nombre de origen que ya logren
resolver, pero `source_app_icon_ref` sigue siendo opcional. Si no hay una
referencia de icono segura y renderizable, la card debe mantener su fallback
sin fabricar un icono. Resolver esa ausencia específicamente en Wayland queda
fuera de este cambio y requiere una propuesta OpenSpec separada.

#### Propiedad del listener y seguridad entre hilos

`GnomeIntegrationState` vive dentro del estado gestionado por Tauri, que exige
`Send + Sync`. El `ListenerHandle` sólo contiene un flag atómico compartido,
un `JoinHandle` y, opcionalmente, la ruta del socket para su limpieza; por lo
tanto debe ser `Send` estructuralmente. No se permite satisfacer ese contrato
con un `unsafe impl Send`: no debe contener un marcador artificial que lo
vuelva `!Send` ni referencias al loop o a una ventana Tauri.

El shell sólo crea el transporte y conserva el handle. El loop, el backoff,
la actualización del snapshot y la limpieza del socket se reutilizan desde
`clipvault-platform::spawn_listener_thread_with_socket`; el shell no duplica
un segundo loop. El transporte Unix es no bloqueante, de modo que el loop
observa la cancelación, hace join y elimina el socket al cerrar ClipVault.

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

El diagnóstico es también el punto de entrada explícito a la configuración:
cuando el estado sea `ready` y `applicable = true`, su tarjeta debe ofrecer
**Configurar integración GNOME**. El control abre el modal de consentimiento
con el snapshot ya consultado; no acepta consentimiento, instala ni cambia la
configuración por sí mismo. Las sesiones no aplicables y las builds sin la
feature conservan sólo el diagnóstico informativo.

Los errores tipados de los comandos Tauri se convierten a un mensaje breve y
estable antes de mostrarse. La UI no debe interpolar un objeto de IPC como
`[object Object]`, ni reflejar rutas locales o detalles crudos de I/O. El
mensaje debe distinguir, como mínimo, recursos ausentes de la build, fallo de
instalación local y fallo al iniciar el listener.

El shell resuelve la extensión únicamente desde el directorio de recursos de
Tauri. En Linux, `cargo tauri dev` conserva el prefijo `resources/` dentro del
directorio de recursos, mientras que algunos bundles pueden exponer el recurso
directamente. La resolución debe admitir ambas disposiciones sin caer al árbol
fuente ni depender del directorio de trabajo; así el mismo contrato se prueba
en desarrollo y en el artefacto distribuido.

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
