# Design: quick-paste-actions

## Dependencia

La implementación parte de `quick-paste-compact-ui`: ventana fija de `720 ×
520`, lista vertical, items de dos líneas, foco en búsqueda, miniaturas y
listeners existentes. No se deben duplicar sus componentes ni rehacer su
geometría.

## Dos flujos intencionalmente separados

El flujo de teclado y el menú explícito no deben confundirse:

```text
Enter/Shift+Enter
  → escribir representación al clipboard
  → ocultar Quick Paste
  → no enviar Cmd/Ctrl+V
  → usuario pega manualmente

Menú ... / acción directa
  → hide quick-paste
  → flujo existente de paste
  → pegado sintético al target anterior
```

El flujo copy-only no debe llamar a `pasteEntryCommand` si ese comando también
dispara `PasteController::paste`. Debe existir una operación tipada separada
en core/shell para preparar una entrada en el clipboard, por ejemplo un
servicio `copy_entry` y un comando Tauri equivalente. La API exacta queda a
criterio de la implementación, pero no debe duplicar la lógica de carga de
payloads ni de supresión de autocaptura.

## Copy-only de teclado

La operación copy-only debe:

1. cargar la entrada por id sin mutarla;
2. seleccionar la representación solicitada;
3. escribir texto plano, rich text o imagen mediante los adapters existentes;
4. armar la supresión de autocaptura para que la escritura no cree una nueva
   card;
5. devolver un resultado metadata-only (`copied`, `failed` o capability
   unavailable);
6. ocultar Quick Paste después de una escritura exitosa;
7. no invocar ningún `PasteController`, `CGEvent`, XTest ni evento sintético;
8. no restaurar, limpiar ni reemplazar el clipboard después de escribirlo.

La ventana puede conservar el flujo existente de activación y captura del
active app al abrirse, pero el resultado del probe no debe usarse para enviar
un pegado en `Enter`/`Shift+Enter`.

Si la escritura falla, Quick Paste debe permanecer o reaparecer utilizable,
mostrar guidance tipada cuando exista y no alterar la entrada histórica.

## Modalidades de teclado

- Rich text con plain text: `Enter` copia la representación rich; `Shift+Enter`
  copia plain.
- Sólo plain text: `Enter` copia plain; `Shift+Enter` no duplica la acción.
- Imagen: `Enter` copia la imagen; `Shift+Enter` no ejecuta ningún comando.
- Sin selección: ninguna combinación ejecuta copy o paste.

La disponibilidad se determina con las refs/capabilities reales del
`EntryRecord`, no por el nombre visible del tipo.

## Favoritos

Cada item muestra el pin accesible y reutiliza la mutación existente de
favorito. Los favoritos van primero y cada grupo conserva el orden que ya
entrega la búsqueda o los recientes. El toggle no modifica payload,
timestamp, tags, colecciones, source-app metadata ni assets.

La selección y el foco deben permanecer deterministas cuando el item cambia
de grupo.

## Menú explícito

El menú `...` vive dentro del item fijo y muestra sólo las acciones válidas.
Para esta etapa conserva el flujo directo existente: sus acciones de pegado
pueden ocultar Quick Paste y ejecutar el paste automático al target previo.
Una imagen ofrece solamente `Pegar`; no ofrece acciones de texto.

El menú no debe alterar el tamaño de la fila, duplicar handlers ni crear una
nueva captura.

## No regresiones y privacidad

- La escritura copy-only no genera una card de historial.
- Las imágenes antiguas y nuevas se cargan y copian sin cambiar `asset_ref`.
- El pegado directo del menú mantiene hide-before-paste, guidance y foco.
- Tags, colecciones, favoritos y timestamps permanecen intactos salvo el
  toggle explícito del pin.
- No se escriben contenido, snippets, hashes, bytes, rutas ni identifiers
  sensibles en logs, eventos o respuestas.

## Tests y prueba manual

Los tests deben separar claramente `copied` de `pasted` y demostrar que el
flujo keyboard copy-only no llama al controlador sintético.

En macOS se debe probar con TextEdit, Notas o Safari:

1. enfocar un campo editable;
2. abrir Quick Paste con `Cmd+Shift+V`;
3. seleccionar texto y presionar `Enter`;
4. confirmar que no se pega automáticamente;
5. enfocar un campo y presionar `Cmd+V`;
6. confirmar que se pega la captura;
7. repetir con rich text e imagen;
8. confirmar que el menú directo mantiene su comportamiento definido;
9. confirmar que no se crea una nueva card.
