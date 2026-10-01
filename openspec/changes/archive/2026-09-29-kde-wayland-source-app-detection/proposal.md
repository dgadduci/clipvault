# Propuesta: identificar la aplicación de origen en KDE Wayland

## Why

En Arch Linux con KDE Plasma Wayland, ClipVault captura el portapapeles, pero
las entradas no reciben el origen de la aplicación; por eso no se muestran su
nombre ni su icono. Sin un identificador de origen, el
`LinuxApplicationMetadataProvider` existente no puede buscar la aplicación en
los metadatos `.desktop` locales.

La detección Linux actual combina EWMH para X11/XWayland con sondas de
protocolos públicos Wayland. La lista pública de toplevels no garantiza
información de foco en todos los compositores. La propuesta activa
`linux-native-wayland-app-detection` también deja explícito ese límite.

La prueba manual del 2026-09-28 reportó que sólo Warp muestra nombre e icono
en Arch/KDE/Wayland. La canonicalización ya acepta el nombre base, pero la
prueba manual posterior reportó que no aparece nombre ni icono para ninguna
aplicación y que el estado del bridge sigue en `awaiting_consent` pese al
consentimiento y paquete activos. La revisión del flujo encontró que el script
enviaba una cadena de firma `"susu"` como argumento D-Bus; KWin `callDBus` no
recibe una firma explícita, por lo que el receiver nunca aceptaba el mensaje.
El paquete también estaba declarado como script declarativo QML aunque el
código de entrada es JavaScript. El cambio corregirá el punto de entrada y
enviará argumentos D-Bus de tipo string, y actualizará el paquete instalado al
reactivar una integración ya consentida. Una prueba posterior aún no mostró
origen en ninguna aplicación y reportó `activation_pending`, con consentimiento,
paquete instalado y paquete habilitado. Esto indica que el bridge ya está
esperando, pero no recibe `Publish`. KWin conserva en memoria el script cuyo ID
ya está cargado aunque se actualicen sus archivos y se llame a `reconfigure`;
la integración descargará su propio script con el API de scripting D-Bus de
KWin antes de reconfigurar, para que se cargue la versión JavaScript instalada.
La actualización también debe retirar el antiguo `contents/ui/main.qml` del
paquete propio: versiones anteriores lo usaban como entrada declarativa y el
instalador hasta ahora dejaba ese archivo junto al nuevo punto de entrada JS.

La sesión Arch/KDE/Wayland confirmó que el script no está cargado aunque el
panel informara que el paquete estaba habilitado. La inspección de `kwinrc`
encontró que la clave se escribía como `Plugins/<id>Enabled` dentro del grupo
`[Plugins]`. KWin espera `<id>Enabled` dentro de ese grupo; el prefijo hacía que
KWin ignorase la activación. La corrección escribirá la clave con el formato
KConfig correcto y migrará sólo la clave antigua de ClipVault, conservando su
valor habilitado/deshabilitado y las demás claves.

## What Changes

- Obtener en KDE Plasma Wayland un identificador estable de la ventana activa
  cuando KWin lo publique, canonizando el nombre base que KWin documenta como
  un Desktop File ID terminado en `.desktop`.
- Usar ese identificador en el `ActiveApplicationProbe` existente y el
  pipeline compartido de captura.
- Reutilizar el provider Linux existente para resolver nombre e icono desde
  archivos `.desktop` y persistir referencias locales controladas.
- Mantener sin cambios la captura y la detección de origen en macOS, Linux
  GNOME X11 y Linux GNOME Wayland.
- Mantener la captura usable si KWin, el identificador o la metadata local no
  están disponibles.

## Alcance

La integración nueva se limita a una sesión KDE Plasma Wayland compatible y
al origen de la aplicación. La metadata se limita al identificador de la
aplicación activa, su nombre visible local y el icono local resuelto por el
provider existente. La integración KWin debe ser local, opcional,
consentida, reversible y metadata-only.

La selección del probe debe quedar condicionada a KDE Wayland. GNOME Wayland
conserva la integración GNOME existente y GNOME X11 conserva EWMH. macOS
conserva sus probes nativos.

## Decisión de arquitectura propuesta

El adapter será un script KWin JavaScript de usuario que publique el `desktopFileName`
de la ventana activa mediante un servicio D-Bus de sesión en ClipVault. El
bridge Rust usará `zbus` `~5.13.2`, sólo en Linux y detrás de una feature
opcional, con la feature Tokio y sin defaults. Ese rango respeta el MSRV 1.85
del workspace; versiones posteriores de `zbus` elevan el MSRV. `dbus` no se
elige porque su binding usa libdbus y añade una dependencia nativa de build y
runtime para esta integración.

La instalación será un KPackage bajo el directorio de datos del usuario. Con
consentimiento, ClipVault cambiará únicamente la clave KWin del plugin propio
y solicitará reconfiguración por D-Bus. Debe validar en el runtime objetivo
que instalación, activación, desactivación y eliminación sean reversibles y
que el script publique el foco actual al reconectarse el servicio. Si alguna
condición falla, se actualiza el diseño antes de iniciar la implementación.

La canonicalización de `desktopFileName` se limita a eliminar una ruta que
KWin haya reportado, conservar o añadir el sufijo `.desktop` y validar el
basename con el alfabeto existente. No se usan alias por ejecutable, título,
PID ni procesos para resolver una aplicación.

Al iniciar o reintentar una integración aceptada, ClipVault actualizará el
contenido del KPackage propio antes de pedir a KWin que lo recargue. El bridge
usa la versión de protocolo 2: el método transporta el identificador, el
código de estado y la versión como argumentos D-Bus string, porque el API
`callDBus` de KWin infiere los tipos de los argumentos y no acepta una cadena
de firma independiente.

## Fuera de alcance

- Cambiar la lectura, persistencia, formatos o deduplicación del clipboard.
- Inferir el origen desde título de ventana, PID, proceso, contenido copiado,
  orden de ventanas o heurísticas de nombre.
- Cambiar GNOME Shell, el adapter X11 o los adapters de macOS.
- Garantizar soporte para todos los compositores Wayland o todas las
  versiones de Plasma sin validación explícita.
- Instalar scripts globales, ejecutar comandos externos como requisito de
  runtime, enviar datos fuera del equipo o añadir telemetría.
- Rediseñar las cards, sus acciones o su drag-and-drop.

## Capacidades afectadas

- `desktop-platform-integration`: probe local de aplicación activa para KDE
  Plasma Wayland y precedencia aislada por sesión.
- `clipboard-history-cards`: enriquecimiento existente de `source_app` con
  nombre e icono local.
