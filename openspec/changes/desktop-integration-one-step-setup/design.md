# Diseño: activación sencilla de integraciones Wayland

## Recorrido objetivo

La persona abre Configuración → Integraciones de escritorio, lee una
explicación corta y entiende qué autoriza. Pulsa una vez **Configurar**. Esa
acción guarda el consentimiento y completa las operaciones locales que ya
están soportadas. La vista permanece abierta y verifica el resultado.

El consentimiento debe aparecer antes de escribir o activar la extensión. La
explicación GNOME confirma que ClipVault comunica únicamente el identificador
de la aplicación enfocada, no lee ni envía el contenido del portapapeles, y
que la integración se puede desactivar o retirar.

## GNOME Wayland

- Un único evento explícito de la persona inicia el guardado del
  consentimiento y la instalación local incluida en ClipVault.
- El servicio de integración coordina ambas operaciones; el comando Tauri
  continúa siendo un adaptador delgado y devuelve un resultado tipado.
- La instalación es idempotente y conserva las garantías atómicas y de
  propiedad del recurso de `gnome-wayland-integration`.
- Usar solo interfaces públicas soportadas para activar o abrir la gestión de
  extensiones. No usar `org.gnome.Shell.Eval`, cambios de configuración del
  sistema, ejecución como root ni comandos arbitrarios.
- Si la versión de GNOME necesita que la persona active la extensión o cierre
  y vuelva a iniciar sesión, mostrar solo esos pasos y un botón para abrir
  Extensiones cuando exista un destino conocido. No presentar la integración
  como lista hasta recibir el estado conectado.
- Al volver a abrir ClipVault, consultar el estado automáticamente. Si la
  extensión sigue pendiente, mostrar la acción pendiente y conservar
  disponible **Reintentar**.

## KDE Plasma Wayland

- Conservar una acción conjunta de consentimiento e instalación/activación.
- Reutilizar el ciclo de vida KWin existente, incluyendo su mecanismo de
  recarga cuando esté disponible.
- Mostrar éxito solo después de observar el estado habilitado y la conexión
  confirmada. Si la activación falla, mantener el estado anterior y ofrecer
  reintentar o desactivar.

## Fallos y reversibilidad

Si el consentimiento se guarda pero la instalación falla, explicar que la
integración aún no está activa y permitir reintentar sin pedir de nuevo una
decisión ya guardada. Un rechazo no instala ni habilita componentes. La
desactivación y desinstalación siguen siendo acciones explícitas y reversibles;
no se actualiza el recurso silenciosamente.

## Texto y privacidad

Las descripciones deben nombrar únicamente los datos mínimos y el alcance de
la operación. Estados, instrucciones exactas para completar el paso manual,
errores, botones y nombres accesibles se traducen en los cinco catálogos.
Ningún texto de producto puede quedar escrito directamente en Svelte o Rust.
