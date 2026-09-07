# Proposal: desktop-card-preview

## Por qué

Quick Paste ya ofrece una preview completa y segura para las capturas de texto
e imagen, pero la ventana principal sólo muestra el resumen truncado dentro de
la card. El usuario debe abrir Quick Paste para inspeccionar una captura sin
copiarla ni pegarla.

La preview de Quick Paste ya resolvió detalles importantes: texto completo,
escape seguro, miniaturas desde el asset persistido, estados de carga/error,
scroll interno y cierre accesible. Reimplementarla en `HistoryCard.svelte`
crearía dos contratos que podrían divergir y repetiría el manejo de Blob URLs.

## Qué cambia

- Agregar `Previsualizar` al menú de cada card para todos los tipos soportados.
- Activar la misma preview con `Cmd+Enter` en macOS y `Ctrl+Enter` en Linux
  cuando una card del Desktop tiene el foco.
- Extraer la superficie y la lógica común de preview de Quick Paste a un
  componente/helper compartido que puedan consumir Desktop y Quick Paste.
- Mantener el comportamiento actual de Quick Paste, pero haciendo que ambos
  consumidores llamen la misma implementación para texto, rich text e imagen.
- Mostrar texto completo y seguro, no el fragmento truncado de la card.
- Mostrar imágenes usando el asset persistido completo, sin recortar,
  redimensionar el archivo ni cambiar sus metadatos.
- Mantener la preview como una operación estrictamente de lectura.

## Decisiones de interacción

- La acción del menú siempre está disponible; si el usuario la activa, la card
  abre la preview y el menú se cierra una sola vez.
- El atajo opera sobre la card enfocada. Si no hay una card enfocada, si el
  foco está en un input/editor/control interactivo o si la entrada ya no está
  visible, no hace nada.
- Desktop y Quick Paste usan `Cmd/Ctrl+Enter`; no se agrega un atajo distinto.
- `Escape` cierra primero la preview. El foco vuelve al disparador cuando éste
  sigue existiendo.
- Click fuera de la superficie de preview la cierra, igual que en Quick Paste.
- Abrir o cerrar la preview no copia, pega, marca favorito, edita título,
  cambia tags/colecciones, crea historial ni cambia la aplicación activa.

## No objetivos

- No cambiar el tamaño o la geometría fija de Quick Paste ni del Desktop.
- No crear una segunda ventana, un nuevo comando Tauri ni una nueva ruta de
  búsqueda.
- No duplicar `entryFullPreviewText`, `escapeForPreview`, resolvers de assets,
  estados de thumbnail, sanitización ni manejo de Blob URLs.
- No cambiar el contenido persistido, títulos, tags, colecciones, favoritos,
  timestamps o referencias de assets.
- No cambiar el comportamiento de copiar/pegar, los menús existentes salvo
  agregar `Previsualizar`, ni el pegado sintético.
- No agregar red, telemetría, embeddings, dependencias ni recursos externos.

## Capacidades afectadas

### Capacidades modificadas

- `clipboard-history-cards`: nueva preview de la captura desde la card.
- `quick-paste`: reemplazo interno por el componente compartido, sin cambio
  funcional observable.

## Impacto esperado

- Frontend: nuevo componente/helper compartido de preview, `HistoryCard.svelte`,
  `HistoryCardRail.svelte` o `App.svelte` para coordinar una única preview
  Desktop, y `QuickPaste.svelte` para consumir la misma superficie.
- Bridges: reutilización de `clipboardAssetCommand` y
  `richTextPreviewCommand` si la implementación vigente los necesita; no se
  espera cambio de contrato Tauri.
- Backend/core/DB: sin cambios esperados.
- Tests frontend: preview común, shortcut, menú, foco, imágenes, texto,
  privacidad y regresiones de cards.
