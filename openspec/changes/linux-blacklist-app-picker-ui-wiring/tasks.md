# Tareas: cableado del selector Linux en PrivacyModal

## 1. OpenSpec y relevamiento

- [x] 1.1 Confirmar que `App.svelte` monta `PrivacyModal.svelte` y no
  `SettingsPanel.svelte` para la superficie visible de Privacidad.
- [x] 1.2 Documentar que el backend y los wrappers Linux ya existen y que la
  brecha está en el componente montado.

## 2. Implementación

- [x] 2.1 Mover imports, estado y flujo del catálogo Linux a
  `PrivacyModal.svelte`.
- [x] 2.2 Mover el markup y estilos del modal, conservando iconos, cancelación,
  fallback y sincronización de la blacklist.
- [x] 2.3 Eliminar el flujo Linux duplicado de `SettingsPanel.svelte`.
- [x] 2.4 Añadir una regresión frontend que fije el wiring de la UI activa y
  descarte el componente no montado como fuente del selector.
- [x] 2.5 Evitar que una respuesta Linux explícita `unsupported` caiga al
  picker legado y oculte el motivo del catálogo.
- [x] 2.6 Alinear la serialización de los comandos Linux con las uniones
  TypeScript discriminadas por `kind` y fijar la regresión de contrato.

## 3. Verificación

- [x] 3.1 Ejecutar `npm run check`, `npm run build` y `npm test`.
- [x] 3.2 Confirmar en `dist` los símbolos del modal y catálogo; no añadir
  artefactos generados a Git.
- [x] 3.3 Ejecutar `openspec validate linux-blacklist-app-picker-ui-wiring
  --strict --type change` y revisar `git diff --check`.
- [x] 3.4 Repetir la prueba manual en X11 y Wayland con el bundle/binario del
  mismo checkout. Validado el 2026-09-13: en ambas sesiones la superficie
  activa de Privacidad mostró la lista de aplicaciones disponibles para
  agregarlas a la blacklist y las aplicaciones incluidas en la blacklist no
  generaron nuevas capturas.
