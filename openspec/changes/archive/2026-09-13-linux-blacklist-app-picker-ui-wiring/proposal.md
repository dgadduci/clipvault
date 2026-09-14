# Propuesta: cablear el selector Linux en la UI activa

## Problema

El backend Linux del selector visual ya expone los comandos Tauri de catálogo
y alta, pero la implementación del modal quedó en `SettingsPanel.svelte`.
`App.svelte` no monta ese componente: monta `PrivacyModal.svelte`. Además, las
respuestas Rust se serializaban como enums externamente etiquetados mientras la
UI espera una unión con `kind`. Por eso el botón visible no reconocía un
catálogo `supported`, invocaba el picker legado y mostraba `unsupported_session`
tanto en X11 como en Wayland.

## Objetivo

Trasladar el flujo del catálogo Linux a `PrivacyModal.svelte`, conservar el
fallback legado sólo cuando el comando Linux no exista en otra plataforma y
garantizar mediante
una regresión frontend que la superficie montada invoque el catálogo y el
comando `add`.

## Alcance

- Imports, estado y flujo de catálogo Linux en `PrivacyModal.svelte`.
- Contrato JSON internamente etiquetado (`kind`) entre los comandos Rust Linux
  y las uniones TypeScript ya declaradas.
- Modal visual, confirmación, cancelación y resolución segura de iconos.
- Una respuesta Linux explícita `unsupported` se comunica sin invocar el
  picker legado, que no aporta información en Linux y ocultaba la causa.
- Eliminación del código Linux duplicado/no montado en `SettingsPanel.svelte`.
- Regresión source-level frontend y verificación del bundle Vite.

## Fuera de alcance

- Cambiar el resolver de backend, el catálogo `.desktop` o los comandos Rust.
- Cambiar la detección X11, Wayland, XWayland o GNOME.
- Eliminar el ingreso manual de identificadores.
- Cambiar PrivacyGate, SQLite, assets persistidos o el payload del clipboard.

## Criterio de éxito

En la UI de Privacidad que monta `App.svelte`, pulsar `Seleccionar aplicación`
intenta primero `clipvault_ignored_app_linux_catalog`; un catálogo soportado
abre `linux-picker-modal`, y seleccionar una fila invoca
`clipvault_ignored_app_linux_add`. Cancelar no muta la blacklist. El bundle
generado desde el checkout contiene esos símbolos.
