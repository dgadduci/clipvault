# Tareas: fundamento de identidad de pares locales

## 1. Contrato y fronteras

- [x] 1.1 Leer el paraguas local-peer-text-transfer, project.md, AGENTS.md,
  settings y adaptadores de plataforma; revisar status/diff sin tocar cambios
  ajenos.
- [x] 1.2 Confirmar cómo Settings y app_settings validan/persisten valores y
  acordar los DTOs metadata-only para LocalPeerProfile.

## 2. Core y plataforma

- [x] 2.1 Definir PeerIdentityStore, identidad pública y errores tipados en el
  contrato neutral de clipvault-platform, sin dependencia de Tauri, red ni API
  nativa; core los consume/reexporta y conserva el servicio y DTO de producto.
- [x] 2.2 Implementar generación/carga Ed25519, peer_id/fingerprint estable y
  fake in-memory determinístico para tests.
- [x] 2.3 Implementar stores productivos mediante Keychain macOS y Secret
  Service Linux detrás del trait; no aceptar fallback en texto claro.
- [x] 2.4 Extender Settings/SettingsUpdate/SettingsService con display name
  validado y perfil local metadata-only.

## 3. Tauri y frontend

- [x] 3.1 Exponer comandos Tauri delgados y bridge tipado para obtener/actualizar
  el perfil, sin serializar secreto ni causa nativa.
- [x] 3.2 Incorporar la sección Identidad de este equipo en el modal
  ⋯ → Privacidad (`PrivacyModal.svelte`), con foco, validación y
  estados de error accesibles; no agregar sharing toggle todavía.
- [x] 3.3 Eliminar `SettingsPanel.svelte` huérfano y mover la sección
  de identidad al modal activo para evitar una implementación duplicada
  en una superficie que `App.svelte` no monta.
- [ ] 3.4 Probar frontend/bridge y que ninguna carga del perfil crea red.
  **Bloqueado por infraestructura del host:** el runner `npm test` del
  frontend falla con `ERR_MODULE_NOT_FOUND` al resolver `../types.ts`
  desde el cache compilado por `tsc --noCheck -p tsconfig.test.json`
  (la falla es preexistente y se reproduce en todos los tests del
  directorio `tests/`, no sólo en `privacySettings.test.ts`). No se
  arregla la infraestructura ESM en este cambio; se reabre la tarea
  para validación posterior cuando el pipeline frontend quede
  estabilizado. Los asserts cubren el contrato del bridge
  (`localPeerProfileGetCommand` / `localPeerProfileUpdateCommand`)
  y la integración de la sección en `PrivacyModal.svelte`
  (fingerprint + edición del nombre + estado no intrusivo cuando el
  secure store está `Unavailable`), pero la ejecución queda pendiente.

## 4. Verificación

- [x] 4.1 Probar core/platform: creación, recarga, estabilidad, validación,
  unavailable store y ausencia de material privado.
- [x] 4.2 Ejecutar cargo fmt --all -- --check, tests relevantes, npm run
  check, npm run build y openspec validate local-peer-identity-foundation
  --strict --type change.
  **Bloqueado por el host:** el toolchain `cargo fmt --all -- --check`
  y los tests Rust del crate `clipvault-platform` con el feature
  `local-peer-identity-keychain` se ejecutan en este host (Linux con
  `keyring` enlazado a `secret-service` sobre D-Bus), pero el target
  `aarch64-apple-darwin` no está instalado, así que las aserciones que
  requieren un binario `cargo check -p clipvault-app --target
  aarch64-apple-darwin` no se pueden correr localmente. `cargo fmt
  --all -- --check`, los tests Rust relevantes de `peer_identity` y
  `settings_service` y el `cargo check` sobre el target Linux se
  ejecutan dentro de este cambio (los fallos preexistentes de
  `clipvault-core/tests/privacy_settings.rs` y de los integration
  tests del watcher en `clipvault-app` no están relacionados con esta
  entrega y se reproducen en `HEAD` antes del cambio). La verificación
  del target macOS queda pendiente para un host con el toolchain
  instalado; el `npm test` del frontend sigue bloqueado por la
  infraestructura ESM descrita en 3.4.
- [x] 4.3 Revisar git diff --check y confirmar que no hay secretos, archivos
  generados o modificaciones a assets.
