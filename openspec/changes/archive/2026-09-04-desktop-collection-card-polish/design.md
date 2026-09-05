# Diseño: desktop-collection-card-polish

## Principios

La implementación debe ser un refinamiento incremental del desktop existente.
La card y el rail siguen siendo la única superficie visual de capturas. El
frontend puede formatear metadata que ya existe, pero no debe leer SQLite ni
inventar una segunda fuente de verdad.

## Geometría del desktop y colecciones

El rail de cards ya tiene el token local --cv-card-size y cards cuadradas de
tamaño fijo. El panel de colecciones debe usar el mismo contrato geométrico:

- el panel y el área visible del rail deben compartir una altura estable,
  calculada desde el token de card y el espacio de sus controles;
- el encabezado del panel y el control de nueva colección permanecen visibles;
- sólo collection-list tiene overflow-y auto y un min-height de cero que
  permita el scroll dentro del panel;
- muchas colecciones nunca pueden ampliar el ancho del desktop ni aumentar
  indefinidamente su altura;
- en un viewport estrecho se permite apilar los paneles, pero el listado sigue
  teniendo un límite interno y no crea un overflow horizontal.

No se debe fijar una segunda constante incompatible con --cv-card-size. El
panel y el rail deben reaccionar juntos si el token cambia.

## Edición de colecciones

El botón textual Renombrar desaparece. Para una colección de usuario, el doble
clic sobre el nombre inicia el editor inline en la misma fila. La colección
Historial no se puede renombrar.

Como doble clic no es accesible para todos los usuarios, el nombre debe poder
recibir foco y ofrecer una alternativa de teclado documentada, recomendándose
F2 para iniciar el renombrado. Enter confirma cuando el editor está activo y
Escape cancela. La activación normal del botón de colección conserva la
selección de la colección y no inicia un renombrado accidental.

El editor usa el ancho disponible de la fila. Sus confirmación y cancelación
son iconos con aria-label, tooltip y foco visible. No se deben mostrar botones
textuales Guardar/Cancelar para renombrar.

El icono de eliminar se muestra en la misma línea del nombre, sólo para
colecciones de usuario, con color de peligro, nombre accesible y la
confirmación existente. El icono nunca aparece para Historial.

## Creación de colecciones

El control + Nueva se reemplaza por un icono local de suma o carpeta con nombre
accesible Nueva colección. Al activarlo aparece un único input inline amplio,
sin abrir otra aplicación. La confirmación y cancelación deben ser iconos
dentro del contenedor del formulario o un patrón visual equivalente que deje
la mayor parte del ancho al input.

Enter confirma, Escape cancela, el nombre se recorta y se aplican las
validaciones ya existentes. El icono de confirmación se deshabilita o muestra
estado inválido para un nombre vacío. Cancelar no crea ni modifica nada.

## Edición de título de card

El menú de la card ya no incluye Editar título. El doble clic sobre el título
visible inicia un input inline en la zona superior central de la card. El input
debe conservar el ancho disponible sin cambiar las dimensiones cuadradas.

El editor tiene confirmación/cancelación accesibles mediante iconos, Enter y
Escape. La validación actual se conserva. Un título vacío confirmado restaura
el título por defecto del tipo de contenido, sin modificar content, hashes,
assets, tags, colecciones ni timestamps de captura.

La navegación por teclado debe poder enfocar el título y activar la misma
edición sin depender exclusivamente del doble clic; la acción debe anunciarse
con aria-label y title.

## Atajo de búsqueda

El shell registra un único listener de teclado de nivel desktop, con cleanup
determinista. En macOS reconoce metaKey + f; en Linux reconoce ctrlKey + f.
El listener previene la búsqueda nativa del webview y enfoca el input de
búsqueda, sin disparar una búsqueda con contenido nuevo por sí mismo.

Si el usuario ya está escribiendo en un input, textarea o contenteditable, el
atajo no debe robarle el foco salvo que el elemento sea el propio input de
búsqueda. Si hay una modal con una operación bloqueante abierta, se respeta el
ciclo de foco de la modal.

La barra muestra ⌘F en macOS y Ctrl F en Linux mediante un hint visual y
accesible. No se registra una segunda combinación global ni se añade una
dependencia de plataforma nueva.

## Fecha, edad, orden y tamaños

### Fecha de captura

created_at es el instante original de aceptación de la captura y debe ser
persistido en la misma transacción que la fila. El cambio debe reutilizar el
campo existente y demostrar que sobrevive a cierre y reapertura de SQLite.

Favorito, tag, colección, título, paste o una captura duplicada no deben
reescribir la fecha original que la card usa para su edad. updated_at y
last_seen_at pueden seguir cumpliendo sus contratos internos, pero no son la
fuente del texto Hace.

### Tiempo transcurrido

Crear un helper puro y testeable que reciba created_at y un instante now. Debe
interpretar timestamps RFC3339/UTC sin depender de la zona horaria local,
protegerse ante datos inválidos o fechas futuras y producir un texto compacto
y un nombre accesible.

Reglas deterministas recomendadas:

| Edad | Texto visual |
| --- | --- |
| menor a 1 minuto | Ahora |
| menor a 1 hora | Hace N min |
| menor a 1 día | Hace N h |
| menor a 30 días | Hace N días |
| menor a 12 meses de 30 días | Hace N meses |
| resto | Hace N años |

El intervalo que actualiza la edad debe ser único para el desktop/rail,
aproximadamente cada minuto, y no debe volver a consultar SQLite, perder el
estado de thumbnails ni remontear las cards. Debe limpiarse al destruir el
componente.

### Orden

Las consultas que alimentan el rail deben ordenar por created_at DESC y usar
id DESC como desempate determinista. Favoritos no tienen prioridad
cronológica especial en esta vista: pin/unpin no debe mover una card por sí
mismo. La búsqueda conserva su ranking propio; cuando sus resultados se
renderizan en el rail no se debe ordenar de nuevo destruyendo ese ranking.

### Contadores

- En una card textual, el contador es el total de caracteres del contenido
  canónico, no el largo del preview truncado. Para evitar depender de detalles
  de UTF-16, contar puntos de código Unicode con una operación local y
  determinista (Array.from o equivalente); exponer el valor completo en
  accesibilidad.
- En una card de imagen, el peso proviene de content_size, que debe
  representar los bytes del payload PNG persistido, no el tamaño del thumbnail,
  del Blob URL ni de la metadata. Formatear en bytes cuando sea menor a 1024,
  KB hasta 1024 KB y MB a partir de allí, usando base 1024 y manteniendo el
  número exacto de bytes accesible.
- No mostrar contador textual en lugar de una imagen ni leer el sentinel vacío
  de content como si fuera el peso de la imagen.

## Protección explícita contra regresiones de imágenes

El cambio no puede resolver tags, pin, orden o layout reemplazando un
EntryRecord parcial. Toda actualización frontend debe conservar asset_ref,
mime_type, payload_width, payload_height, content_size y los demás campos de
payload.

En el arranque se debe verificar la cadena completa:

SQLite -> EntryRecord -> recent_entries -> HistoryCardRail
       -> clipvault_clipboard_asset -> Blob URL -> thumbnail

La hidratación de tags/colecciones y el cambio de favorito sólo pueden
modificar sus mapas o is_pinned; no pueden borrar referencias de assets,
disparar un fallback prematuro ni revocar un Blob URL válido. Los estados de
thumbnail deben seguir diferenciando loading, loaded y error.

## Privacidad y compatibilidad

Los iconos, labels y hints son locales. No se registran contenido, snippets,
hashes, rutas absolutas ni bytes de imágenes. Se conservan las semánticas de
Historial, tags, colecciones, favoritos, eliminación, retención, búsqueda,
pegado, blacklist, permisos y quick-paste.

## Tests requeridos

- Helpers puros para tamaño de panel, formato de edad, conteo Unicode, tamaño
  de bytes y detección de Cmd/Ctrl+F.
- Frontend para doble clic/teclado, confirmación/cancelación inline, eliminación
  con icono rojo, hint de plataforma, listener único y cleanup.
- Integración de colecciones para scroll interno y no expansión del layout.
- Rust/SQLite para fecha inmutable, orden por created_at, tamaño de imagen y
  persistencia después de reabrir.
- Regresión de imagen completa: una imagen existente conserva su fila, asset
  válido, bytes retornados por Tauri y thumbnail visible después de reiniciar,
  hidratar tags y hacer pin/unpin.
