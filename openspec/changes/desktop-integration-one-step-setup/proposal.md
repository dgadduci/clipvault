# Propuesta: activación sencilla de integraciones Wayland

## Por qué

La configuración GNOME actual pide primero aceptar el consentimiento y luego
volver a pulsar para instalar la extensión. En KDE la persona ya tiene una
acción conjunta para consentir y activar. Las diferencias no responden a una
necesidad del usuario y hacen que GNOME parezca más difícil de lo que es.

## Cambios

- Combinar el consentimiento explícito y la instalación local de GNOME en una
  sola acción **Configurar integración**.
- Intentar habilitar la extensión GNOME con la herramienta oficial del
  escritorio y confirmar el resultado mediante la conexión local; conservar
  una guía manual cuando la sesión todavía requiera intervención.
- Mantener en KDE una sola acción de consentimiento y activación, aclarando
  qué se instala y confirmando el estado real de KWin al terminar.
- Tras la acción, mostrar progreso, resultado y el único paso manual que el
  entorno todavía requiera. Si GNOME requiere cerrar sesión y volver a
  entrar, indicarlo exactamente y comprobar la conexión al regresar.
- Dar acceso a abrir la pantalla de extensiones GNOME cuando el entorno tenga
  un destino conocido y seguro; si no, presentar la navegación manual breve.
- Mantener visible cómo desactivar o retirar la integración y cambiar una
  decisión anterior.
- Localizar todos los textos de consentimiento, progreso, estados, acciones y
  errores en `en`, `es`, `pt`, `de` y `fr`.

## Capacidades

- Nueva: `desktop-integration-activation` — define el recorrido consentido de
  activación que se ofrece desde `desktop-integration-settings-hub`.

## Orden de implementación

Depende de `desktop-integration-settings-hub`. Antes de implementar, confirmar
el estado final de `gnome-wayland-integration` y la integración KDE vigente,
incluyendo sus APIs soportadas para habilitación y recarga. No se debe usar
una API privada o un mecanismo que eluda el consentimiento para ahorrar un
paso.

## Impacto

- Servicios de instalación y activación GNOME/KDE, adaptadores Tauri y vista
  Integraciones de escritorio.
- No cambia qué información publica cada integración ni las limitaciones de
  pegado sintético de Wayland.
- No añade descargas, privilegios de administrador, telemetría ni
  dependencias.
