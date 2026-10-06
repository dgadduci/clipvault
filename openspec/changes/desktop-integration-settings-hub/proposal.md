# Propuesta: configuración accesible de integraciones de escritorio

## Por qué

Las integraciones de GNOME y KDE Wayland están agrupadas hoy bajo
**Development**, junto a diagnósticos y controles técnicos. Una persona que
solo quiere usar ClipVault difícilmente buscará allí cómo habilitar una
función. Además, las pantallas actuales exponen estados y valores técnicos que
no ayudan a completar la configuración.

## Cambios

- Agregar **Integraciones de escritorio** a Configuración general en Linux
  cuando la sesión detectada tenga una integración compatible y aplicable.
- Reubicar allí la configuración de GNOME y KDE Plasma Wayland. Conservar en
  Development los diagnósticos y acciones de desarrollo.
- Presentar el estado en lenguaje cotidiano, con la siguiente acción clara y
  las acciones existentes de reintentar, desactivar o desinstalar cuando
  correspondan.
- Mantener intactos los servicios de integración, su consentimiento y sus
  capacidades. Esta etapa cambia el acceso y la presentación, no su activación.
- Añadir las etiquetas, instrucciones, estados, errores y nombres accesibles a
  `en`, `es`, `pt`, `de` y `fr`.

## Capacidades

- Nueva: `desktop-integration-settings` — acceso y presentación de los
  controles de integración desde Configuración general.

## Reconciliación con cambios activos

Se revisaron `keyboard-shortcuts`, `desktop-toolbar-layout` y
`gnome-wayland-integration` antes de tocar los componentes. Esta implementación
conserva el editor de atajos, los callbacks de la barra y la entrada de
configuración GNOME desde su diagnóstico. Los cambios activos quedan sin
archivar porque sus verificaciones pendientes exceden este alcance.

## Impacto

- `app/tauri/frontend`: entrada de Configuración general y vista de
  Integraciones de escritorio; retiro de controles de usuario de Development.
- Catálogos de idioma: textos completos en los cinco idiomas disponibles.
- No se agregan comandos, dependencias, permisos ni accesos de red.
