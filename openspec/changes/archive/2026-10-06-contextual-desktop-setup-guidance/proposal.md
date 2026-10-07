# Propuesta: guía contextual de configuración

## Why

Una integración opcional es difícil de descubrir si no se necesita para abrir
ClipVault, pero un asistente obligatorio de primer inicio añade pasos incluso
para personas que no necesitan configurar nada. La aplicación ya detecta el
sistema, el servidor gráfico y los estados de sus integraciones; puede mostrar
una ayuda breve solo donde resulte útil.

## What Changes

- En GNOME Wayland y KDE Plasma Wayland, mostrar una tarjeta pequeña y
  descartable en la ventana principal cuando la integración aplicable aún no
  fue configurada.
- Mostrarla cuando la ventana esté lista, sin retrasar el inicio, abrir un
  diálogo automáticamente ni bloquear las funciones disponibles.
- Ofrecer **Configurar** para ir directamente a la integración detectada y
  **Ahora no** para ocultar el aviso. El rechazo explícito del consentimiento
  y el cierre del aviso son decisiones distintas.
- Recordar localmente que la tarjeta fue descartada y mantener siempre el
  acceso posterior en Configuración → Integraciones de escritorio.
- Mantener la ayuda de macOS ligada al intento de pegar que requiere
  Accesibilidad; no pedir ese permiso como requisito de primera ejecución.
- No mostrar guías de instalación en Linux X11 ni en escritorios Wayland sin
  una integración compatible. Mantener para esas sesiones las explicaciones
  de capacidad que aparecen cuando se intenta una función no disponible.
- Localizar toda tarjeta, paso, acción y estado en `en`, `es`, `pt`, `de` y
  `fr`.

## Capacidades

- Nueva: `contextual-desktop-setup-guidance` — aviso opcional y contextual
  para los dos entornos Wayland con integración soportada.
- Modifica: `desktop-integration-settings` — la tarjeta conduce a la
  configuración permanente.

## Orden de implementación

Depende de `desktop-integration-settings-hub` y
`desktop-integration-one-step-setup`. Debe reutilizar sus estados y acciones,
además de respetar la guía ya definida en `platform-permission-guidance`.

## Impacto

- `app/tauri/frontend`: tarjeta no modal que se oculta tras una decisión
  local y abre la integración correspondiente.
- `app_settings`: persistencia local y por integración de la decisión de
  ocultar la tarjeta, sin migración de tablas si el almacén de preferencias lo
  permite.
- No se agrega un asistente obligatorio, servicio de red ni permiso nuevo.
