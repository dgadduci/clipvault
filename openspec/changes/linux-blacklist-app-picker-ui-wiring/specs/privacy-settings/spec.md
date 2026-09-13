# Cableado del selector visual Linux en Privacidad

## ADDED Requirements

### Requirement: La superficie activa de Privacidad ofrece el selector Linux

La superficie de Privacidad montada por `App.svelte` SHALL usar el catálogo
Linux antes del picker legado cuando el comando de catálogo devuelva
`supported`. SHALL mostrar `linux-picker-modal` con los candidatos del catálogo
y SHALL usar `clipvault_ignored_app_linux_add` para confirmar una fila.

#### Scenario: X11 o Wayland con catálogo soportado

- GIVEN `App.svelte` monta `PrivacyModal.svelte`
- AND `clipvault_ignored_app_linux_catalog` devuelve `supported`
- WHEN el usuario pulsa `Seleccionar aplicación`
- THEN se muestra `linux-picker-modal`
- AND se muestran los candidatos recibidos
- AND no se invoca el picker legado

#### Scenario: Sesión Linux sin catálogo visual

- GIVEN `clipvault_ignored_app_linux_catalog` devuelve `unsupported`
- WHEN el usuario pulsa `Seleccionar aplicación`
- THEN la UI comunica que el catálogo visual no está disponible
- AND no se invoca el picker legado
- AND el error no se sustituye por su mensaje genérico `unsupported_session`

#### Scenario: Contrato de respuesta Linux reconocido por la UI

- GIVEN el comando de catálogo Linux devuelve candidatos
- WHEN Tauri serializa la respuesta hacia `PrivacyModal.svelte`
- THEN contiene `kind: "supported"` y los campos del catálogo al mismo nivel
- AND la respuesta de alta contiene `kind: "added"` o `kind: "updated"`
- AND ninguna respuesta usa una variante Serde externamente etiquetada

#### Scenario: Confirmar un candidato

- GIVEN el modal Linux está abierto con un candidato del catálogo
- WHEN el usuario selecciona ese candidato
- THEN se invoca `clipvault_ignored_app_linux_add` con su identificador
- AND la lista de aplicaciones ignoradas se sincroniza con la respuesta

#### Scenario: Cancelar no muta la blacklist

- GIVEN el modal Linux está abierto
- WHEN el usuario pulsa `Cancelar`
- THEN el modal se cierra
- AND no se invoca `clipvault_ignored_app_linux_add`
- AND no cambia la lista de aplicaciones ignoradas

### Requirement: No existe una implementación Linux duplicada en una superficie no montada

El flujo Linux SHALL estar definido en `PrivacyModal.svelte`, que es la
superficie activa, y SHALL NOT permanecer duplicado en `SettingsPanel.svelte`.

#### Scenario: Bundle actualizado

- GIVEN se ejecuta `npm run build` desde el checkout actual
- WHEN se inspecciona el bundle Vite
- THEN contiene `linux-picker-modal`
- AND contiene `ignoredAppLinuxCatalogCommand`

#### Scenario: Flujo validado manualmente en X11 y Wayland

- GIVEN el bundle frontend y el binario Tauri provienen del mismo checkout
- WHEN el usuario abre la superficie activa de Privacidad en X11 y Wayland
- THEN en ambas sesiones se muestra la lista de aplicaciones disponibles para
  agregarlas a la blacklist
- AND las aplicaciones incluidas en la blacklist no generan nuevas capturas
