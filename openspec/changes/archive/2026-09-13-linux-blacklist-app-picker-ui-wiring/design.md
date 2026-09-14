# Diseño: cableado del selector Linux en PrivacyModal

## Decisión

`PrivacyModal.svelte` es la única superficie de Privacidad que `App.svelte`
monta actualmente. El flujo Linux existente se mueve allí sin crear otro
componente ni duplicar reglas de negocio:

1. `pickAndAddIgnored` intenta `ignoredAppLinuxCatalogCommand`.
2. Si la respuesta es `supported`, guarda el catálogo, abre el modal y
   precarga sus iconos con el `iconResolver` existente.
3. `confirmLinuxPick` invoca `ignoredAppLinuxAddCommand` usando el candidato
   seleccionado; la metadata autoritativa continúa siendo validada por Rust.
4. `closeLinuxPicker` libera las URLs `blob:` y no realiza ninguna mutación.
5. Si el comando Linux no está disponible (por ejemplo en una build no Linux),
   se conserva el flujo legado para las plataformas que ya dependían de él.
   Si el comando responde `unsupported`, la UI comunica que el catálogo no
   está disponible sin invocar el picker legado: ese adaptador tampoco puede
   operar en Linux y sustituía la causa real por un error genérico.

Los enums Rust de catálogo y alta se serializan con `#[serde(tag = "kind")]`.
Así, `Supported` llega como `{ "kind": "supported", ... }` y `Added` como
`{ "kind": "added", ... }`, que coincide con las uniones discriminadas de
`types.ts`. La representación externa de Serde (`{ "supported": { ... } }`)
no es compatible con ese contrato y nunca debe volver a usarse aquí.

El código Linux equivalente se elimina de `SettingsPanel.svelte` para que no
quede una segunda implementación que pueda divergir o producir un bundle
engañoso.

## Estado y ciclo de vida

El estado efímero del catálogo y sus iconos vive dentro de
`PrivacyModal.svelte`. El estado persistente continúa sincronizándose mediante
`syncEntriesWithPicker`, `onSettingsChanged` y los comandos existentes. El
`onDestroy` del modal libera tanto los iconos persistidos como los iconos del
catálogo.

## Verificación

El test frontend leerá las fuentes y fijará estos invariantes:

- `App.svelte` importa y monta `PrivacyModal.svelte`.
- `PrivacyModal.svelte` importa e invoca ambos wrappers Linux, renderiza
  `linux-picker-modal`, y mantiene el botón de cancelación.
- Una respuesta `unsupported` del catálogo no alcanza
  `ignoredAppPickAndAddCommand`.
- La serialización de `LinuxCatalogResponse` y `LinuxPickAndAddResponse`
  contiene `kind` y no una clave de variante externa.
- `SettingsPanel.svelte` no contiene el flujo Linux duplicado.

Además, `npm run check`, `npm run build` y `npm test` deben pasar. El bundle
resultante se inspecciona para confirmar `linux-picker-modal` y
`ignoredAppLinuxCatalogCommand`; `dist` sigue siendo un artefacto generado y
no se añade a Git.

## Privacidad y regresiones

El selector sólo transporta identificadores y metadata de catálogo. No se
añaden logs, red, lectura de clipboard, contenido, hashes, rutas absolutas ni
assets nuevos. No se tocan cards, layout ni drag and drop.
