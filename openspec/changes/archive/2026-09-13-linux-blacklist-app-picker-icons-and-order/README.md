# linux-blacklist-app-picker-icons-and-order

Orden alfabético e iconos locales en el selector visual Linux de Privacidad

Este cambio corrige dos defectos observados durante la prueba manual en Linux:
el catálogo de aplicaciones debe ordenarse por el nombre visible y cada
aplicación debe mostrar su icono local cuando exista. El alcance incluye X11,
XWayland y Wayland con Desktop File ID, reutilizando el flujo de iconos de
capturas de Wayland y sin modificar la semántica de la blacklist.
