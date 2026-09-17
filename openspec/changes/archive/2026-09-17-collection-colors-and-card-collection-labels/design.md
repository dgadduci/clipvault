# Diseño: colores de colecciones y etiquetas de membresía

## 1. Fronteras de arquitectura

```text
Sidebar / CollectionColorModal / HistoryCard
                    ↓ comandos Tauri thin
          OrganizationService / DTOs seguros
                    ↓ transacción
             SQLite collections + memberships
```

La lógica de validación, normalización, generación del color inicial y
persistencia vive en `clipvault-db`/`clipvault-core`. Tauri sólo adapta
argumentos, resultados y el evento metadata-only de actualización. Svelte
renderiza `Collection.color_hex` y no accede directamente a SQLite.

## 2. Contrato del color

La entidad `Collection` incorpora `color_hex: String`. El valor canónico es un
color RGB opaco en formato hexadecimal de seis dígitos, normalizado a
minúsculas (`#rrggbb`). No se aceptan alpha, nombres CSS, `currentColor`,
gradientes ni valores fuera de rango.

La paleta inicial base es:

| Nombre | HEX |
| --- | --- |
| rojo | `#c62828` |
| amarillo | `#a16207` |
| verde | `#2e7d32` |
| azul | `#1565c0` |

La paleta se usa sólo para el color inicial. El selector puede guardar cualquier
color RGB válido. Los tonos base son deliberadamente oscuros para que el texto
de las etiquetas sea legible sobre superficies claras; el picker debe mostrar
el valor elegido y un nombre accesible, sin depender únicamente del color.

La selección aleatoria es local, no criptográfica y sólo sirve para variedad
visual. La decisión se toma en el backend al crear la colección, no en el
frontend, y se inyecta o estabiliza en los tests para que éstos sean
deterministas. `Historial` recibe azul como valor inicial; su identidad,
protección contra renombrado/eliminación y memberships no cambian. Su color sí
puede editarse porque el selector se aplica a todas las colecciones visibles.

## 3. Persistencia y migración

Se agrega una migración posterior a la migración actual de organización:

- `collections.color_hex TEXT NOT NULL` con un valor por defecto compatible
  con el color azul.
- Las bases existentes conservan nombres, timestamps y memberships.
- `Historial` y las colecciones ya existentes quedan con azul si no tenían
  color previo.
- La migración debe ser idempotente, comprobable desde una base anterior y
  reversible siguiendo el patrón de migraciones del repositorio.

El repositorio debe devolver y actualizar el color en la misma entidad
`Collection`. `set_collection_color` valida el valor antes de modificar la
fila, actualiza `updated_at` y devuelve la colección actualizada. Un fallo de
validación o un ID inexistente no modifica la base.

La creación de una colección mantiene la validación de nombres y agrega el
color elegido por el servicio dentro de la misma inserción. El cambio de color
no toca ninguna fila de `entry_collections`, `entry_tags`, entradas, assets,
favoritos ni timestamps de capturas.

## 4. Comandos y eventos

Agregar un comando Tauri thin para actualizar el color por `collection_id` y
`color_hex`. Puede devolver la colección actualizada, pero nunca debe incluir
contenido de clipboard, hashes, snippets, rutas ni bytes de assets.

Después de un guardado exitoso se emite el evento existente
`clipvault://organization-updated` con payload vacío. La app vuelve a cargar el
snapshot de organización y las asociaciones de las cards visibles; no se
agregan listeners globales por colección ni por card.

El bridge TypeScript debe exponer `color_hex` y el comando nuevo con tipos
explícitos. Los errores deben conservar el contrato de errores tipados y no
mostrar valores arbitrarios del backend como HTML.

## 5. Selector de color y sidebar

La opción recomendada es `svelte-awesome-color-picker` v4, actualmente
documentada para Svelte 5. Expone `bind:hex`, permite desactivar el canal alpha,
ofrece navegación por teclado y variantes accesibles. Referencias:

- <https://svelte-awesome-color-picker.vercel.app/>
- <https://svelte-awesome-color-picker.vercel.app/#api>

Debe agregarse como dependencia runtime del frontend y quedar registrada en
`package-lock.json`. Se configura con `isAlpha={false}` y el modal debe
guardar sólo el valor HEX normalizado al confirmar. La paleta base puede
mostrarse como swatches rápidos, sin impedir la selección libre.

La compatibilidad multiplataforma se basa en que el componente sólo usa DOM,
CSS y JavaScript del frontend. Tauri usa WKWebView en macOS y WebKitGTK en
Linux; por eso el componente no debe invocar APIs nativas ni asumir X11 o
Wayland. La compatibilidad final se comprueba manualmente en Wayland, X11 y
macOS.

Cada fila de la sidebar conserva su interacción actual y agrega, junto al
nombre, un cuadrado pequeño con `background-color: color_hex`. El cuadrado es
un control interactivo aislado:

- doble click abre el modal para esa colección;
- Enter/Space también lo abre para teclado;
- click simple no cambia la colección seleccionada;
- tiene `aria-label`, `title`, `data-testid` y `data-collection-id`;
- no inicia drag, no dispara el drop de la fila y no cambia el orden de
  colecciones.

El modal reutiliza el shell de modales existente: foco inicial en el picker,
guardar/cancelar, Escape, click de backdrop y foco de retorno al cuadrado. El
estado visual no se actualiza de forma permanente hasta que el comando de
guardado termina con éxito.

## 6. Etiquetas de colecciones en cards

`HistoryCard` ya recibe asociaciones de tags y colecciones. Se reutiliza esa
fuente de datos, enriquecida con `Collection.color_hex`, sin crear una consulta
paralela ni transportar contenido.

Debajo de la fila actual de tags se agrega una fila de chips de colección:

- cada colección de usuario asignada se renderiza; la colección de sistema
  `Historial` se excluye de los chips inline de la card;
- cada chip usa el nombre completo, el `color_hex` propio como color de texto,
  borde, fondo, radio, padding y truncamiento visual consistentes con los
  `tag-chip` existentes;
- mientras todos los chips entren en el ancho disponible, se muestran juntos y
  no aparece ningún control adicional;
- cuando no entren todos, la fila muestra los chips que quepan y un único chip
  interactivo `+N`, donde `N` es exactamente la cantidad de colecciones de
  usuario omitidas. El chip no debe depender de un número fijo de colecciones:
  la condición y el conteo se calculan con el ancho real de la card y se
  actualizan ante resize;
- el botón de overflow abre un modal informativo con todas las colecciones
  asignadas a esa captura, incluida `Historial`, cada una con el mismo chip,
  nombre y color. El modal no modifica memberships;
- si una entrada sólo pertenece a `Historial`, no se renderiza una fila de
  colecciones ni un botón de overflow en la card;
- el chip `+N` y el modal tienen nombre accesible, foco visible, Escape,
  backdrop, cierre explícito y `aria-haspopup="dialog"`. El chip conserva la
  semántica de los indicadores `+N` de tags, pero sigue siendo un botón
  interactivo que abre el listado completo;
- el layout mantiene la card cuadrada, sus acciones, previews, tags y el
  controlador singleton de drag and drop. Los chips siguen siendo
  informativos; el único control interactivo nuevo es el chip `+N`.

La detección de overflow debe usar `ResizeObserver` o una estrategia
equivalente basada en `scrollWidth`/`clientWidth`, sin asumir que una card
siempre tiene el mismo tamaño. Los cambios de selección, focus o drag no deben
abrir el modal accidentalmente. El botón debe quedar excluido de la selección
de card y de todos los payloads de drag.

La medición debe hacerse contra el ancho intrínseco del conjunto completo de
colecciones de usuario y contra el ancho disponible de la fila, antes de
recortar el subconjunto visible. No es válido medir únicamente la fila que ya
fue derivada como visible: si esa fila ya está recortada, `scrollWidth` puede
coincidir con `clientWidth` y ocultar el overflow real. La tira o elemento de
medición debe pertenecer al mismo contexto de la card, ser accesible desde el
layout de `HistoryCard` y usar la misma tipografía, padding, borde y gaps que
los chips pintados; no debe quedar fuera del alcance del selector que la mide.
El primer cálculo debe ejecutarse después de que el DOM hidratado tenga
geometría, y repetirse cuando cambien las memberships, las fuentes o el ancho
de la card. La vista debe converger en este orden: medir todo el conjunto,
determinar si existe overflow, reservar el ancho real del `+N` y recién después
derivar los chips visibles. Si no existe overflow, no se muestra el `+N`.

Un valor inválido que llegara desde una base antigua no debe romper la card:
el backend debe normalizarlo durante la migración y el frontend usa un color
seguro de fallback sólo para render, dejando el error diagnosticable sin
exponer contenido sensible.

## 7. Verificación

Automática:

- migración desde una base anterior, valor por defecto y rollback;
- selección inicial dentro de la paleta y persistencia tras reinicio;
- validación de HEX, actualización idempotente y protección de memberships;
- comando Tauri, evento de actualización y serialización del DTO;
- sidebar, doble click, teclado, modal guardar/cancelar/Escape;
- card con cero, una y varias colecciones, chips con el estilo de tags, color
  por colección, detección responsive de overflow, conteo `+N` y modal con
  listado completo;
- regresiones del controlador de drag and drop y del layout protegido.

Manual:

- Ubuntu GNOME Wayland: crear colección, abrir picker, cambiar color,
  reiniciar y comprobar sidebar/cards;
- Ubuntu X11: repetir el flujo y comprobar que no se usa una API X11 para el
  picker;
- macOS: repetir el flujo en WKWebView;
- confirmar que color, memberships, tags, imágenes y assets sobreviven a
  reinicios y que Quick Paste no cambia.
