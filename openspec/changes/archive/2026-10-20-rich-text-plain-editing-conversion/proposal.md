## Why

Las capturas de texto enriquecido contienen una representación canónica de
texto plano, pero hoy no se pueden editar desde el historial ni desde Quick
Paste. El usuario necesita corregir ese contenido y aceptar de forma explícita
que, al guardar un cambio, la captura perderá su formato enriquecido.

## What Changes

- Permitir iniciar la edición de capturas rich desde el historial principal y
  Quick Paste mediante el mismo modal de edición de texto plano.
- Mostrar una advertencia antes de abrir el editor; cancelar la advertencia no
  abre el modal ni modifica la captura.
- Al confirmar, cargar en el editor la representación canónica de texto plano.
  Solo una edición con texto cambiado y guardada convierte la captura; cancelar
  el editor o cerrar la advertencia conserva el registro y sus assets.
- Persistir la conversión en la misma entrada, retirar su metadata rich y
  conservar identidad y metadata ajena al contenido.
- Mantener las capturas de imagen fuera de la edición y actualizar los contratos
  de teclado y accesibilidad para que rich siga el flujo de confirmación.
- Preservar la presentación existente del desktop: rail horizontal desplazable,
  viewport estable de colecciones con scroll vertical y texto claro en las
  listas de colecciones y equipos vinculados.
- Añadir los textos visibles de advertencia y confirmación a los catálogos
  `en`, `es`, `pt`, `de` y `fr`.

## Capabilities

### New Capabilities

### Modified Capabilities

- `clipboard-text-history`: editar capturas rich como texto plano tras una
  confirmación, con conversión persistida solo al guardar un texto modificado.
- `clipboard-history-cards`: exponer el flujo de confirmación desde las cards
  del historial y su atajo de edición.
- `quick-paste`: exponer el mismo flujo desde el menú y atajo de Quick Paste.
- `desktop-shell-layout`: preservar el rail de capturas y el contraste de las
  listas de organización durante los cambios de interfaz.

## Impact

- Afecta el modal de edición, las acciones/atajos de las cards y Quick Paste,
  los comandos Tauri y servicios/repositorios Rust que guardan ediciones, y los
  catálogos de traducción.
- Requiere una operación explícita de conversión rich-a-plain en la frontera de
  persistencia; el guardado de texto plano existente sigue atendiendo entradas
  de texto elegibles.
- No requiere migración de esquema ni una dependencia nueva de editor.
