# Diseño: origen de capturas en KDE Plasma Wayland

## Diagnóstico y límite de la solución existente

El cambio debe empezar separando dos casos: `source_app` llega vacío, o el
identificador llega pero no encuentra un `.desktop`/icono local. Los
diagnósticos no deben exponer el identificador, el título, el PID, rutas ni
contenido del clipboard.

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
   aplicación de KWin; no se lee ni se transmite el título de ventana.
3. Valida y acota el valor antes de publicarlo. Si KWin lo entrega como ruta
   absoluta, sólo puede conservarse el nombre de archivo `.desktop`; nunca se
   persiste ni se informa la ruta. Valores vacíos, inválidos o sin una
   identidad resoluble producen estado de origen desconocido.
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
`PublishActiveApplication` lleva versión y un identificador acotado; un
mensaje explícito de estado vacío borra el snapshot. Se rechazan cadenas
vacías, controles y valores con forma de ruta después de normalizar únicamente
el nombre de archivo permitido. Los errores no registran el argumento.

### Paquete y ciclo de vida

El script se instala como KPackage dentro del directorio KWin del usuario,
bajo un identificador reservado para ClipVault. Con consentimiento explícito,
ClipVault activa sólo su clave `Plugins/<id>Enabled` en la configuración KWin
del usuario y pide a KWin reconfigurar mediante el bus de sesión. No ejecuta
`kwriteconfig`, `qdbus`, `kpackagetool` ni otro proceso externo. Las
operaciones deben conservar el resto de `kwinrc`, ser atómicas e idempotentes,
y rechazar un directorio de paquete o una clave del plugin que pertenezca a
otro recurso.

La reconfiguración usa el método documentado `org.kde.KWin /KWin
org.kde.KWin.reconfigure`, invocado directamente con `zbus`. El adapter de
instalación escribe la clave y el paquete; el lifecycle de Tauri ordena la
llamada D-Bus y sólo cambia el probe compartido cuando el receiver está
registrado.

Desactivar cambia sólo la clave del plugin propio a deshabilitado y confirma
el estado con KWin; desinstalar además retira sólo el directorio de paquete
propio una vez desactivado. Si ya existe un recurso con el mismo identificador
que no puede atribuirse a ClipVault, no se sobrescribe ni elimina. Los estados
visibles distinguen consentimiento, instalación, activación, conexión y
causa tipada de indisponibilidad.

Antes de implementar el bridge debe probarse en el runtime Arch/KDE objetivo
que: a) el método de reconfiguración carga y descarga el paquete de usuario;
b) los cambios de configuración persisten sin tocar claves ajenas; c) la
activación vuelve a publicar `workspace.activeWindow` si ClipVault inicia
después de KWin; y d) desactivar/desinstalar deja la sesión y los scripts
ajenos intactos. Si se necesita una API privada adicional o no se puede
garantizar el snapshot inicial, se pausa la fase 2 y se actualiza este diseño.

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
Los diagnósticos reportan backend, etapa y categoría de error, sin valores de
origen ni rutas.

## Regresiones requeridas

- Selección de probe aislada por sesión: KDE Wayland, GNOME Wayland, GNOME
  X11, Linux X11 y macOS.
- Actualización, limpieza y desconexión del snapshot; no se reutiliza una
  identidad anterior.
- Receiver valida versión, tamaño y forma de mensajes; logs metadata-only.
- Metadata `.desktop` resuelta por el provider existente, incluyendo nombre
  e icono; fallos de lookup no bloquean la captura.
- PrivacyGate/blacklist permanece antes del provider y de la creación de
  assets.
- Sin cambios en el flujo de clipboard, el controlador de drag-and-drop, los
  assets existentes o los caminos GNOME/macOS.

## Referencias técnicas

- [API de scripting de KWin](https://develop.kde.org/docs/plasma/kwin/api/)
- [Tutorial oficial de scripts y paquetes KWin](https://develop.kde.org/docs/plasma/kwin/)
- [zbus 5.13.2: historial de MSRV y cambios](https://docs.rs/crate/zbus/5.14.0/source/CHANGELOG.md)
- [zbus: API de servicio, sesión y Tokio](https://docs.rs/zbus/latest/zbus/)
- [dbus-rs: requisitos de libdbus](https://github.com/diwic/dbus-rs#requirements)
