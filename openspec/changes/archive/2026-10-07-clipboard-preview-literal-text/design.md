# Diseño: conservar los caracteres originales en el preview de texto

## Fuente y representación

El preview completo continúa usando `entryFullPreviewText(entry)` como fuente
canónica. El texto de una captura no se decodifica ni se transforma para
presentarlo: por ejemplo, una comilla se muestra como comilla y una secuencia
capturada literalmente como `&quot;` permanece como `&quot;`.

La rama de texto sin resaltado debe pasar el valor original como interpolación
de texto de Svelte (`{fullText}`). Svelte escapa el valor al insertarlo como
nodo de texto, evitando que el contenido se interprete como HTML sin convertir
la representación visible en entidades escritas. No se debe usar `{@html}` con
texto capturado ni pre-escapar con `escapeForPreview` antes de una interpolación
de texto.

La rama de código resaltado mantiene el resultado de `renderHighlightedCode`,
que produce markup seguro para la inserción HTML existente. Los tests deben
proteger que las comillas y otros caracteres sigan visibles como caracteres
originales después de que el webview interprete las entidades del markup del
resaltador.

## Alcance compartido

`ClipboardPreview.svelte` es el único overlay que usan el Desktop y Quick
Paste. Corregir allí la interpolación simple mantiene idéntico el resultado en
ambas superficies. Las filas compactas y las cards siguen usando sus propias
proyecciones acotadas; no se modifica su truncamiento ni la persistencia.

## Seguridad y límites

- El contenido plano capturado siempre se trata como texto, incluidos valores
  como `<script>`, atributos con handlers y URL.
- El contrato de texto completo, saltos LF/CRLF, tabs, sangría y líneas vacías
  se conserva.
- El código resaltado conserva sus reglas actuales de detección y
  renderizado seguro; no se agrega un segundo sanitizador.
- No se aplica decodificación de entidades HTML al contenido recibido: eso
  alteraría capturas que contienen secuencias como `&quot;` literalmente.
- No se modifica ninguna fila de historial ni se agrega logging del contenido.

## Verificación

La cobertura debe combinar pruebas de la función pura de fuente canónica con
una regresión del componente que compruebe que la rama simple interpola
`fullText` directamente y no pre-escapa ni inserta HTML. Se verifica que la
ruta resaltada siga usando el renderer seguro. Los checks y tests frontend
cubren ambas superficies porque comparten el componente.
