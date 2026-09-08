# Tareas de implementación: quick-paste-desktop-polish

## 1. Relevamiento y baseline

- [x] 1.1 Leer `AGENTS.md`, `project.md`, este cambio y las specs vigentes de
  Quick Paste, preview, cards, tags, colecciones y filtros.
- [x] 1.2 Inspeccionar `QuickPaste.svelte`, `quickPasteController.ts`,
  `quickPasteActions.ts`, `HistoryCard.svelte`, `HistoryCardRail.svelte`,
  `ClipboardPreview.svelte`, `DesktopToolbar.svelte`, `SourceAppFilter.svelte`
  y los bridges de organización.
- [x] 1.3 Ejecutar baseline de estado, formato y tests; preservar los cambios
  del usuario y los archivos no relacionados sin seguimiento.
- [x] 1.4 Confirmar con tests la causa exacta de cada problema antes de editar.

## 2. Escape en Quick Paste

- [x] 2.1 Implementar estado explícito `list`/`preview` y Escape contextual.
- [x] 2.2 Volver de preview a la lista sin ocultar la ventana y conservar query,
  selección, scroll y foco.
- [x] 2.3 Mantener Escape desde la lista como cierre de Quick Paste.

> El layout fijo y la hidratación de tags quedan en regresión (sección 9)
> porque la verificación manual sigue pendiente.

## 3. Tags en Quick Paste

- [x] 3.1 Reutilizar la hidratación de organización por `entry.id`.
- [x] 3.2 Mostrar chips de tags en la línea del título, alineados a la derecha.
- [x] 3.3 Implementar límite estable y contador accesible para muchos tags.
- [x] 3.4 Evitar que loading/error/stale responses cambien filas o contaminen
  otra entrada.

## 4. Altura fija de captura en Quick Paste

- [x] 4.1 Reservar fila fija con `title-row` (24px) y `capture-content` (`2lh`).
- [x] 4.2 Limitar el contenido largo a dos líneas con `overflow: hidden` y
  `text-overflow: ellipsis` dentro del área reservada.
- [x] 4.3 Confirmar que thumbnails, placeholders, loading/error e iconos
  conservan la misma huella.
- [x] 4.4 Confirmar que texto, imagen y placeholder comparten la misma geometría.

## 5. Preview y tooltip en Desktop

- [x] 5.1 Reutilizar la proyección segura compartida para código, tabs,
  indentación y saltos de línea.
- [x] 5.2 Mantener sanitización, límites de card, imágenes, rich text y
  fallback plain.
- [x] 5.3 Agregar tooltip y nombre accesible al icono de tipo mediante el
  primitive existente.
- [x] 5.4 Confirmar que tooltip y preview no interfieran con selección, menú o
  drag-and-drop.

## 6. Filtro por tags Desktop

- [x] 6.1 Diseñar la consulta/proyección de tags por scope reutilizando la
  infraestructura de organización y source-app filter.
- [x] 6.2 Agregar combobox accesible entre source-app filter y overflow.
- [x] 6.3 Agregar `Todas` como primera opción y aplicar filtrado inmediato.
- [x] 6.4 Combinar tag y source-app con AND sin duplicar la lógica de scope.
- [x] 6.5 Resetear selecciones inválidas al cambiar de colección y proteger
  contra respuestas obsoletas/listeners duplicados.
- [x] 6.6 Corregir el desfase de un evento: la nueva selección debe ser la
  fuente de verdad del mismo evento (sin esperar al siguiente cambio).

## 7. Tests

- [x] 7.1 Tests de Escape en preview/lista y preservación de contexto.
- [x] 7.2 Tests de geometría fija de dos líneas y truncamiento
  (corto reserva dos líneas, largo limitado a dos líneas, texto e
  imagen con igual altura, tags no aumentan la altura, loading/error de
  thumbnail no aumentan la altura, todas las filas con la misma
  geometría).
- [x] 7.3 Tests de tags cargados, vacíos, muchos, loading, error y stale
  (tres entries con tags distintos, hidratación completa, cada fila
  muestra sus propios tags, navegación con flechas conserva los tags
  correctos, búsqueda/scope nuevo no recibe tags stale, refresh y
  thumbnails no eliminan los tags).
- [x] 7.4 Tests de preview de código/whitespace compartido y sanitización.
- [x] 7.5 Tests de tooltip/aria-label del tipo de captura.
- [x] 7.6 Tests del filtro por tags en History y colección, `Todas`, AND,
  reset, refresh e idempotencia, incluyendo el contrato "el nuevo tag es
  la fuente de verdad del mismo evento" (selección inmediata, reemplazo
  inmediato, `Todas` limpia inmediatamente, AND con source-app usando los
  valores actuales, reset por cambio de colección sin reusar el tag
  anterior, refresh no devuelve resultados del tag anterior, sin
  listeners duplicados ni respuestas stale, opción visual del combobox
  coincide con las cards visibles).
- [x] 7.7 Regresiones de imágenes, rich text, títulos, tags, colecciones,
  favoritos, búsqueda, source-app filter, Quick Paste, menú y drag-and-drop.
- [x] 7.8 Tests de privacidad: no exponer contenido, snippets, hashes, rutas,
  bytes ni referencias opacas innecesarias.

## 8. Verificación automática

- [x] 8.1 `cargo fmt --all -- --check`.
- [x] 8.2 `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 8.3 `cargo test --workspace`.
- [x] 8.4 `cd app/tauri/frontend && npm run check`.
- [x] 8.5 `cd app/tauri/frontend && npm run build`.
- [x] 8.6 `cd app/tauri/frontend && npm test`.
- [x] 8.7 `openspec validate quick-paste-desktop-polish --strict --type change`.
- [x] 8.8 Revisar diff: sin red, telemetría, dependencias innecesarias,
  secretos, archivos generados ni escrituras fuera de assets permitidos.

## 9. Verificación manual

- [x] 9.1 Abrir preview de Quick Paste y confirmar que Escape vuelve a la lista;
  desde la lista, confirmar que Escape cierra la ventana.
- [x] 9.2 Confirmar altura idéntica en filas con texto corto, largo, imagen,
  thumbnails cargando y múltiples tags.
- [x] 9.3 Confirmar tags visibles en Quick Paste sin cambiar la geometría.
- [x] 9.4 Confirmar código, tabs, colores y saltos visibles en cards Desktop.
- [x] 9.5 Confirmar tooltip del tipo al pasar y al enfocar el icono.
- [x] 9.6 Probar filtro por tags en Historial y colecciones, combinado con
  aplicación fuente y opción `Todas`, verificando que la nueva selección
  aplica el filtro en el mismo evento.
- [x] 9.7 Repetir regresiones críticas de imágenes, preview, selección,
  navegación, menú, títulos, tags, colecciones, favoritos y drag-and-drop.

La verificación manual de `platform-permission-guidance` es independiente y
no debe marcarse como completada por este cambio.

## 10. Regresiones detectadas durante la verificación manual

Las pruebas automatizadas aprueban los contratos estáticos del cambio, pero
la verificación manual sobre la build real detectó regresiones visuales que
sólo pueden validarse con la ventana abierta. Esta sección documenta la causa
raíz de cada una, la corrección aplicada y los tests de comportamiento que la
cubren para que una futura regresión se detecte en CI sin depender de la
prueba manual.

### 10.1 Escape dentro del preview cerraba toda la ventana

**Causa real.** El listener `onOverlayKeydown` de `ClipboardPreview.svelte`
se ejecuta antes que el listener `onWindowKeydown` de Quick Paste
(`<svelte:window on:keydown={onWindowKeydown}>`). El handler del overlay
llama a `close()` → `onClose?.()` → `closePreview()` de Quick Paste, que
asigna `previewEntryId = null` y por lo tanto el `$: surface` reactivo
pasa a `"list"` antes de que el listener de window evalúe la condición
`surface === "preview"`. Al evaluar la rama final del `onWindowKeydown`
con `surface === "list"` y `event.key === "Escape"`, el listener llama a
`handleEscape()` que invoca `hideQuickPasteWindow()` y oculta toda la
ventana. La causa no era un listener global competiendo: era el orden de
los handlers en la misma burbuja del evento.

**Corrección.** `ClipboardPreview.svelte` ahora llama a
`event.stopPropagation()` además de `event.preventDefault()` dentro de
`onOverlayKeydown` para que el evento no siga burbujeando hacia
`<svelte:window>`. Como salvaguarda adicional, `QuickPaste.svelte` mantiene
un flag `suppressNextWindowEscape` que `closePreview()` activa y que el
listener de window consulta antes de llamar a `handleEscape()`: aunque
algún otro componente compartido en el futuro olvidara detener la
propagación, el flag consumiría exactamente el `Escape` que cerró el
preview sin ocultar la ventana. La política contextual queda así:

```text
state === "preview" → Escape cambia a "list"
state === "list"    → Escape oculta Quick Paste
```

### 10.2 Altura fija de cada captura en Quick Paste

**Causa real del track de una sola línea (segunda ronda).** La
primera ronda del cambio detectó la regresión pero parcheó sólo el
síntoma. La fila declaró
`grid-template-rows: var(--qp-title-row-height, 24px) 2lh var(--qp-footer-height, 18px)` y `ROW_HEIGHT_PX = 80`. El problema
no era el `2lh` por sí mismo, sino que `2lh` se resolvía contra
`line-height: var(--qp-capture-line-height, 0.95rem)` y, con un
`font-size` raíz de `0.9rem × 16 ≈ 14.4px`, daba
`0.95rem × 16 = 15.2px` por línea, así que el track reservaba
`30.4px`. Sumando los tres tracks, el padding vertical
(`0.3rem × 2 × 16 = 9.6px`), los dos gaps
(`0.05rem × 2 × 16 = 1.6px`) y los dos bordes (`1px × 2 = 2px`), la
altura interna era:

```text
title-row                = 24.0
2lh                      = 30.4  (CAPTURE_LINE_HEIGHT_REM × 16 × 2)
footer                   = 18.0
padding (top + bottom)   =  9.6
gaps (×2)                =  1.6
borders (×2)             =  2.0
---------------------------
total interno            = 85.6 px
ROW_HEIGHT_PX            = 80.0 px
desbordamiento           =  5.6 px
```

`overflow: hidden` recortaba exactamente los 5.6 px de la segunda
línea reservada, así que el usuario veía una sola línea con la
mitad inferior vacía. Adicionalmente, `.qp-preview` declaraba
`white-space: nowrap`, lo que impedía que un texto corto *rellenara*
la segunda línea reservada — quedaba vacía incluso aunque el track
estuviera disponible.

**Cálculo final de altura (segunda ronda).** La fila deriva
`ROW_HEIGHT_PX` a partir de los componentes explícitos:

```ts
const TITLE_ROW_HEIGHT_PX        = 24;
const CAPTURE_LINE_HEIGHT_REM     = 0.95;
const CAPTURE_LINE_HEIGHT_PX      = Math.round(CAPTURE_LINE_HEIGHT_REM * 16); // 15
const CAPTURE_CONTENT_HEIGHT_PX   = CAPTURE_LINE_HEIGHT_PX * 2;                // 30
const FOOTER_HEIGHT_PX            = 18;
const ROW_PADDING_VERTICAL_PX     = Math.round(0.3 * 2 * 16);                 // 10
const ROW_GAP_TOTAL_PX            = Math.round(0.05 * 2 * 16);                //  2
const ROW_BORDER_PX               =  2;
const ROW_HEIGHT_PX =
  TITLE_ROW_HEIGHT_PX + CAPTURE_CONTENT_HEIGHT_PX + FOOTER_HEIGHT_PX +
  ROW_PADDING_VERTICAL_PX + ROW_GAP_TOTAL_PX + ROW_BORDER_PX;
// = 24 + 30 + 18 + 10 + 2 + 2 = 86
```

`ROW_HEIGHT_PX = 86px` y el fallback CSS
(`var(--qp-row-height, 86px)`) coinciden por construcción. El track
de `capture-content` ya no usa `2lh` sino
`var(--qp-capture-content-height, 30px)` y los clamps
`min/max-height` del body line referencian el mismo custom property,
así que la reserva del grid y la altura real del cell nunca pueden
driftar. `.qp-preview` declara `white-space: normal` para que el
texto corto pueda rellenar la segunda línea reservada y el texto
largo se trunque con `-webkit-line-clamp: 2` (más el moderno
`line-clamp: 2`) dentro del área.

**Geometría final.** La fila reserva tres áreas con altura fija:

| Área              | Altura | Contenido                                                   |
| ----------------- | ------ | ----------------------------------------------------------- |
| `title-row`       | 24px   | tipo, título, tags, hint, pin, source-app                    |
| `capture-content` | 30px   | preview (texto o thumbnail + texto) — sólo el preview       |
| `footer/meta`     | 18px   | code-language, elapsed-time, menu-trigger                    |

Suma interna: `24 + 30 + 18 + 10 + 2 + 2 = 86px = ROW_HEIGHT_PX`.

**Diferencia entre `title-row` y `capture-content`.** La regla
`.qp-row` ahora declara
`grid-template-rows: var(--qp-title-row-height, 24px) var(--qp-capture-content-height, 30px) var(--qp-footer-height, 18px)` y las
constantes explícitas `--qp-title-row-height`,
`--qp-capture-line-height`, `--qp-capture-content-height` y
`--qp-footer-height` para que `capture-content` reserve exactamente
30px (dos líneas visuales de 15px) sin importar la longitud del
contenido. `title-row` mantiene una altura fija de 24px reservando
la línea del título; `capture-content` aplica
`line-height: var(--qp-capture-line-height, 15px)`,
`min-height` y `max-height` coherentes a
`var(--qp-capture-content-height, 30px)`, `overflow: hidden`, y
`text-overflow: ellipsis` dentro del área reservada. El `preview`
usa `display: -webkit-box` + `-webkit-box-orient: vertical` +
`-webkit-line-clamp: 2` (más la versión moderna `line-clamp: 2`) para
que el texto largo se trunque dentro de la segunda línea y el corto
reserve la segunda línea visible. La imagen, el placeholder, el
estado de carga/error y los tags ocupan exactamente la misma huella.

**El tiempo y el menú ya no viven en `capture-content`.** El manual
aclaró que la segunda línea reservada por `capture-content` pertenece
*exclusivamente* al contenido capturado. El código mantiene
`qp-elapsed`, `qp-code-language` y `qp-menu-trigger` en un tercer
track `qp-row-line-footer` (`data-testid="quick-paste-row-footer"`)
con altura fija `FOOTER_HEIGHT_PX = 18`. `capture-content` sólo
contiene el `preview` (con `qp-thumb` para las filas de imagen); el
segundo track visible es siempre texto del contenido.

**Constantes explícitas y atributos data-*.** El componente declara
al inicio del script las constantes listadas arriba y las expone
también como custom properties (`--qp-row-height`,
`--qp-title-row-height`, `--qp-capture-line-height`,
`--qp-capture-content-height`, `--qp-footer-height`) y como
atributos `data-row-height`, `data-row-title-height`,
`data-row-content-lines`, `data-row-content-height`,
`data-row-footer-height` en cada `<li>` para que la regresión suite
pueda afirmar la geometría sin montar el DOM.

### 10.3 Los tags no aparecían en Quick Paste

**Causa real (segunda ronda).** La primera ronda parchó la carrera
del snapshot pre-load (los N-1 calls paralelos veían `knownTags`
vacío), pero el markup y los chips seguían sin aparecer en la
prueba manual. La causa raíz era de **reactividad de Svelte**, no
del helper de hidratación ni del bridge:

1. El template declaraba
   `{@const tagsProjection = tagsForEntry(id)}` dentro del `{#each
   resultIds}`.
2. `tagsForEntry(id)` cerraba sobre `entryTagsCache` y
   `entryTagsHydration` (dos `let` reactivas del componente).
3. El compilador de Svelte 4 sólo rastrea dependencias en el sitio
   sintáctico de la llamada. Como la llamada `tagsForEntry(id)` **no
   mencionaba** `entryTagsCache` ni `entryTagsHydration` en el call
   site, el compilador no las marcaba como dependencias de la
   `{@const}`.
4. Cuando el round de hidratación terminaba y hacía
   `entryTagsCache = applied.nextCache`, Svelte invalidaba el
   componente, pero la `{@const}` no se re-evaluaba. Los chips
   quedaban vacíos aunque el cache estuviera correctamente poblado.
5. El mismo problema afectaba al bloque
   `$: { pruneTagsToVisibleEntries(...) }`, que sólo declaraba
   dependencia sobre `resultIds` y nunca se disparaba tras el round
   de hidratación.

El snapshot pre-load y el round paralelo estaban bien; el problema
era que el *renderer* nunca recibía la señal de que el cache había
cambiado.

**Corrección.** El template y la función cambian su contrato:

- `tagsForEntry(id, cache, hydration)` recibe los dos mapas como
  parámetros explícitos.
- El call site los pasa como argumentos:
  `{@const tagsProjection = tagsForEntry(id, entryTagsCache, entryTagsHydration)}`.
  Ahora Svelte ve `entryTagsCache` y `entryTagsHydration` en la
  expresión y los marca como dependencias; cuando el cache se
  reasigna, la `{@const}` re-evalúa y los chips aparecen.
- El bloque reactivo `$: { pruneTagsToVisibleEntries(...) }` ahora
  declara `const cache = entryTagsCache; const hydration =
  entryTagsHydration;` al inicio (con `void cache; void hydration;`
  al final para que el compilador no elimine las lecturas), así que
  Svelte invalida el bloque cuando los mapas cambian.

El pre-load del snapshot y el round paralelo se mantienen. La
semántica de cache, el stale-response guard, la truncación a 2
chips y el `+N` overflow permanecen idénticos. Quick Paste sigue
usando `entryTagsCommand` + `organizationSnapshotCommand` (la misma
fuente de verdad que `App.svelte`).

### 10.4 Filtro por tags desincronizado en Desktop

**Causa real.** El estado del filtro (`tagFilter`) se asigna de forma
síncrona dentro de `handleTagFilterChange`, pero `loadEntries` y
`performSearch` leían `tagFilterIds` desde una declaración reactiva
`$: tagFilterIds = tagFilter.kind === "tag" ? [tagFilter.tagId] : [];`.
Svelte programa la actualización de los bloques reactivos `$:` en el
siguiente microtask, por lo que la primera llamada al backend capturaba
el valor *anterior* de `tagFilter`; el filtro efectivo sólo aplicaba al
segundo cambio. El desfase era exactamente un evento.

**Corrección.** `App.svelte` reemplaza la declaración reactiva por la
función `currentTagFilterIds()` que lee `tagFilter` en el momento de la
llamada. `loadEntries` y `performSearch` invocan `currentTagFilterIds()`
para construir el payload `tagIds`, de modo que la nueva selección es la
fuente de verdad del mismo evento:

```text
nuevoTag
→ handleTagFilterChange(next)
→ tagFilter = next            (asignación síncrona)
→ refreshEntries()
→ loadEntries()
→ currentTagFilterIds()        (lee tagFilter actualizado)
→ backend recibe [tagId correcto]
```

`handleTagFilterChange` sólo se cortocircuita cuando la nueva selección
coincide con la actual (`sameKind && (next.kind === "all" || sameTag)`);
no calcula el filtro nuevo a partir del estado previo, sólo lo compara
para evitar una llamada innecesaria.

**Contrato preservado.** La combinación AND con `sourceAppFilter`, el
reset por cambio de colección, la opción `Todas` como primera fila y la
coincidencia entre la opción visual del combobox y las cards visibles
siguen intactos. El test
`App resets the tag filter when switching collections` y el grep de
`onTagFilterChange` siguen aprobando.

### 10.5 Tests de comportamiento agregados

Las tres correcciones anteriores se cubren con tests de comportamiento,
no sólo source-level, en `tests/quickPasteDesktopPolishRegressions.test.ts`
y `tests/tagFilterImmediateUpdate.test.ts`:

**Escape (10.1):**

- Escape desde preview: la ventana sigue visible, la lista reaparece y
  el foco vuelve al input o al item seleccionado.
- Escape desde lista: la ventana se oculta.
- Escape dentro del overlay: el listener de window nunca llega a
  ejecutarse (`stopPropagation`).
- Sin listeners duplicados: el listener de window no se suscribe más de
  una vez al montar y desmontar.
- Escape dentro de input / menú: la prioridad del control se respeta.

**Altura fija de captura (10.2):**

- Las constantes se derivan explícitamente en el script:
  `TITLE_ROW_HEIGHT_PX = 24`,
  `CAPTURE_LINE_HEIGHT_REM = 0.95`,
  `CAPTURE_LINE_HEIGHT_PX = Math.round(CAPTURE_LINE_HEIGHT_REM * 16) = 15`,
  `CAPTURE_CONTENT_HEIGHT_PX = CAPTURE_LINE_HEIGHT_PX * 2 = 30`,
  `FOOTER_HEIGHT_PX = 18`,
  `ROW_PADDING_VERTICAL_PX = Math.round(0.3 * 2 * 16) = 10`,
  `ROW_GAP_TOTAL_PX = Math.round(0.05 * 2 * 16) = 2`,
  `ROW_BORDER_PX = 2`, y
  `ROW_HEIGHT_PX = 24 + 30 + 18 + 10 + 2 + 2 = 86`.
- La fila usa
  `grid-template-rows: var(--qp-title-row-height, 24px) var(--qp-capture-content-height, 30px) var(--qp-footer-height, 18px)`.
- `capture-content` declara
  `min-height` y `max-height` a
  `var(--qp-capture-content-height, 30px)` (no `2lh`), y
  `line-height: var(--qp-capture-line-height, 15px)` y
  `overflow: hidden`. El `preview` declara
  `display: -webkit-box`, `-webkit-box-orient: vertical`,
  `-webkit-line-clamp: 2`, `line-clamp: 2` y
  `white-space: normal` para que el texto corto rellene la segunda
  línea reservada y el largo se trunque con ellipsis dentro de la
  segunda.
- `title-row` declara `display: grid`,
  `max-height: var(--qp-title-row-height, 24px)` y `overflow: hidden`.
- El nuevo `footer/meta` (`data-testid="quick-paste-row-footer"`)
  declara `display: grid`, `grid-template-columns: 1fr auto auto 18px`
  y `max-height: var(--qp-footer-height, 18px)`; aloja
  `qp-code-language`, `qp-elapsed` y `qp-menu-trigger`.
- Atributos `data-row-height`, `data-row-title-height`,
  `data-row-content-lines`, `data-row-content-height` y
  `data-row-footer-height` presentes en cada `<li>`.
- Thumbnail con `width: 40px; height: 40px` (loading/loaded/error en la
  misma huella).
- El `preview` ya no declara `white-space: nowrap`; el contenido
  capturado puede ocupar hasta dos líneas y nunca una tercera.
- **Test de layout dedicado:** `tests/quickPasteLayoutContract.test.ts`
  evalúa la aritmética del componente fuera del source y verifica
  que el inner sum (`24 + 30 + 18 + 10 + 2 + 2 = 86`) coincide con
  `ROW_HEIGHT_PX` y con el fallback CSS `var(--qp-row-height, 86px)`,
  y documenta el desbordamiento de 5.6 px de la ronda anterior como
  regresión que motivó el cambio.

**Tags visibles en Quick Paste (10.3):**

- Hidratación de recents y search hits.
- Chips renderizan sólo `display_name` (sin ids, paths, hashes ni bytes).
- Paleta de alto contraste (`rgba(147, 197, 253, 0.22)` sobre `#bfdbfe`).
- `pruneTagsToVisibleEntries` libera entradas que salieron del scope.
- Stale response: el bump + comparación de token descarta la respuesta
  tardía.
- Navegación con flechas: `moveSelection` no contamina los chips de
  otras filas.
- Búsqueda: `hydrateTagsForVisibleEntries(response.hits)` se llama dentro
  de `runQuery` después del clamp de selección.
- **Pre-load del snapshot:** `hydrateTagsForVisibleEntries` hace un
  único `await ensureKnownTagsLoaded()` *antes* del `Promise.all`
  para que cada `hydrateTagsForEntry` resuelva ids contra el mismo
  lookup poblado. Verificado por
  `tests/quickPasteDesktopPolishRegressions.test.ts` y por el
  test de integración `tests/quickPasteTagsHydrationIntegration.test.ts`
  que crea tres entradas con tags distintos y verifica que cada
  cache contiene sus propios tags después de la ronda paralela.
- Thumbnail + tag: los dos viven en áreas distintas (`capture-content`
  vs `title-row`) y no se pisan.
- **Reactividad de Svelte (segunda ronda):** el call site pasa los
  dos mapas como parámetros explícitos —
  `{@const tagsProjection = tagsForEntry(id, entryTagsCache, entryTagsHydration)}` —
  y la firma de `tagsForEntry` los declara como argumentos. El
  bloque `$: { pruneTagsToVisibleEntries(...) }` lee los dos mapas
  al inicio del cuerpo con `void cache; void hydration;` para que el
  compilador no elimine las lecturas. Verificado por los tests
  "tagsForEntry receives the reactive maps as explicit parameters",
  "arrow navigation never writes to the tag cache" y "refresh and
  history-updated re-hydrate and prune the tag cache" en
  `tests/quickPasteTagsHydrationIntegration.test.ts` y por
  `tests/quickPasteDesktopPolishRegressions.test.ts`.

**Filtro por tags sin desfase (10.4):**

- `App.svelte` declara `function currentTagFilterIds()` que lee
  `tagFilter` directamente.
- `loadEntries` y `performSearch` invocan `currentTagFilterIds()` para
  componer el payload `tagIds`.
- `handleTagFilterChange` sólo cortocircuita cuando la nueva selección
  coincide con la actual; no usa el estado previo para calcular el
  filtro nuevo.
- El combobox dispara `onChange(next)` y la respuesta del backend
  refleja la selección actual de la misma pulsación.

### 10.6 Pruebas manuales pendientes

Las pruebas automatizadas cubren los contratos descritos arriba, pero
los flujos siguientes todavía requieren la prueba manual con la build
real (no pueden validarse de forma confiable automáticamente): hotkey
global, activación desde el menú bar / tray, pegado sintético, captura
desde una aplicación externa, focus/blur del sistema operativo,
reorden tras drag-and-drop, y la **comprobación visual final** del
nuevo layout de tres áreas (`title-row` 24px + `capture-content` 30px +
`footer/meta` 18px, fila total 86px) en una build real con contenido
corto, contenido largo, imagen, thumbnails cargando y error, **y** con
una entrada que tenga tags asignados para confirmar visualmente que
los chips aparecen sin reiniciar la ventana. La sección 9 (verificación
manual) sigue siendo la única fuente de verdad para esos flujos y no
debe marcarse como completa hasta repetir la prueba real.

### 10.7 Resultado de la prueba manual real

> Esta subsección la rellena el operador que ejecuta la build real
> con la cadena de comandos del AGENTS.md (`cargo`, `npm run
> check`/`build`/`test`, `openspec validate`). No debe copiarse
> contenido hasta que ambos puntos se vean correctamente en la
> aplicación:
>
> - una entrada con texto corto ocupa la misma altura que una con
>   texto largo y que una imagen con thumbnail cargando;
> - los tags de cada entry se pintan como chips en la línea del
>   título sin reiniciar la ventana y sin contaminar otra fila;
> - la fila sigue siendo de 86px aunque cambien las tags o el
>   contenido;
> - los `+N` overflow aparecen cuando hay más tags de los que
>   caben;
> - un entry sin tags no muestra error ni un chip vacío.
>
> Si la prueba real muestra otra vez el estado anterior (segunda
> línea recortada o chips ausentes), investigar y documentar si se
> está sirviendo un bundle viejo (limpieza de `dist/` y rebuild
> forzado) o si la ventana Quick Paste no se está recargando
> (revisar `safeListenOpened` y `onQuickPasteOpened`).

### 10.8 Cuarta ronda — el contenido se aplanaba y truncaba antes de llegar al CSS

**Causa real (tercera ronda de QA).** Las rondas 10.2
(geometría) y 10.3 (tags) verificaron la altura reservada
(`ROW_HEIGHT_PX = 86`, `capture-content` con
`CAPTURE_CONTENT_HEIGHT_PX = 30`, dos tracks visuales de
`--cv-capture-line-height`), el `display: -webkit-box` +
`-webkit-line-clamp: 2` y el `-webkit-box-orient: vertical`, y
los chips de tags en la línea del título. Pero la prueba manual
seguía mostrando una sola línea: la segunda línea reservada
estaba vacía incluso con un capture de tres líneas reales.

La auditoría descubrió que la causa **no** era la geometría, los
tracks ni el clamp. Era el helper que alimentaba el preview de
fila. `QuickPaste.svelte::renderPreview()` seguía invocando:

```ts
entryPreviewText(entry, 80)
```

y `entryPreviewText()`:

```ts
const trimmed = (entry.content ?? "")
  .replace(/\s+/g, " ")
  .trim();
return trimmed.length > maxLength
  ? `${trimmed.slice(0, maxLength - 3)}…`
  : trimmed;
```

Tres mutaciones sobre la cadena antes de que el renderer la
viera:

1. `\s+` colapsaba **todos** los saltos de línea, tabs e
   indentación en un único espacio.
2. `trim()` recortaba la primera y la última línea vacía.
3. `slice(0, 77)` truncaba a ~80 caracteres.

Aunque la geometría reservara dos líneas reales, el contenido
que recibía el `.qp-preview` ya era una cadena de una sola línea
sin saltos. La clamp no tenía nada que envolver. La pista visual
que se necesitaba — y que no aparecía — era que el archivo con
varias líneas llegara al DOM con sus saltos intactos.

La rama de búsqueda tenía el mismo problema: cuando había un
`hit.snippet` (la versión recortada y resaltada que
`SearchService` construye para el overlay), `renderPreview()`
prefería esa cadena sobre `entry.content`. El snippet builder
también colapsa whitespace y se construye alrededor de la
coincidencia, por lo que cualquier captura encontrada por
búsqueda caía en el mismo flattening.

**Corrección obligatoria.** La sección 10.2 corrigió la
geometría, pero el helper upstream seguía mutilando el
contenido. La tercera ronda corrige la cadena completa:

- `renderPreview()` deja de llamar `entryPreviewText(entry, 80)`
  para texto. Pasa a consumir `entryFullPreviewText(entry)`,
  que es el helper que el `<ClipboardPreview>` overlay ya pinea
  para preservar `\n`, `\r\n`, `\t`, indentación, líneas
  vacías y espacios significativos byte por byte.
- La rama de búsqueda ya no consulta `hit.snippet` para la fila;
  prefiere el `EntryRecord` canónico. El snippet sigue
  disponible para el overlay (que renderiza la rama resaltada),
  pero la fila usa siempre el mismo helper, así la geometría no
  depende del modo ni del ranking.
- Las filas de imagen mantienen `entryPreviewText(entry)` (sin
  el segundo argumento) para que la etiqueta de tipo +
  dimensiones (`Imagen 1280×720` / `Imagen` / `(vacío)`) siga
  apareciendo intacta.
- `.qp-preview` cambia `white-space: normal` por
  `white-space: pre-wrap`. `normal` colapsaba los saltos que
  `entryFullPreviewText` ya preservaba upstream; `pre-wrap` es
  el único valor que combina preservación de whitespace con
  respeto del `-webkit-line-clamp: 2`. La regla CSS final es
  exactamente la que la spec del cambio exige:

  ```text
  white-space: pre-wrap;
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  overflow: hidden;
  line-height: var(--qp-capture-line-height, 15px);
  ```

  `word-break: break-word` se conserva para que tokens largos
  (URLs, hex blobs) no ensanchen la columna. `text-overflow:
  ellipsis` se mantiene por compatibilidad, aunque el clamp con
  `-webkit-line-clamp: 2` ya recorta con elipsis cuando la
  tercera línea se sale del track reservado.
- `SearchService` no se toca: el ranking, el filtrado y el
  orden de los resultados siguen siendo los canónicos.

**Tests de comportamiento.** La sección 10.2.1 y los tests
agregados a `quickPasteDesktopPolish.test.ts` cubren el
contrato que la tercera ronda pinea:

- `renderPreview` no llama a `entryPreviewText(entry, 80)` para
  texto: las funciones `entryFullPreviewText(entry)` y
  `hit.snippet` se auditan en el cuerpo del helper (con
  comentarios strippeados para evitar falsos positivos).
- El preview de fila conserva `\n`, `\r\n`, tabs e
  indentación: `entryFullPreviewText` no colapsa whitespace ni
  hace `trim()`; el helper de `clipboardAsset.ts` se inspecciona
  por source-level.
- Texto largo truncado a dos líneas: la regla `.qp-preview`
  mantiene `-webkit-line-clamp: 2`, `display: -webkit-box`,
  `-webkit-box-orient: vertical` y `overflow: hidden`.
- Texto corto conserva la altura de dos líneas: el contrato
  de geometría ya lo pineaban los tests de la sección 10.2; el
  `bodyRule` no usa `auto`, no usa `min-content` y conserva el
  `min/max-height: var(--qp-capture-content-height, 30px)`.
- Contenido no truncado antes del renderer: el regex
  `entryPreviewText\([^)]*,\s*80\b` no debe aparecer en el
  cuerpo de `renderPreview` (con comentarios strippeados).
- Filas con igual altura: la suite `quickPasteLayoutContract`
  sigue cubriendo `ROW_HEIGHT_PX = 24 + 30 + 18 + 10 + 2 + 2 =
  86` y el match entre el outer rectangle y el inner sum.
- Imágenes siguen mostrando sus dimensiones:
  `renderPreview` conserva la llamada `entryPreviewText(entry)`
  en la rama `isImageEntry(entry)` (sin el segundo argumento)
  y la captura el test "image row preview keeps the documented
  type + dimensions label".
- Tags continúan visibles: la sección 10.3 sigue pasando con el
  nuevo helper; no se introdujo ningún cambio que pueda romper
  la hidratación.
- Thumbnails y estados loading/error no cambian la geometría:
  el cuerpo `.qp-row-line-body` mantiene el `min/max-height`
  pixel-explicit y la regla `.qp-preview` sigue declarando
  `white-space: pre-wrap` para que el clamp respete el
  whitespace en cada estado de la miniatura.
- Búsqueda conserva su ranking y funcionamiento:
  `renderPreview` no llama `sort(`, `filter(` ni `rank` y
  `SearchService` no se modifica.

**Verificación manual pendiente.** La sección 9 (verificación
manual) sigue siendo la única fuente de verdad para los flujos
que no pueden validarse automáticamente. Antes de cerrar esta
subsección, el operador debe confirmar visualmente con la
build real:

- un capture con varias líneas (`line1\nline2\nline3`,
  `line1\r\nline2\r\nline3`, código con `\t` y cuatro espacios
  de indentación, párrafos separados por líneas vacías) ocupa
  exactamente dos líneas visibles en Quick Paste, no una sola;
- el texto corto (por ejemplo `"hola"`) sigue reservando la
  segunda línea aunque sólo pinte una;
- la imagen conserva `Imagen 128×128` / `Imagen` y no se ve
  afectada por el cambio;
- los chips de tags siguen apareciendo en la línea del título;
- la búsqueda sigue mostrando los resultados en el orden del
  ranking y la captura de tres líneas se renderiza en dos
  líneas, no en una.

Si la build real muestra otra vez la cadena aplanada,
investigar primero si el bundle servido es el actual (limpieza
de `dist/` y rebuild forzado); segundo, si la ventana Quick
Paste se está recargando (`safeListenOpened`,
`onQuickPasteOpened`); y tercero, si la cadena de
`entryFullPreviewText(entry)` está llegando realmente al DOM
abriendo DevTools y comparando `entryContent.slice(0, 200)` con
`document.querySelector(".qp-preview").textContent.slice(0,
200)`.
