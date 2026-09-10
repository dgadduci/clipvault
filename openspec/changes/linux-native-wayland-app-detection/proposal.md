# Propuesta: detección de aplicaciones nativas Wayland

## Problema

En una sesión GNOME Wayland, ClipVault puede identificar aplicaciones XWayland mediante la ventana X11 auxiliar, pero las aplicaciones nativas Wayland no publican `WM_CLASS`. Por eso capturas provenientes de aplicaciones como Terminal, Firefox o Chrome pueden quedar sin `source_app`, nombre e icono aunque el resto del pipeline funcione.

El comportamiento actual es correcto para una sesión Wayland que no ofrece una fuente pública de identidad: el sistema no debe inventar un identificador ni usar el título de la ventana como nombre de aplicación. Sin embargo, cuando el compositor ofrece una lista pública de toplevels Wayland, ClipVault puede obtener el `app_id` sin depender de APIs privadas.

## Objetivo

Agregar una sonda nativa Wayland que:

1. obtenga el `app_id` del toplevel activo mediante un protocolo público;
2. lo entregue como identificador estable al pipeline de captura;
3. reutilice `LinuxApplicationMetadataProvider` para resolver nombre e icono desde archivos `.desktop`;
4. respete PrivacyGate antes de persistir filas o crear assets;
5. mantenga intactos X11, XWayland, macOS, imágenes, tags, colecciones, favoritos, Quick Paste y drag-and-drop.

## Decisiones acordadas

- La implementación debe preferir `ext-foreign-toplevel-list-v1`, usando `zwlr_foreign_toplevel_management_unstable_v1` como fallback para compositores wlroots cuando corresponda.
- Estos protocolos no son universales ni necesariamente habilitados por el compositor. La ausencia, rechazo o indisponibilidad se informa como `Unavailable`, sin bloquear el loop ni usar fuentes privadas.
- En Wayland, la sonda nativa es la fuente autoritativa cuando está operativa. No se debe usar una respuesta XWayland vieja si la sonda nativa informó que no hay toplevel activo.
- El `app_id` es metadata de la aplicación y se trata con el mismo nivel de confianza que `WM_CLASS`; no se ejecuta ningún proceso con él.
- El nombre e icono se resuelven por el provider Linux ya existente. No se duplica el parser `.desktop`, la búsqueda XDG ni el almacenamiento de iconos PNG.
- No se usarán `org.gnome.Shell.Eval`, extensiones GNOME obligatorias, `wmctrl`, `xprop`, scraping de títulos, `/proc`, enumeración de procesos ni procesos externos.

## Fuera de alcance

- Garantizar detección en todos los compositores Wayland.
- Instalar extensiones GNOME o cambiar la configuración del compositor.
- Inferir la aplicación desde el título de ventana, PID, ruta de proceso o contenido del portapapeles.
- Rasterizar iconos mediante procesos externos.
- Cambiar el modelo de `EntryRecord`, el comportamiento de blacklist o la UI principal salvo los diagnósticos necesarios.

## Resultado esperado

En un compositor compatible, una captura de una aplicación Wayland nativa conserva `source_app`, y el flujo existente completa `source_app_name` y `source_app_icon_ref`. En un compositor no compatible, la captura sigue siendo válida pero permanece con origen desconocido según el contrato vigente.

## Referencias técnicas

- [ext-foreign-toplevel-list-v1](https://wayland.app/protocols/ext-foreign-toplevel-list-v1)
- [wlr-foreign-toplevel-management-unstable-v1](https://wayland.app/protocols/wlr-foreign-toplevel-management-unstable-v1)
- [XML del protocolo ext-foreign-toplevel-list-v1](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/staging/ext-foreign-toplevel-list/ext-foreign-toplevel-list-v1.xml)
- [XWayland](https://wayland.freedesktop.org/docs/book/Xwayland.html)
