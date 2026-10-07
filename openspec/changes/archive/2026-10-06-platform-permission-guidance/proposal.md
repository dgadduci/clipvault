## Why

Cuando el usuario intenta pegar una entrada y la interfaz recibe
capability_unavailable, hoy solo muestra un estado técnico como
paste: capability_unavailable. Esto no permite saber si falta un permiso de
macOS, si la sesión Linux no soporta pegado sintético o si el backend falló por
otra razón.

En macOS, el controlador que envía Cmd+V mediante CGEvent requiere que el
proceso tenga permiso de Accesibilidad. En Linux X11 normalmente no existe un
permiso equivalente, pero pueden fallar el display, XTest o el aislamiento de
un paquete. En Linux Wayland, el pegado sintético puede no tener una API
portable y la respuesta no representa algo que el usuario pueda resolver desde
un panel de permisos.

ClipVault debe distinguir estos casos y explicar qué puede hacer el usuario,
incluyendo una acción para abrir la configuración del sistema cuando exista un
destino confiable.

## What Changes

- Agregar un diagnóstico tipado de problemas de plataforma que distinga
  permission_required, unsupported_session, backend_unavailable y unknown.
- Enriquecer las respuestas de pegado y otras operaciones de plataforma con
  una guía opcional: título, explicación, pasos, reintento y destino seguro de
  configuración.
- Detectar en macOS si el proceso puede publicar eventos de entrada antes de
  informar que el pegado sintético está disponible.
- Mostrar una ventana/modal en Svelte cuando una operación falla por permisos,
  backend o limitación de plataforma.
- Agregar un botón para abrir Ajustes del Sistema de macOS en Privacidad y
  seguridad → Accesibilidad, con fallback a la aplicación de Ajustes cuando
  el deep link no pueda abrirse.
- Incluir guía Linux diferenciada para X11, Wayland y sesiones desconocidas.
  No se debe presentar como permiso una limitación estructural de Wayland.
- Agregar un botón de reintento que vuelva a consultar las capacidades sin
  reiniciar la aplicación.
- Mantener todos los comandos Tauri delgados, sin enviar contenido del
  clipboard a logs, eventos ni mensajes de error.

## Capabilities

### New Capabilities

- platform-permission-guidance: diagnóstico, guía visual y apertura segura
  de configuración para errores de integración de plataforma.

### Modified Capabilities

- desktop-platform-integration: las respuestas de capacidad y pegado pasan a
  incluir la causa y la remediación cuando estén disponibles.

## Impact

- crates/clipvault-platform: nuevos tipos de issue/guidance, detección de
  permiso de Accesibilidad en macOS y adapter SettingsNavigator con destinos
  tipados.
- crates/clipvault-core: propagación de guidance sin exponer payloads ni
  duplicar lógica de plataforma.
- app/tauri/src-tauri: comando delgado para abrir la configuración y
  respuestas serializables enriquecidas.
- app/tauri/frontend: modal de permiso/limitación, pasos específicos por
  plataforma, botones Abrir configuración, Reintentar y Cerrar.
- No se agregan red, telemetría, cuentas ni permisos automáticos. ClipVault
  nunca modifica silenciosamente la configuración del sistema.
