# Diseño: fundamento de identidad de pares locales

## Frontera

El grafo actual del workspace ya tiene una dependencia de
`clipvault-core` hacia `clipvault-platform` para consumir los adaptadores de
sistema. Por ello, el contrato neutral de identidad (`PeerIdentityStore`,
`PeerId`, `PeerFingerprint`, `LocalPeerIdentity`, errores y outcomes) vive en
`clipvault-platform::peer_identity`: declararlo en core y hacer que platform lo
implemente invertiría ese arco y crearía un ciclo entre crates. El módulo de
contrato no depende de Keychain, D-Bus, Tauri ni de red; las implementaciones
nativas quedan detrás del feature de plataforma correspondiente.

`clipvault-core::peer_identity` consume y reexporta ese contrato, y conserva la
lógica de producto: generación para el fake de tests, `PeerIdentityService`,
el store permanentemente unavailable por defecto, y la proyección
`LocalPeerProfile`. El shell llama ese servicio y transmite al frontend
solamente `LocalPeerProfile`: `peer_id`, fingerprint abreviado y
`display_name`. No transmite clave pública completa, clave privada,
certificado ni detalles del almacén nativo.

La identidad se crea perezosamente cuando una acción futura de red la requiere
o cuando el usuario prepara el perfil. La operación es idempotente: cargar una
identidad existente no rota claves ni cambia peer_id.

## Persistencia y seguridad

La clave Ed25519 y, si el adaptador lo requiere, su certificado se serializan
solamente dentro del secure store bajo un identificador de servicio fijo y
versionado. app_settings incorpora local_peer_display_name; el nombre se
normaliza con trim, rechaza vacío/control chars y tiene máximo 64 caracteres.

SQLite no recibe clave privada, certificado privado, contraseña, token, ruta
del keychain ni endpoint. Los tests usan un store fake en memoria; ninguna
prueba usa el keychain real.

Un error del store se convierte en un outcome estable, por ejemplo
secure_identity_unavailable, sin incluir la causa o datos sensibles. No existe
fallback a archivo de texto claro.

## UI y transición

El modal ⋯ → Privacidad (`PrivacyModal.svelte`, la superficie activa que
`App.svelte` monta) muestra la sección **Identidad de este equipo** con el
nombre editable y el fingerprint abreviado. La sección reutiliza el contrato
metadata-only del bridge (`LocalPeerProfile`, `LocalPeerProfileResponse`)
sin exponer clave pública, clave privada ni detalles del almacén nativo.
Cuando el secure store devuelve `Unavailable` la UI muestra el estado
mudo (`role="status"`, `aria-live="polite"`) y mantiene la edición del
nombre visible habilitada porque la persistencia del nombre no depende
del keychain. El componente `SettingsPanel.svelte` quedó huérfano tras la
reorganización del shell: este cambio lo elimina para evitar duplicación,
y reubica la sección de identidad en el modal activo. No incluye aún
Compartir en red local para no presentar una capacidad que todavía no
inicia discovery. El cambio `local-peer-discovery` agrega ese toggle y
reutiliza este perfil sin regenerarlo.

## Dependencias aprobadas

- `ed25519-dalek` (con `rand_core`) sobre los módulos de identidad de core y
  platform: la
  implementación canónica en Rust de Ed25519 — RFC 8032 — sirve
  exclusivamente para generar y serializar la clave privada y derivar
  `peer_id` / `fingerprint` de la clave pública. Se descartó:
  (a) `ring` por su API ligada a FFI con superficie mayor, (b)
  `ed25519` de RustCrypto porque requiere emparejar `ed25519-dalek`
  para serialización DER/PKCS#8 y duplicaría superficie, (c)
  implementación propia porque la especificación exige verificación de
  firma en cambios posteriores. Sólo se usa `SigningKey`, `VerifyingKey`
  y `Sha512` interno; no se construye ningún material criptográfico
  nuevo ni se permite serialización fuera del secure store.
- `keyring` sobre `clipvault-platform` con un único feature
  opcional `local-peer-identity-keychain` que, según `cfg(target_os)`,
  enlaza la implementación nativa (`security-framework` en macOS y
  `secret-service` sobre D-Bus en Linux). Se descartó:
  (a) `security-framework` + `secret-service` directos porque
  duplican el manejo de errores y los traits de Linux/macOS no son
  intercambiables, (b) mantener un backend propio de Keychain
  porque rompe la promesa de privacidad ("el material privado nunca
  abandona el secure store") al reimitar `SecItemAdd` con código
  nuevo, (c) ningún store de secretos (fallback a texto claro)
  porque la propuesta lo prohíbe explícitamente.

La entrada de keyring guarda y recupera directamente los 32 bytes secretos con
`Entry::set_secret` / `Entry::get_secret`; no se añade un codec como base64 ni
se usa un password de texto para representar el material privado.

Cualquier actualización de estas dos dependencias requiere una
revisión previa del cambio OpenSpec.

## Verificación

Probar generación, carga tras reinicio simulado, derivación estable,
validación de nombre, error seguro y que comandos/bridge no serializan claves.
Ejecutar builds Linux/macOS de compilación; no se requiere prueba LAN.
