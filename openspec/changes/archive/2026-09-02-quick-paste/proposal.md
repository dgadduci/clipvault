## Why

`quick-paste` está especificado en
`openspec/specs/quick-paste/spec.md` y figura como requisito del MVP v0.1
("Ventana rápida y hotkey global" + "Selección y pegado del elemento
elegido"). Hoy el shell Tauri emite el evento `clipvault://quick-search`
cuando se registra el hotkey, pero no existe la ventana flotante que lo
recibe. El usuario puede ver los resultados en la pantalla principal, no
puede usar el atajo global como flujo diario y nunca llega a pegar desde
ella.

Además, el adaptador `global-hotkey` mapea `cmd_or_ctrl` a `Modifiers::CONTROL`
de manera incondicional, lo que rompe la promesa de la spec
(`Cmd+Shift+V` en macOS, `Ctrl+Shift+V` en Linux).

## What Changes

- Crear una segunda ventana Tauri llamada `quick-paste` (oculta al
  iniciar, compacta ~640x420, sin decoraciones, no redimensionable,
  siempre encima, fuera del taskbar cuando la plataforma lo permita) y
  registrarla en `capabilities/default.json`.
- Hacer del frontend principal (`App.svelte`) el único listener de
  `clipvault://quick-search`, con un registrar idempotente. Antes de
  mostrar la ventana, consultar `activeApplicationCommand`. Tras
  enfocarla, emitir un evento interno `clipvault://quick-paste-opened`
  a la ventana `quick-paste` (sin payload sensible).
- Crear `QuickPaste.svelte` con lista (recientes para query vacía,
  búsqueda para query no vacía), navegación circular
  ArrowUp/ArrowDown, Home/End, Enter para confirmar y Escape para
  cerrar. Enter sin selección NO invoca `pasteEntryCommand`. La
  ventana se oculta antes de invocar el comando de paste.
- Reusar `PlatformGuidanceModal` y la guidance existente cuando
  `pasteEntryCommand` devuelva `failed` o `capability_unavailable`,
  re-mostrando la ventana en ese caso. Mantener la entrada histórica
  intacta ante errores.
- Corregir el mapeo de modificadores en
  `crates/clipvault-platform/src/runtime/hotkey_global.rs`:
  `cmd_or_ctrl` debe ser `SUPER` en macOS y `CONTROL` en el resto.
  Conservar `default_macos_binding` y `default_linux_binding`.
- Revisar `runSearch` para que cancelaciones resuelvan limpiamente,
  los errores del backend no queden como promesas pendientes y los
  resultados obsoletos no sobrescriban a los nuevos.
- No se introducen red, telemetría, embeddings, favoritos, borrado,
  retención ni dependencias nuevas. No se loguea contenido del
  clipboard.

## Capabilities

### New Capabilities

- quick-paste: ventana flotante y transient que consume el evento del
  hotkey global, expone búsqueda y selección, y orquesta el pegado
  contra el aplicativo activo.

### Modified Capabilities

- desktop-platform-integration: el shell expone una segunda ventana
  `quick-paste` y el frontend principal es el único listener del
  evento del hotkey. El mapeo de modificadores de `global-hotkey`
  respeta la plataforma (SUPER en macOS, CONTROL en Linux). El
  pegado sigue siendo responsabilidad de `PasteService` y el shell
  reutiliza `PlatformGuidanceModal`.

## Impact

- `app/tauri/tauri.conf.json`: segunda ventana `quick-paste` con
  flags transient.
- `app/tauri/src-tauri/capabilities/default.json`: incluir `quick-paste`
  en `windows`.
- `app/tauri/frontend/src/App.svelte`: registrar listener único
  idempotente, capturar active app y mostrar/emitir.
- `app/tauri/frontend/src/QuickPaste.svelte`: nueva vista.
- `app/tauri/frontend/src/lib/search.ts`: hardening del helper
  `runSearch` (errores y cancelaciones).
- `app/tauri/frontend/src/lib/tauri.ts`: helpers para emitir el evento
  `clipvault://quick-paste-opened` y abrir la ventana.
- `crates/clipvault-platform/src/runtime/hotkey_global.rs`: mapeo
  `cmd_or_ctrl` dependiente de la plataforma.
- Tests: nuevos tests `node:test` (frontend) y `cargo test`
  (plataforma) que cubren los puntos críticos del cambio.
- No se introduce ninguna dependencia nueva y los crates siguen sin
  conocer Tauri ni Svelte.
