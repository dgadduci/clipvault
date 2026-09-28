# Propuesta: identificar la aplicación de origen en KDE Wayland

## Problema

En Arch Linux con KDE Plasma Wayland, ClipVault captura el portapapeles, pero
las entradas no reciben el origen de la aplicación; por eso no se muestran su
nombre ni su icono. Sin un identificador de origen, el
`LinuxApplicationMetadataProvider` existente no puede buscar la aplicación en
los metadatos `.desktop` locales.

La detección Linux actual combina EWMH para X11/XWayland con sondas de
protocolos públicos Wayland. La lista pública de toplevels no garantiza
información de foco en todos los compositores. La propuesta activa
`linux-native-wayland-app-detection` también deja explícito ese límite. Antes
de implementar, hay que confirmar en el runtime KDE si falta `source_app` o si
el identificador ya llega y falla únicamente el enriquecimiento de metadata.

## Objetivos

- Obtener en KDE Plasma Wayland un identificador estable de la ventana activa
  cuando KWin lo publique.
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

El adapter será un script KWin de usuario que publique el `desktopFileName`
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
