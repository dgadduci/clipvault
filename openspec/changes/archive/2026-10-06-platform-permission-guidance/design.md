# Design: platform-permission-guidance

## Context

desktop-platform-integration ya expone una matriz de capacidades y
PasteOutcome::CapabilityUnavailable, pero el resultado actual no comunica
por qué la capacidad no está disponible. En macOS, CGEvent puede requerir
Accesibilidad para que el evento de teclado llegue a otra aplicación. En Linux
X11, el problema suele ser el display o la extensión XTest; en Wayland, el
pegado sintético puede no tener una API portable y no debe interpretarse como
un permiso faltante.

## Diagnóstico tipado

El platform layer debe exponer tipos serializables y estables:

    pub enum PlatformIssueKind {
        PermissionRequired,
        UnsupportedSession,
        BackendUnavailable,
        Unknown,
    }

    pub enum PlatformSettingsTarget {
        MacosAccessibility,
        LinuxDesktopIntegration,
    }

    pub struct PlatformGuidance {
        pub capability: String,
        pub kind: PlatformIssueKind,
        pub title: String,
        pub summary: String,
        pub steps: Vec<String>,
        pub retryable: bool,
        pub can_open_settings: bool,
        pub settings_target: Option<PlatformSettingsTarget>,
    }

Los nombres concretos pueden ajustarse al estilo existente, pero la causa no
debe viajar como texto libre únicamente. La respuesta de paste_entry debe
incluir guidance: Option<PlatformGuidance> cuando el resultado sea una
capacidad no disponible o un error de plataforma accionable.

El objeto no debe contener el texto copiado, snippets, hashes, tokens ni
credenciales.

## macOS: Accesibilidad

Antes de anunciar que synthetic_paste está disponible, el adapter macOS debe
hacer un preflight de publicación de eventos de entrada mediante la API nativa
apropiada (CGPreflightPostEventAccess, AXIsProcessTrusted o la equivalente
compatible con la versión de los crates utilizados).

Si el permiso falta:

- paste_entry devuelve CapabilityUnavailable con
  PlatformIssueKind::PermissionRequired.
- La guía explica que ClipVault necesita permiso de Accesibilidad para enviar
  Cmd+V a la aplicación activa.
- Los pasos incluyen abrir Ajustes del Sistema, ir a Privacidad y seguridad →
  Accesibilidad, habilitar ClipVault o el proceso que lo ejecuta en modo
  desarrollo, volver a ClipVault y presionar Reintentar.
- settings_target es MacosAccessibility; el frontend no construye URLs
  arbitrarias.

La apertura de settings debe estar detrás de un SettingsNavigator. Debe
intentar el deep link de preferencias de macOS y, si falla, abrir la aplicación
de Ajustes del Sistema o devolver un resultado tipado con los pasos manuales.
No se debe ejecutar una URL recibida desde el frontend sin validación.

## Linux: permisos, backend y sesión

La guía Linux debe distinguir explícitamente estos casos.

### Linux X11

X11 no tiene un permiso de Accesibilidad equivalente al de macOS para este
flujo. Si falla la conexión al display, falta XTest o el backend está aislado
por el método de empaquetado, la causa debe ser BackendUnavailable o
PermissionRequired solo si el adapter puede demostrar una restricción de
permisos del sandbox.

Los pasos deben poder incluir, según el diagnóstico:

1. Confirmar que la sesión es X11 y que la aplicación se está ejecutando dentro
   de la sesión gráfica correcta.
2. Confirmar que el display y la extensión XTest están disponibles.
3. Si la aplicación está empaquetada con Flatpak, Snap u otro sandbox,
   consultar y habilitar los permisos de integración de escritorio que el
   paquete declare, sin inventar permisos universales.
4. Volver a ClipVault y presionar Reintentar.

Cuando el entorno de escritorio tenga un panel conocido y confiable, el
SettingsNavigator puede abrirlo mediante un destino interno como
LinuxDesktopIntegration. Si no hay un destino portable, la modal debe mostrar
los pasos manuales y ocultar Abrir configuración.

### Linux Wayland

Si no existe un mecanismo soportado para pegado sintético:

- la causa es UnsupportedSession, no PermissionRequired;
- la ventana explica que no se trata de un permiso de macOS ni de un botón
  que pueda habilitarse universalmente;
- no se muestra Abrir configuración salvo que se conozca un mecanismo
  específico y seguro del compositor;
- se indican alternativas honestas, como usar una sesión X11 o configurar un
  portal/protocolo de atajos y pegado compatible del compositor;
- el historial, la captura y las demás capacidades disponibles siguen
  funcionando.

### Linux sin display reconocido

La guía debe indicar que ClipVault no pudo identificar una sesión gráfica
compatible y recomendar iniciar la aplicación desde una sesión X11 o Wayland
válida. No debe sugerir otorgar permisos de macOS ni ejecutar comandos
automáticos.

## UI y flujo

Cuando una operación recibe guidance:

1. La UI abre una modal accesible, con foco inicial en el título y lectura
   clara por VoiceOver/lectores de pantalla.
2. Muestra título, explicación y pasos numerados.
3. Muestra Abrir configuración solo si can_open_settings es verdadero.
4. Abrir configuración invoca clipvault_open_platform_settings con el destino
   enum y luego mantiene disponible Reintentar.
5. Reintentar vuelve a consultar capacidades y repite la operación solo si el
   usuario lo solicita; no debe pegar dos veces automáticamente.
6. Si la apertura falla, la modal conserva los pasos manuales y muestra el
   error sin incluir contenido del clipboard.
7. Cerrar descarta la modal sin modificar ni eliminar la entrada histórica.

El mismo componente debe poder reutilizarse para futuros conflictos de hotkey,
permisos de clipboard y limitaciones de aplicación activa, aunque este cambio
debe priorizar el error de pegado en macOS y sus equivalentes Linux.

## Tauri y seguridad

Agregar un comando delgado, por ejemplo:

    clipvault_open_platform_settings { target }

El comando delega en el adapter y devuelve opened, fallback_required o failed
con guidance. El core sigue siendo responsable de los tipos y del flujo,
mientras Tauri solo adapta IPC y ventana.

La guía puede contener nombres de aplicaciones, sesiones y pasos del sistema,
pero no debe registrar el texto del clipboard ni incluirlo en eventos. No se
agregan permisos de red ni se solicitan privilegios elevados automáticamente.

## Tests

- Preflight macOS sin permiso produce PermissionRequired y destino
  MacosAccessibility.
- Preflight macOS con permiso permite el flujo de pegado.
- Error de backend se distingue de permiso faltante.
- X11 sin display/XTest genera guía específica sin mencionar macOS.
- Wayland produce UnsupportedSession sin opción falsa de abrir settings.
- Un adapter fake de settings permite probar éxito, fallback y error.
- La respuesta serializada contiene guidance sin payload del clipboard.
- La modal muestra/oculta botones según can_open_settings y permite reintentar
  sin mutar la entrada.
