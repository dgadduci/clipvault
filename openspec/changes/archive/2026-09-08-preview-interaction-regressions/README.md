# preview-interaction-regressions

Este cambio corrige regresiones de interacción y presentación detectadas en el
preview compartido, Quick Paste y las cards del Desktop:

- preservar saltos de línea, tabulaciones y líneas vacías en el preview de
  texto enriquecido;
- ordenar el modo reciente de Quick Paste de forma determinista y cronológica;
- evitar que el menú de acciones de una card quede recortado por la card o por
  la rail;
- permitir seleccionar y deseleccionar cards del Desktop con click;
- mostrar el atajo de preview únicamente para la card seleccionada;
- mostrar los atajos reales junto a las acciones del menú que los tienen.

La implementación debe reutilizar `ClipboardPreview.svelte`, los helpers de
atajos existentes, el controlador actual de menús y el controlador singleton
de drag-and-drop. No se debe crear una segunda preview ni una segunda lógica de
selección de resultados.

Estado: propuesto, pendiente de implementación.

No sincronizar ni archivar automáticamente. No modificar ni marcar la
verificación pendiente de `platform-permission-guidance`.
