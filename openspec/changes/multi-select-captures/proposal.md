# Proposal: multi-select-captures

## Why

Desktop y QuickPaste solo permiten mantener una captura seleccionada a la vez.
Esto impide organizar juntas varias capturas desde el escritorio y deja ambas
listas sin una selección múltiple coherente.

## What Changes

- Permitir selección múltiple temporal en Desktop y QuickPaste mediante
  Ctrl/Cmd + click para alternar capturas, con controles visibles por captura
  en QuickPaste; Desktop seleccionará rangos con Shift + click o Shift +
  flechas y QuickPaste con Shift + flechas.
- En Desktop, permitir arrastrar las capturas seleccionadas a una colección
  local. La asignación conservará el historial y todas las asociaciones
  existentes, y respetará las restricciones para colecciones vinculadas a
  equipos.
- En QuickPaste, confirmar una selección de varias capturas ejecutará un paste
  stack secuencial en el orden visible de la lista. Los clicks simples
  conservarán el flujo actual de una captura.
- Limpiar selecciones obsoletas al cambiar la lista visible y mantenerlas solo
  en memoria.

## Capabilities

### Modified Capabilities

- `clipboard-history-cards`: selección múltiple temporal en las cards del
  escritorio.
- `desktop-header-card-dnd`: drag-and-drop de la selección a una colección.
- `quick-paste`: selección múltiple en la lista QuickPaste y paste stack al
  confirmar una selección grupal.

## Impact

- Afecta al estado de selección de `HistoryCardRail.svelte`,
  `HistoryCard.svelte` y `QuickPaste.svelte`, al controlador singleton de drag
  and drop, a la zona delegada de colecciones y al adaptador de asignación en
  `App.svelte`.
- Reutiliza `clipvault_entry_collections_set` y la combinación existente de
  asociaciones; no requiere migraciones, comandos de base de datos ni nuevas
  dependencias.
- Añade las claves de accesibilidad y selección necesarias a los catálogos
  `en`, `es`, `pt`, `de` y `fr`.
