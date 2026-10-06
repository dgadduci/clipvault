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
- Después de instalar, intentar habilitar la extensión mediante la interfaz
  pública `gnome-extensions enable` con el UUID constante de ClipVault. El
  proceso se invoca directamente, sin shell ni argumentos aportados por el
  usuario. El éxito de ese comando no equivale a una conexión confirmada.
- La conexión local del listener sigue siendo la única señal de que GNOME
  cargó la extensión. Mientras no llegue el handshake, la interfaz conserva
  el estado pendiente y no muestra éxito.
- La instalación es idempotente y conserva las garantías atómicas y de
  propiedad del recurso de `gnome-wayland-integration`.
- Si `gnome-extensions` no existe o no puede habilitar la extensión, conservar
  el consentimiento y el recurso instalado. La persona puede reintentar sin
  aceptar de nuevo.
- Mostrar **Abrir Extensiones** solo si existe el ejecutable conocido
  `/usr/bin/gnome-extensions-app`; abrirlo directamente y sin aceptar rutas o
  comandos de la interfaz. Si no está disponible o no se puede iniciar,
  ofrecer instrucciones localizadas para encontrar Extensiones desde
  Actividades.
- No usar `org.gnome.Shell.Eval`, cambios de configuración del sistema,
  ejecución como root ni comandos arbitrarios.
- Si la versión de GNOME necesita que la persona active la extensión o cierre
  y vuelva a iniciar sesión, mostrar solo esos pasos y un botón para abrir
  Extensiones cuando exista un destino conocido. No presentar la integración
  como lista hasta recibir el estado conectado.
- Un error al iniciar el listener no desinstala el recurso ni cambia el
  consentimiento a `disabled`. Al volver a abrir ClipVault se consulta el
  estado; si sigue pendiente, **Reintentar** vuelve a habilitar y preparar el
  listener sin repetir el consentimiento.

La interfaz del sistema se mantiene detrás de un adapter testeable en
`clipvault-platform`. No se agrega una dependencia: los dos ejecutables
permitidos se resuelven por rutas constantes y se comprueba su disponibilidad.

## KDE Plasma Wayland

- Conservar una acción conjunta de consentimiento e instalación/activación.
- Reutilizar el ciclo de vida KWin existente, incluyendo su mecanismo de
  recarga cuando esté disponible.
- Mostrar éxito solo después de observar el estado habilitado y la conexión
  confirmada. Si la activación falla, mantener el estado anterior y ofrecer
  reintentar o desactivar.
- Al recuperarse de un error, volver a consultar el estado para mostrar el
  consentimiento persistido y permitir reintentar sin volver a pedirlo.

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

## Referencias de interfaces soportadas

- El manual de GNOME Shell 42.9 documenta `gnome-extensions enable UUID` y
  señala que el estado debe confirmarse aparte con `info`:
  <https://manpages.ubuntu.com/manpages/jammy/man1/gnome-extensions.1.html>.
- La aplicación oficial GNOME Extensions administra las extensiones y admite
  GNOME 3.36 o posterior: <https://apps.gnome.org/Extensions/>.
- Ubuntu Jammy y Arch Linux publican el ejecutable y el desktop entry de
  Extensions en sus paquetes; la implementación solo los abre cuando está
  presente el ejecutable conocido:
  <https://packages.ubuntu.com/jammy/arm64/gnome-shell-extension-prefs/filelist>
  y <https://archlinux.org/packages/extra/x86_64/gnome-shell/files/>.
