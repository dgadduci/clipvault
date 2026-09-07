# Design: quick-paste-preview-ui

## Principios

La implementación extiende el Quick Paste actual. No debe crear una segunda
ventana, un segundo listener ni una segunda lógica de paste. Las capas deben
mantenerse separadas:

```text
QuickPaste.svelte
  → bridges y resolvers existentes
  → comandos Tauri delgados
  → servicios/core y adapters de plataforma existentes
```

## Iconos

El indicador de tipo debe usar `contentTypeIcons.ts` o el registro equivalente
ya utilizado por el frontend principal. Si falta un tipo, se agrega un icono
vectorial pequeño y consistente dentro de ese registro; no se agregan PNGs
generados ni dependencias de iconos por conveniencia.

El icono de la aplicación fuente debe reutilizar el campo de metadata y el
resolver existente (`source_app_icon_ref`/bridge correspondiente). El área
debe tener dimensiones estables durante `loading`, `loaded` y `error`:

- `loaded`: muestra el icono real;
- `loading`: muestra un placeholder visual distinguible, no un cuadrado vacío;
- `error` o metadata ausente: muestra un icono genérico de aplicación y deja
  el nombre sólo como información accesible, no como texto principal.

Las respuestas obsoletas no pueden reemplazar el icono de otra fila. Los
Object URLs deben liberarse con el lifecycle existente.

La fuente de verdad visual y funcional es el desktop principal: Quick Paste
debe compartir el registro de iconos de tipo, los tokens tipográficos y los
resolvers ya utilizados por `HistoryCard.svelte`. Si se necesita una
adaptación, debe ser un helper compartido y no un mapa paralelo.

## Menú de cada item

El botón `...` abre un popover accesible anclado a la fila, sin cambiar la
altura fija del item y sin quedar recortado por el contenedor scrolleable.

| Tipo de entrada | Acciones de copia | Acción común |
| --- | --- | --- |
| Texto plano, shell, JSON, HTML u otro tipo no-rich | `Copiar` | `Previsualizar` |
| Texto con rich y plain | `Copiar texto enriquecido`, `Copiar texto plano` | `Previsualizar` |
| Imagen | `Copiar` | `Previsualizar` |

`Copiar` y las variantes rich/plain del menú escriben la representación
seleccionada en el portapapeles mediante el mismo flujo copy-only de
`Enter`/`Shift+Enter`. No ejecutan `pasteEntryCommand`, no envían un pegado
sintético, no ocultan ni vuelven a mostrar Quick Paste tras el éxito y no
crean una nueva captura. El texto de cada acción debe describir la operación
real para no sugerir que ClipVault pega en otra aplicación.

`Previsualizar` no copia, no pega, no crea historial y no modifica favoritos,
tags, colecciones, timestamps o la aplicación activa. Pin/unpin sigue siendo
un control independiente.

El popover debe abrirse efectivamente al activar `...`; sus acciones deben
quedar dentro del viewport de la ventana y ser visibles aun cuando la fila
esté cerca del borde inferior del listview.

## Previsualización

La previsualización se muestra como un overlay/modal dentro de la ventana fija
de Quick Paste. No abre otra ventana.

- Atajo: `Cmd+Enter` en macOS y `Ctrl+Enter` en Linux.
- El atajo opera sobre la entrada seleccionada y no sobre el texto que se está
  escribiendo en el campo de búsqueda cuando no hay una selección válida.
- Texto: se muestra con wrapping y scroll interno; si existe preview rich
  segura se puede utilizar, con fallback a texto plano escapado.
- La preview de texto debe cargar la representación completa de la captura; no
  puede reutilizar el preview truncado que se muestra en la fila.
- HTML/rich: sólo se renderiza el preview sanitizado existente, dentro de un
  contexto sin scripts, eventos ni navegación activa.
- Imagen: se muestra usando el asset persistido, ajustada con `contain` al
  área disponible y sin modificar la referencia del asset.
- Otros tipos: se muestra su representación textual segura o un estado de
  preview no disponible.
- `Escape` cierra primero el overlay; un segundo `Escape` cierra Quick Paste.
- Al cerrar el overlay se conserva la selección y no se pierde el scroll.

El overlay debe conservar foco accesible, tener nombre y cerrar también con
click fuera cuando la interacción existente lo permita.

## Búsqueda

`Cmd/Ctrl+K` enfoca el campo existente y selecciona su contenido para permitir
reemplazar la consulta inmediatamente. El atajo se captura únicamente cuando
Quick Paste está activo; no se registra otro listener global persistente.

La barra debe mostrar una insignia adaptada a la plataforma (`⌘K` o `Ctrl K`)
sin alterar el ancho fijo de la ventana.

## Geometría y tipografía

- Mantener `720 × 520`, la lista vertical y el scroll interno.
- Aplicar radio de borde consistente de aproximadamente `16px` al shell de la
  ventana y el mismo lenguaje de borde que los items.
- Mantener el ancho y alto de cada item estables.
- Reutilizar la familia tipográfica, pesos, colores y escala de
  `HistoryCard.svelte`/tokens globales; no crear una segunda escala gigante
  para Quick Paste.
- La densidad compacta puede ajustar padding y truncamiento, pero no debe
  cambiar la jerarquía tipográfica de las cards.
- No debe producirse scroll horizontal ni crecimiento de la ventana al abrir
  el menú o el overlay.

## Privacidad y no regresiones

- Ningún icono, preview, comando o evento debe transportar contenido completo,
  bytes, hashes, rutas absolutas o identificadores sensibles en logs.
- Los assets antiguos y nuevos deben continuar cargándose y pegándose.
- `asset_ref`, metadata de imagen, tags, colecciones, favoritos y timestamps
  deben permanecer intactos salvo la mutación explícita del pin.
- Se deben conservar el drag and drop, la edición de títulos, la búsqueda por
  título y contenido, el foco del target anterior y los listeners idempotentes.
- Las pruebas que utilicen imágenes deben usar temporales y nunca
  `~/.clipvault`.

## Corrección de interacción por click

La decisión anterior de `quick-paste-actions` que hacía que el click de fila
fuera equivalente a Enter completo queda reemplazada aquí. El click debe
seleccionar y ejecutar copy-only, pero no debe llamar a `hide()` ni a `show()`
en el camino exitoso: Quick Paste debe permanecer visible sin parpadeo y el
usuario debe poder seguir seleccionando. Enter conserva su comportamiento
vigente y continúa ocultando Quick Paste después de copiar. La implementación
debe compartir el controlador de copia, con una rama explícita que no ejecute
ninguna operación de visibilidad cuando `hideAfterSuccess` es falso.

## Fidelidad de copia de imágenes

La acción `Copiar` de una imagen debe leer el asset PNG canónico completo desde
`asset_ref` y publicarlo mediante una operación de clipboard que conserve esos
bytes codificados. El core debe decodificarlo únicamente para validación,
dimensiones y supresión de la siguiente captura; no debe volver a codificarlo
para la publicación. Así, el preview y el contenido que recibe otra aplicación
son exactamente el mismo PNG, sin pasar por la miniatura, la previsualización
ni una ruta de reducción de tamaño. Un round-trip con una imagen no cuadrada y
suficientemente grande debe conservar ancho, alto y el contenido completo; las
pruebas no deben usar únicamente una imagen de un pixel ni comprobar sólo que
la operación retornó `Ok`.

El contrato se mantiene neutral al backend: macOS debe implementar la
publicación directa de PNG en `NSPasteboard` y Linux debe conservar su adapter
actual mediante el fallback bitmap existente. Si una sesión no puede escribir
imágenes, se devuelve la respuesta tipada existente; nunca se sustituye
silenciosamente por texto ni se escribe una imagen parcial.

## Consistencia visual

Quick Paste debe reutilizar los mismos tokens de familia, tamaño, peso, color
y escala tipográfica que `HistoryCard.svelte` y el registro visual principal.
Los iconos de tipo, aplicación fuente y favorito deben utilizar el mismo
registro/resolver y el mismo tamaño efectivo de desktop, sin placeholders
permanentes ni valores hardcodeados divergentes. Cualquier adaptación para la
fila compacta debe vivir en helpers o tokens compartidos.

## Cierre por pérdida de foco

Quick Paste es una paleta transitoria. Si su ventana pierde el foco de la
ventana del sistema porque el usuario activa otra aplicación, debe ocultarse
automáticamente. Esta señal debe venir del estado de foco de la ventana Tauri
o del evento equivalente disponible en la versión instalada, no de un
`blur` indiscriminado aplicado a cada elemento del DOM. El listener debe ser
idempotente y limpiarse al desmontar.

El cierre por pérdida de foco no debe dispararse cuando el foco cambia entre
controles internos, ni mientras se abre el menú, se muestra el preview o se
ejecuta una operación interna de copia.

La integración debe usar la API de foco de la ventana Tauri actualmente
instalada (`getCurrentWindow().onFocusChanged` o su equivalente exacto),
validando el payload booleano `focused`. No se debe asumir que un
`listen("tauri://blur")` con un target arbitrario representa correctamente el
estado de foco de la ventana.

La tipografía debe salir de un único sistema compartido entre la webview
principal y la de Quick Paste. Se deben comparar los estilos computados del
desktop y Quick Paste, no sólo comprobar que ambos archivos contienen una
variable CSS con el mismo nombre.

## Verificación manual

En macOS real:

1. Cerrar cualquier instancia previa y abrir la build nueva.
2. Abrir Quick Paste con `Cmd+Shift+V`.
3. Confirmar icono de tipo e icono real de aplicación en texto e imagen.
4. Abrir `...` en texto plano, rich text e imagen y comprobar las acciones.
5. Probar `Cmd+K`, escribir una consulta y confirmar el foco.
6. Seleccionar una captura y probar `Cmd+Enter` y `Previsualizar`.
7. Confirmar que Escape cierra primero la preview y luego Quick Paste.
8. Confirmar que preview no copia, no pega ni crea una card.
9. Activar `Copiar`, `Copiar texto enriquecido`, `Copiar texto plano` y
   `Copiar` de imagen desde sus menús; comprobar que sólo dejan la
   representación en el portapapeles, no ejecutan pegado sintético y Quick
   Paste permanece visible.
10. Confirmar que Enter y Shift+Enter conservan su comportamiento de copiar y
    ocultar la ventana, mientras que click y menú permanecen visibles.
11. Copiar una imagen no cuadrada desde Quick Paste, pegarla manualmente en
    otra aplicación y confirmar que se recibe completa, sin recorte.
12. Con Quick Paste visible, activar otra aplicación y confirmar que Quick
    Paste se cierra; comprobar que cambiar foco dentro de Quick Paste no lo
    cierra.

En Linux repetir con `Ctrl+K` y `Ctrl+Enter`, distinguiendo X11 y Wayland
cuando la capacidad de preview o asset no esté disponible.
