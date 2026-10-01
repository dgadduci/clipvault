# Diseño: origen de capturas en KDE Plasma Wayland

## Desajuste de formato y límite de la solución existente

KWin documenta `Window.desktopFileName` como el nombre base del archivo
`.desktop` sin la extensión ni la ruta, por ejemplo `org.kde.foo`. También
puede devolver una ruta completa al archivo `.desktop`. La prueba manual del
2026-09-28 sólo obtuvo nombre e icono para Warp; no registró el valor KWin de
cada aplicación.

La canonicalización recorta una ruta cuando la hay y añade `.desktop` a un
nombre base sin extensión. La revisión del resultado manual del 2026-09-28
mostró que el estado permanecía en `awaiting_consent` con consentimiento y
paquete activos, señal de que el receiver no había procesado ningún mensaje.
El script pasaba `"susu"` a `callDBus` como si fuera una firma; KWin define
`callDBus(service, path, interface, method, arg..., callback?)`, así que esa
cadena era el primer argumento de `Publish` y cambiaba el tipo/orden del
mensaje. El receiver esperaba `Publish(s id, u state, u v)` y lo rechazaba.

El paquete también estaba declarado como `declarativescript` y usaba un
`Loader` para cargar un archivo JavaScript como componente QML. El diseño
corregido usa el punto de entrada JavaScript KWin documentado:
`X-Plasma-API: javascript`, `X-Plasma-MainScript: code/main.js`, y registra
`workspace.windowActivated` directamente en el script.

La prueba manual posterior mantuvo el estado `activation_pending`: el bridge
se registraba, pero no recibía el primer `Publish`. KWin conserva un script
habilitado en memoria durante `reconfigure` y no carga otra vez un plugin con el
mismo ID mientras siga activo. Reescribir el KPackage en disco por sí solo no
actualizaba el script en ejecución. Antes de reconfigurar, ClipVault ahora pide
al servicio `org.kde.kwin.Scripting` que descargue únicamente su propio plugin;
después KWin puede cargar el JavaScript actualizado desde el paquete instalado.
Si el plugin aún no estaba cargado, la descarga no es necesaria y se continúa
con la reconfiguración.

La migración del paquete tenía otro residuo: el instalador reescribía metadata
y JavaScript, pero no retiraba `contents/ui/main.qml` que instaló la versión
declarativa anterior. Al actualizar un paquete existente, elimina sólo ese
archivo legado después de comprobar que el `Id` de metadata pertenece a
ClipVault. Conserva otros archivos del directorio del usuario.

La prueba siguiente confirmó que KWin no tenía el script cargado. `kwinrc`
mostró `Plugins/clipvault-kde-source-appEnabled=true` dentro de `[Plugins]`.
La API documentada de KWin configura scripts bajo el grupo `Plugins` con una
clave `<id>Enabled`; el prefijo adicional `Plugins/` hacía que KWin ignorase
la preferencia aunque el inspector de ClipVault la contara como habilitada.
El instalador ahora analiza la sección `[Plugins]`, escribe la clave correcta
y migra la clave mal formada propia si existe. La migración conserva su valor
booleano para respetar una desactivación previa y elimina sólo esa clave
obsoleta; el resto de `kwinrc` queda intacto.

La canonicalización ocurre antes de publicar el mensaje D-Bus: un nombre base
válido pasa a `nombre.desktop`, y una ruta KWin sólo aporta su basename. El
bridge mantiene la validación estricta del contrato D-Bus y el provider sigue
haciendo coincidencia exacta por Desktop File ID.

El probe Wayland genérico puede ser `Unavailable` en compositores que no
publican un protocolo compatible para conocer el toplevel enfocado. El cambio
activo `linux-native-wayland-app-detection` prohíbe inferir el foco a partir
del orden de la lista pública de ventanas. KDE requiere una fuente de foco
específica del compositor para cubrir ese caso.

## Fuente propuesta para KWin

Usar un script de usuario de KWin como adapter de sesión:

1. El script consulta inicialmente `workspace.activeWindow` y sigue los
   cambios de activación mediante el API de scripting de KWin.
2. Para la ventana activa lee `desktopFileName`. Ese valor es metadata de
   aplicación de KWin; no se lee ni se transmite el título de ventana. KWin
   puede dar un nombre base sin extensión o una ruta al archivo `.desktop`.
3. Si hay ruta, conserva sólo el basename. Si hay un nombre base sin la
   extensión, añade `.desktop`. Valida y acota el resultado antes de
   publicarlo; valores vacíos, inválidos o con separadores producen estado de
   origen desconocido. Nunca se persiste ni se informa una ruta.
4. Publica el identificador o el estado vacío hacia un bridge de ClipVault
   dentro del bus D-Bus de la sesión del mismo usuario. El contrato incluye
   versión, estado y como máximo el identificador validado. No incluye título,
   PID, rutas, formatos, hashes ni contenido del clipboard.
5. El bridge actualiza el snapshot del `ActiveApplicationProbe` compartido.
   Al desconectarse, deshabilitarse o dejar de tener ventana activa, borra el
   snapshot para que una identidad anterior no quede asociada a capturas nuevas.

La documentación oficial de KWin 6 describe `workspace.activeWindow`, la
señal `windowActivated` y la propiedad `desktopFileName`. También documenta
los paquetes KPackage de scripts KWin y su ubicación de usuario. El build
debe validar las versiones de Plasma/KWin realmente soportadas antes de
prometer compatibilidad más amplia.

## IPC, instalación y consentimiento

### Dependencia D-Bus

La decisión es usar `zbus` `~5.13.2`, como dependencia opcional y exclusiva
del target Linux, reenviada por `linux-kde-kwin-integration`. Se desactivan
features por defecto y se habilita sólo `tokio`: Tokio ya está en el
workspace y el receiver debe registrarse como servicio D-Bus de sesión sin
crear otro executor. El rango `~5.13.2` limita la resolución a `5.13.x` para
mantener el MSRV del workspace, Rust 1.85; `zbus` 5.14 elevó su requisito a
Rust 1.87. La dependencia aporta el servidor/dispatch tipado del bus y está
escrita en Rust, sin requerir headers, `pkg-config` ni una biblioteca
libdbus externa.

Se descarta `dbus` 0.9 para este bridge: su binding usa libdbus y su
configuración estándar requiere la biblioteca de desarrollo en build y
libdbus en runtime. El bus de sesión ya es parte de la sesión KDE objetivo;
no se agrega un daemon, ejecutable ni servicio remoto.

El servicio de ClipVault aceptará sólo mensajes con protocolo soportado y
sender asociado al propietario actual del nombre de bus `org.kde.KWin`.
`Publish` lleva versión y un identificador acotado; un
mensaje explícito de estado vacío borra el snapshot. Se rechazan cadenas
vacías, controles y valores con forma de ruta después de normalizar únicamente
el nombre de archivo permitido. Los errores no registran el argumento.

El protocolo 2 define `Publish(s id, s state, s version)`. El script convierte
el código de estado y la versión a string para que el tipo D-Bus no dependa de
la conversión de números JavaScript de Qt. El receiver valida esos valores
contra los códigos y versión soportados antes de actualizar el snapshot.

### Paquete y ciclo de vida

El script se instala como KPackage dentro del directorio KWin del usuario,
bajo un identificador reservado para ClipVault. Con consentimiento explícito,
ClipVault activa sólo la clave `<id>Enabled` del grupo `[Plugins]` en la
configuración KWin del usuario y pide a KWin reconfigurar mediante el bus de
sesión. No ejecuta
`kwriteconfig`, `qdbus`, `kpackagetool` ni otro proceso externo. Las
operaciones deben conservar el resto de `kwinrc`, ser atómicas e idempotentes,
y rechazar un directorio de paquete o una clave del plugin que pertenezca a
otro recurso.

La reconfiguración usa `org.kde.KWin /KWin org.kde.KWin.reconfigure` y
`org.kde.KWin /Scripting org.kde.kwin.Scripting.unloadScript`, invocados
directamente con `zbus`. El adapter de instalación escribe la clave y el
paquete; el lifecycle de Tauri ordena la descarga y reconfiguración, y sólo
cambia el probe compartido cuando el receiver está registrado.

Al reactivar una integración con consentimiento aceptado y paquete habilitado,
ClipVault vuelve a escribir sus recursos empaquetados, retira el punto de
entrada declarativo legado de su paquete, limpia la identidad anterior del
snapshot, descarga el script propio que KWin tenga activo y solicita la
reconfiguración. Así, una actualización de ClipVault también actualiza el
script KWin en ejecución y una identidad previa no se asocia a capturas durante
la recarga; los estados desinstalado o deshabilitado no se fuerzan a instalar o
activar automáticamente.

Desactivar cambia sólo la clave del plugin propio a deshabilitado y confirma
el estado con KWin; desinstalar además retira sólo el directorio de paquete
propio una vez desactivado. Si ya existe un recurso con el mismo identificador
que no puede atribuirse a ClipVault, no se sobrescribe ni elimina. Los estados
visibles distinguen consentimiento, instalación, activación, conexión y
causa tipada de indisponibilidad.

La validación de runtime Arch/KDE sigue pendiente para confirmar que: a) la
descarga vía D-Bus seguida de reconfiguración carga el KPackage de usuario; b)
los cambios de configuración persisten sin tocar claves ajenas; c) la
activación vuelve a publicar `workspace.activeWindow` si ClipVault inicia
después de KWin; y d) desactivar/desinstalar deja la sesión y los scripts
ajenos intactos. La descarga usa el API D-Bus de scripting expuesto por KWin;
no requiere una dependencia nueva, proceso auxiliar ni reiniciar KWin.

## Precedencia por plataforma

- macOS: conserva `MacOsActiveApplication` y el provider nativo existente.
- Linux X11, incluido GNOME X11: conserva `X11ActiveApplication` y EWMH.
- GNOME Wayland: conserva la integración GNOME cuando está conectada y el
  orden actual de fallback nativo/XWayland.
- KDE Plasma Wayland: el snapshot KWin conectado es autoritativo. Si está
  operativo pero no hay identificador, no se reutiliza un valor XWayland
  anterior. Si está ausente o el usuario no lo habilitó, se conserva el
  fallback Wayland/XWayland existente.
- Otras sesiones Wayland: no seleccionan el bridge KWin.

La decisión se integra en el probe/cache ya compartido. No se agrega otro
watcher ni otra ruta de persistencia.

## Enriquecimiento y privacidad

El identificador KWin se entrega antes de `PrivacyGate`; una captura
blacklisteada se rechaza antes de consultar el provider o escribir iconos.
Tras una captura permitida, `LinuxApplicationMetadataProvider` resuelve el
nombre visible y el icono desde los archivos `.desktop`/temas locales y
persiste el PNG bajo `assets/application-icons/` según el contrato existente.
No se introduce un segundo parser, se cambia el esquema ni se escribe una
ruta absoluta como metadata.

Si no hay `.desktop`, nombre o icono local resoluble, el contenido válido
sigue capturándose y la UI conserva el fallback actual de origen desconocido.
El script KWin registra en el journal local marcadores de baja frecuencia para
confirmar que se cargó, que ClipVault aceptó un snapshot y si ese snapshot tenía
identificador o estaba vacío. Nunca registra el identificador, título, PID,
ruta ni contenido. El callback de `callDBus` deja que KWin informe errores de
transporte sin incluir valores del origen. Los diagnósticos de la aplicación
siguen reportando sólo backend, etapa y categoría de error.

## Regresiones requeridas

- Selección de probe aislada por sesión: KDE Wayland, GNOME Wayland, GNOME
  X11, Linux X11 y macOS.
- Actualización, limpieza y desconexión del snapshot; no se reutiliza una
  identidad anterior.
- Receiver valida versión, tamaño y forma de mensajes; logs metadata-only.
- Metadata `.desktop` resuelta por el provider existente, incluyendo nombre
  e icono; fallos de lookup no bloquean la captura.
- La canonicalización acepta el formato base sin extensión que documenta KWin,
  pero el receiver sigue rechazando mensajes D-Bus que no estén en el formato
  canónico validado.
- PrivacyGate/blacklist permanece antes del provider y de la creación de
  assets.
- Sin cambios en el flujo de clipboard, el controlador de drag-and-drop, los
  assets existentes o los caminos GNOME/macOS.

## Referencias técnicas

- [API de scripting de KWin](https://develop.kde.org/docs/plasma/kwin/api/)
- [Tutorial oficial de scripts y paquetes KWin](https://develop.kde.org/docs/plasma/kwin/)
- [Código de KWin 6.7.4: carga, descarga y reconfiguración de scripts](https://sources.debian.org/src/kwin-x11/4%3A6.7.4-1/src/scripting/scripting.cpp/)
- [zbus 5.13.2: historial de MSRV y cambios](https://docs.rs/crate/zbus/5.14.0/source/CHANGELOG.md)
- [zbus: API de servicio, sesión y Tokio](https://docs.rs/zbus/latest/zbus/)
- [dbus-rs: requisitos de libdbus](https://github.com/diwic/dbus-rs#requirements)
