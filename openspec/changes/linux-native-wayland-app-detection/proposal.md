# Propuesta: detección de aplicaciones nativas Wayland

## Problema

En una sesión GNOME Wayland, ClipVault puede identificar aplicaciones
XWayland mediante la ventana X11 auxiliar, pero las aplicaciones
nativas Wayland no publican `WM_CLASS`. Por eso capturas provenientes
de aplicaciones como Terminal, Firefox o Chrome pueden quedar sin
`source_app`, nombre e icono aunque el resto del pipeline funcione.

El comportamiento actual es correcto para una sesión Wayland que no
ofrece una fuente pública de identidad: el sistema no debe inventar
un identificador ni usar el título de la ventana como nombre de
aplicación. Sin embargo, cuando el compositor ofrece una lista
pública de toplevels Wayland, ClipVault puede obtener el `app_id` sin
depender de APIs privadas.

## Objetivo

Agregar una sonda nativa Wayland que:

1. determine el toplevel activo mediante un protocolo público que
   exponga foco (actualmente `zwlr_foreign_toplevel_management_unstable_v1`);
2. entregue el `app_id` del toplevel activo como identificador
   estable al pipeline de captura;
3. reutilice `LinuxApplicationMetadataProvider` para resolver nombre
   e icono desde archivos `.desktop`;
4. respete PrivacyGate antes de persistir filas o crear assets;
5. mantenga intactos X11, XWayland, macOS, imágenes, tags,
   colecciones, favoritos, Quick Paste y drag-and-drop.

## Decisiones acordadas

- `ext-foreign-toplevel-list-v1` **no** publica un estado de
  activación. La sonda nunca lo usa para inferir foco: ni toma el
  identificador del primer handle, ni del orden de creación, ni del
  handle con `app_id` "más reciente". Cuando ext es el único
  protocolo enlazado, la sonda devuelve `Unavailable` con causa
  `registry_without_protocol`.
- `zwlr_foreign_toplevel_management_unstable_v1` sí publica un
  array `state[]` por handle; el valor `activated == 2` indica
  foco. Este protocolo se convierte en la fuente autoritativa del
  foco de la sonda.
- Cuando el compositor anuncia ambos protocolos, la sonda enlaza
  ambos, usa zwlr para decidir el handle activo y usa el `app_id`
  del handle (ext o zwlr) que lo identifique. El backend reportado
  pasa a ser `wayland_wlr_foreign_toplevel_with_ext`. Si zwlr no
  está disponible, ext aporta metadata pero no foco → la sonda
  devuelve `Unavailable`.
- El `app_id` es metadata de la aplicación y se trata con el mismo
  nivel de confianza que `WM_CLASS`; no se ejecuta ningún proceso
  con él.
- El nombre e icono se resuelven por el provider Linux ya
  existente. No se duplica el parser `.desktop`, la búsqueda XDG
  ni el almacenamiento de iconos PNG.
- No se usarán `org.gnome.Shell.Eval`, extensiones GNOME
  obligatorias, `wmctrl`, `xprop`, scraping de títulos, `/proc`,
  enumeración de procesos ni procesos externos.
- `WAYLAND_SOCKET` (descriptor heredado por `systemd`) queda fuera
  del alcance; abrirlo como `/proc/self/fd/<fd>` o como socket
  nuevo es incorrecto y propenso a fugas del descriptor. La sonda
  usa la ruta ordinaria `$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY`.

## Fuera de alcance

- Garantizar detección en todos los compositores Wayland.
- Instalar extensiones GNOME o cambiar la configuración del
  compositor.
- Inferir la aplicación desde el título de ventana, PID, ruta de
  proceso o contenido del portapapeles.
- Rasterizar iconos mediante procesos externos.
- Cambiar el modelo de `EntryRecord`, el comportamiento de blacklist
  o la UI principal salvo los diagnósticos necesarios.
- Implementar `ext-foreign-toplevel-list-v1` como fuente autoritativa
  de foco. La sonda la usa como metadata cuando zwlr no está
  enlazado, pero no puede inferir foco a partir de ella sola.
- Implementar activación por descriptor heredado (`WAYLAND_SOCKET`).
  La sonda sólo conecta con la ruta del socket Wayland ordinaria.

## Resultado esperado

En un compositor compatible que publique
`zwlr_foreign_toplevel_management_unstable_v1` (wlroots), una
captura de una aplicación Wayland nativa conserva `source_app`, y el
flujo existente completa `source_app_name` y `source_app_icon_ref`.
En un compositor que sólo publique `ext-foreign-toplevel-list-v1`
(GNOME / KDE Plasma con configuración por defecto en algunas
versiones, KWin con `KDE_WAYLAND`) la sonda devuelve `Unavailable` y
el resto del pipeline sigue funcionando con origen desconocido según
el contrato vigente. En una sesión GNOME Wayland sin zwlr (Ubuntu
GNOME Wayland en la mayor parte de versiones) la sonda devuelve
`Unavailable`; ello **no** es un bug sino el comportamiento honesto
que el contrato exige.

## Referencias técnicas

- [ext-foreign-toplevel-list-v1](https://wayland.app/protocols/ext-foreign-toplevel-list-v1)
- [wlr-foreign-toplevel-management-unstable-v1](https://wayland.app/protocols/wlr-foreign-toplevel-management-unstable-v1)
- [XML del protocolo ext-foreign-toplevel-list-v1](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/staging/ext-foreign-toplevel-list/ext-foreign-toplevel-list-v1.xml)
- [XWayland](https://wayland.freedesktop.org/docs/book/Xwayland.html)

## Notas sobre esta versión

Esta propuesta reemplaza el contenido original de la propuesta tras
la auditoría del cambio: el wire protocol de la primera iteración
era inválido, `WAYLAND_SOCKET` se usaba de forma insegura, el
evento `toplevel` de ext no entrega `app_id`, ext no proporciona
foco y el array `state` de zwlr usa bytes como longitud. La sonda
re-escrita respeta los protocolos, las prioridades de los
compositores y el contrato de privacidad.
