# Diseño: history-unorganized-delete-ux

## Estado de la vista activa

La visibilidad debe derivarse del estado de colección activa que ya utiliza
`App.svelte` para cargar `recentEntriesCommand` frente a
`recentEntriesFilteredCommand`. No se debe inventar un segundo estado ni
inferir el contexto a partir de la cantidad de cards.

En el contrato actual, `selectedCollectionId === null` representa la vista
`Historial`; las colecciones de usuario tienen un identificador propio. Si el
helper existente `activeCollectionIsHistory` se usa para la presentación,
debe corregirse o ajustarse para que represente este contrato de forma
consistente, sin hardcodear un id numérico ni alterar la selección existente.

## Basurero del toolbar

`DesktopToolbar` debe recibir una señal explícita de visibilidad calculada por
el padre, o renderizarse detrás de una condición equivalente en el padre. La
acción debe:

- renderizarse sólo cuando la colección activa sea `Historial`;
- mantener `data-testid="trash-clear-history"` para las pruebas existentes;
- conservar el estilo visual de peligro y `aria-busy`;
- exponer `aria-label` y `title` como `Eliminar capturas no organizadas`;
- no ser reemplazada por un botón deshabilitado invisible: cuando no aplica,
  debe estar ausente del DOM y del tab order;
- conservar el callback `onRequestClearHistory` existente.

La búsqueda activa no cambia la colección activa. Por tanto, si el usuario
está en `Historial` con una consulta escrita, el basurero conserva la
visibilidad actual de Historial y reutiliza exactamente el alcance actual del
comando. Este cambio no convierte el borrado en una acción limitada sólo a
las cards que coinciden con la búsqueda.

## Confirmación

La confirmación debe conservar el contador consultado por
`unorganizedClearableCountCommand` y explicar con lenguaje inequívoco:

> Se eliminarán las capturas no favoritas y sin colecciones de usuario.
> Las capturas favoritas o asociadas a colecciones se conservarán.

El texto exacto puede adaptarse a la composición visual existente, pero debe
contener esos dos hechos. El título y el botón deben usar el mismo concepto
(`Eliminar capturas no organizadas` o una variante equivalente), evitando
llamar a la acción simplemente “Eliminar historial”.

La secuencia debe seguir siendo:

```text
activar basurero → consultar contador → mostrar confirmación
    ├── cancelar → cerrar, sin comando de mutación
    └── confirmar → clearUnorganizedHistoryCommand({ confirm: true })
                    → refreshEntries + refreshUnorganizedClearableCount
```

Errores, estados de carga y la respuesta `confirmation_required` deben
continuar usando la infraestructura existente. Una acción fallida no debe
ocultar permanentemente el toolbar cuando Historial siga activo.

## Acciones que no deben mezclarse

- En una colección de usuario, la card conserva `Quitar de esta colección`.
  Esa acción modifica sólo la membresía contextual y no debe reutilizar el
  callback del basurero.
- El menú de una card conserva `Delete`, con su confirmación y semántica
  global existente.
- El botón de favorito conserva su estado y no se usa como sustituto de la
  elegibilidad del borrado.

## Privacidad y no-regresiones

No añadir contenido de clipboard, snippets, hashes, nombres de aplicaciones ni
rutas a tooltip, confirmación, eventos o logs. El contador es metadata y debe
seguir siendo el único dato dinámico mostrado.

No tocar el pipeline de imágenes. Las cards con imágenes deben conservar
`asset_ref`, `mime_type`, dimensiones, tamaño y el ciclo
`clipboard_asset → Blob URL → thumbnail` después de reiniciar y al cambiar de
colección. Las tags, favoritos, búsqueda y membresías deben conservarse.

El drag-and-drop protegido por `desktop-header-card-dnd` no debe modificarse.
El basurero oculto en una colección no puede convertirse en un destino de
drop ni interferir con pointer capture, mouse fallback, selección de texto o
los listeners existentes.

## Pruebas requeridas

### Frontend

- Historial muestra un único basurero con `title` y `aria-label` claros.
- Una colección de usuario no renderiza el basurero, tampoco en tabulación o
  accesibilidad.
- Cambiar Historial → colección → Historial actualiza la visibilidad sin
  remounts duplicados ni listeners adicionales.
- Historial con búsqueda mantiene la acción y no altera el alcance del
  comando.
- Activar el basurero abre confirmación; cancelar no llama al bridge.
- Confirmar llama una sola vez al comando existente y refresca cards/contador.
- El texto de confirmación distingue no favoritas/sin colección de favoritas/
  organizadas.
- Error y `confirmation_required` dejan la UI recuperable.
- `Quitar de esta colección`, `Delete` individual, pin, tags, imágenes y
  drag-and-drop mantienen sus contratos.

### Rust/core

No se espera modificación. Si se ejecutan pruebas de regresión, deben confirmar
que `clearUnorganizedHistoryCommand` mantiene exactamente el predicado actual
y que las asociaciones y assets de entradas conservadas no cambian.
