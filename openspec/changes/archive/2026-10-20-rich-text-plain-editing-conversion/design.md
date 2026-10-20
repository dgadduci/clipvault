## Context

Las entradas `RichText` guardan su representación canónica de texto plano en
`content` y referencias/hash/tamaños de sus representaciones rich en la misma
fila. `EntryTextEditorModal` ya permite editar entradas de texto plano; el
servicio actual rechaza entradas con referencias rich. Historial y Quick Paste
resuelven el editor desde sus propias acciones y atajos. El cambio activo
`quick-paste-text-editing` define hoy un aviso informativo para el atajo sobre
capturas rich; al implementar este cambio, ese contrato debe alinearse con la
nueva confirmación y conservarse el comportamiento actual para imágenes.

## Goals / Non-Goals

**Goals:**

- Compartir una advertencia y un mismo modal de texto plano en Historial y
  Quick Paste, incluida la ruta de atajo.
- Hacer que confirmar solo autorice la edición; la conversión ocurre al guardar
  texto no vacío que difiere byte por byte del `content` original.
- Mantener la conversión atómica y preservar identidad, metadatos ajenos al
  contenido e integridad de assets compartidos.

**Non-Goals:**

- Conservar, editar o reconstruir formato HTML/RTF después de guardar.
- Editar capturas de imagen, cambiar el esquema de SQLite o agregar una
  dependencia de editor enriquecido.
- Cambiar la edición de capturas que ya son texto plano.

## Decisions

### Preservar la presentación existente del desktop

La conversión de texto rich solo agrega la advertencia y el flujo de edición.
El rail principal conserva sus cards en una sola fila horizontal, scroll
horizontal y altura fija. El panel de colecciones conserva una región de lista
con altura independiente del número de elementos y scroll vertical. Los textos
de colecciones y equipos vinculados usan el token de foreground claro del
sistema visual, sin depender del color heredado del contenedor.

### Confirmación separada de la mutación

Al elegir `Editar captura` para una entrada rich, la interfaz muestra un
diálogo accesible que advierte que guardar texto modificado convertirá la
captura a texto plano y quitará su disponibilidad de copiado rich. Cancelar
cierra el aviso sin abrir el editor. Confirmar abre `EntryTextEditorModal` con
el `content` canónico y el título resuelto de la captura; no invoca ningún
comando de persistencia.

Se reutiliza el modal actual en vez de crear una ruta de edición paralela. En
modo de conversión, Guardar permanece deshabilitado mientras el borrador sea
igual byte por byte al original o esté vacío. Si el usuario cambia el texto y
luego lo restaura al valor original, Guardar vuelve a deshabilitarse. Esta
decisión hace que la confirmación por sí sola nunca convierta el registro.

### Operación backend explícita

La conversión usa una intención/comando tipado separado del guardado existente
de texto plano. El frontend no debe fabricar un registro sin referencias rich
para reutilizar `updateTextEntryCommand`: el backend vuelve a leer la fila y
valida que siga siendo una entrada rich antes de actualizarla.

El core calcula hash, tamaño y tipo textual con las mismas reglas deterministas
que el guardado de texto existente. En una única transacción, la persistencia
actualiza `content` y sus campos derivados, limpia el hash y todas las
referencias/tamaños rich, actualiza `updated_at` y deja intactos ID,
`created_at`, `last_seen_at`, título, favorito, origen, tags y colecciones. Una
entrada ausente, de imagen, ya convertida, vacía, sin cambio o duplicada
produce un resultado tipado y no altera la fila. No se requiere migración.

Las referencias rich se limpian en la misma transacción que la conversión. Los
archivos de assets solo se recolectan después del commit y si el repositorio
confirma que ninguna otra entrada los referencia; ante duda se conservan para
evitar romper otra captura.

### Un único flujo en ambas superficies

Historial y Quick Paste montan el mismo componente de advertencia y el mismo
editor. Cada superficie resuelve de nuevo la fila por ID al confirmar; el
backend vuelve a validarla al guardar. Los atajos existentes delegan en la
acción de edición de su superficie. El atajo sobre una imagen mantiene su
aviso de captura no editable; un atajo sobre rich abre la advertencia de
conversión. Quick Paste conserva query, selección y visibilidad durante el
ciclo. El contrato pendiente de `quick-paste-text-editing` se actualiza para
que rich use esta advertencia y las imágenes conserven su aviso informativo.

### Privacidad y traducciones

La advertencia, confirmación, errores y labels accesibles se consumen mediante
claves de traducción presentes en `en`, `es`, `pt`, `de` y `fr`. El borrador
puede cruzar el bridge únicamente para guardarse; contenido, hash, snippets,
referencias y rutas no se agregan a logs, diagnósticos ni eventos. El evento
existente `clipvault://history-updated` sigue siendo metadata-only.

## Risks / Trade-offs

- **El usuario espera conservar el formato al guardar** → La advertencia
  explica la pérdida de formato antes de abrir el editor y se repite que solo
  Guardar texto modificado convierte la entrada.
- **La entrada cambia desde otra superficie mientras el modal está abierto** →
  El comando vuelve a validar el ID y que la fila siga siendo rich; el backend
  rechaza el estado obsoleto sin limpiar referencias.
- **Los assets rich pueden estar compartidos** → La recolección ocurre después
  del commit y solo si no queda otra referencia; la transacción nunca borra un
  asset potencialmente compartido.
- **Los cambios de menú y teclado pueden afectar interacciones protegidas** →
  Se reutilizan los listeners y controles existentes, y se ejecutan las
  regresiones de cards/drag-and-drop y Quick Paste descritas en `tasks.md`.

## Migration Plan

No hay migración de datos ni de esquema. Las entradas existentes permanecen
intactas hasta que el usuario confirme la advertencia, modifique el texto y
guarde. El rollback consiste en retirar la nueva operación y sus affordances;
las filas ya convertidas siguen siendo entradas de texto plano válidas y no
recuperan automáticamente el formato rich que el usuario aceptó retirar.
