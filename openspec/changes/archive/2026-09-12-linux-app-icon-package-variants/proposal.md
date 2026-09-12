# Propuesta: iconos Linux para variantes de empaquetado

## Evidencia

La verificación manual reportó que X11 y Wayland reconocen la fuente de la
captura y muestran la mayoría de los iconos, pero Firefox y xterm quedan con
el fallback.

La inspección local del host reproduce dos formas distintas del mismo
problema:

- Firefox Snap publica `firefox_firefox.desktop`. La entrada local
  `/var/lib/snapd/desktop/applications/firefox_firefox.desktop` declara
  `Icon=/snap/firefox/current/default256.png`. El provider actual encuentra
  la entrada, pero rechaza esa ruta absoluta porque sólo permite raíces XDG
  de iconos.
- xterm está instalado como `debian-xterm.desktop` y `debian-uxterm.desktop`.
  Esas entradas no declaran `StartupWMClass` ni `X-GNOME-WMClass`; declaran
  `Exec=xterm`/`Exec=uxterm` e `Icon=mini.xterm`. Un identificador de ventana
  `xterm` no coincide con el filename `debian-xterm` y por tanto el provider
  no llega a resolver el icono de tema.

El elemento común no es el transporte de captura: en ambos casos el
empaquetado rompe la suposición de una relación uno-a-uno entre identidad de
ventana, filename `.desktop` e icono dentro de `XDG_DATA_DIRS`. La corrección
debe ser genérica para variantes de distribución y no un hardcode de Firefox
o xterm.

## Objetivo

1. Resolver aliases locales exactos de una entrada `.desktop` sin mutar ni
   reemplazar `source_app`.
2. Usar el basename del primer ejecutable de `Exec=` sólo como alias de
   metadata, sin ejecutar comandos, expandir shell ni enviar el valor por
   IPC.
3. Permitir iconos absolutos únicamente dentro de raíces locales de paquetes
   explícitamente confiables, incluyendo el layout Snap observado, sin abrir
   `/`, `/tmp` ni el home completo.
4. Mantener la resolución PNG/SVG, el namespace
   `assets/application-icons/`, el bridge existente y el fallback actual.
5. Exponer un diagnóstico estable para distinguir `exec_basename` de los
   matchers existentes y separar una ruta absoluta fuera de una raíz permitida
   de una ausencia de archivo.

## Alcance

- `LinuxApplicationMetadataProvider` y sus fixtures de filesystem.
- El modelo de diagnóstico `MatchStrategy` y sus strings estables.
- Parser local mínimo de `.desktop` para obtener un alias ejecutable seguro.
- Allowlist local de raíces de iconos de paquetes, con canonicalización,
  comprobación de archivo regular y rechazo de symlinks que escapen.
- Tests unitarios/integración del provider y una prueba manual X11, XWayland y
  GNOME Wayland para Firefox empaquetado y xterm.
- Especificación compartida de que Desktop y Quick Paste reutilizan el bridge
  actual sin cambios de DTO ni de layout.

## Fuera de alcance

- No ejecutar, parsear dinámicamente ni validar disponibilidad de ningún
  comando indicado por `Exec=`.
- No usar títulos, PID, `/proc`, D-Bus, ventanas enumeradas, red, telemetría,
  iconos enviados por GNOME Shell ni rutas transportadas por IPC.
- No hacer matching por prefijo, substring, similitud, nombre visible o
  eliminación arbitraria de sufijos.
- No hardcodear reglas para una sola aplicación, no renombrar assets
  existentes y no cambiar `source_app`, PrivacyGate, SQLite, cards, Quick
  Paste o drag-and-drop.
- No agregar dependencias nuevas ni cambiar el versionado hasta implementar y
  verificar el cambio funcional.

## Criterio de éxito

En los fixtures equivalentes a Firefox Snap, el provider conserva
`firefox_firefox.desktop`, resuelve `Name=Firefox` y persiste un PNG desde la
raíz de paquete permitida. Una ruta absoluta fuera de las raíces permitidas
sigue produciendo fallback sin crear asset.

## Decisión de cierre

La aceptación manual de esta entrega se limita explícitamente a Firefox Snap:
en la sesión Linux probada se detectaron su nombre e icono. xterm no mostró
icono y conserva el fallback; esta limitación se acepta como decisión de
producto y no se continuará trabajando en ella dentro de este cambio. La
cobertura automatizada de `ExecBasename` se conserva como regresión del
provider, pero no constituye un criterio de aceptación manual de esta
entrega.

Referencias: [Desktop Entry Specification](https://specifications.freedesktop.org/desktop-entry/latest-single/),
[Shell.App.get_id](https://gnome.pages.gitlab.gnome.org/gnome-shell/shell/method.App.get_id.html)
y [Icon Naming Specification](https://specifications.freedesktop.org/icon-naming/latest/).
