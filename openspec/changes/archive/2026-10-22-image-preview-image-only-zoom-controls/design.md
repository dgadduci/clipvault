# Design: image-preview-image-only-zoom-controls

## Approach

La imagen mantiene su tamaño base calculado por el ajuste proporcional que ya
se realiza al abrir o redimensionar la ventana. Se aplica una transformación
CSS centrada solo al elemento `<img>`; la ventana, su barra y los controles de
rotación quedan fuera de esa transformación.

El zoom empieza en `1×` respecto del ajuste actual. Los botones aumentan o
reducen la escala en un factor de `1.25`, dentro del rango `0.25×` a `4×`.
Los gestos son continuos y se limitan al mismo rango. Los botones se desactivan
al alcanzar cada extremo. Al cambiar a otra captura se vuelve a `1×`; el zoom
no se persiste.

## Touchpad gestures

En los webviews que expresan el gesto como `wheel` con `ctrlKey`, se evita el
zoom predeterminado del navegador y se convierte el delta vertical en un factor
de escala de la imagen. En WebKit se interceptan `gesturestart`,
`gesturechange` y `gestureend`; su escala relativa se aplica sobre el nivel
existente al iniciar el gesto. Los listeners se registran solo mientras el
componente está montado y se eliminan al destruirlo.

Los listeners se instalan en fase de captura para cancelar el gesto antes de
que lo procese el webview. La ventana nativa se crea con el zoom por hotkeys
desactivado y el webview de preview se normaliza a `1×` al iniciar. Desktop y
QuickPaste también se normalizan a `1×` y cancelan los gestos de zoom general.
Esto evita que los gestos alteren el tamaño aparente de los controles o activen
otros breakpoints de layout. La disponibilidad de los botones depende de un
valor reactivo derivado del estado de carga de la imagen, en vez de una función
invocada directamente desde el markup.

Los listeners se instalan en fase de captura para cancelar el gesto antes de
que lo procese el webview. La ventana nativa se crea con el zoom por hotkeys
desactivado y el webview de preview se normaliza a `1×` al iniciar. Esto evita
que los gestos cambien el zoom general, alteren el tamaño aparente de los
controles o activen otros breakpoints de layout. La disponibilidad de los
botones depende de un valor reactivo derivado del estado de carga de la imagen,
en vez de una función invocada directamente desde el markup.

Los manejadores solo actúan en el modo de imagen independiente. Los eventos de
rueda ordinarios y todos los previews de texto conservan su comportamiento.

## Controls and accessibility

Los botones de zoom se ubican junto a los dos botones de rotación en la misma
barra. Usan iconos SVG locales de lupa con `+` y `−`, etiquetas accesibles y
tooltips localizados. No muestran porcentajes ni agregan una barra secundaria.

## Integridad visual del escritorio

La lista de capturas conserva su rail horizontal con desplazamiento horizontal.
El zoom de la ventana de preview no debe modificar la escala de texto de las
colecciones, sus menús, equipos vinculados ni filtros de la barra superior.
Los números de los badges de colección usan texto blanco explícito para evitar
que una variable tipográfica inválida como color los deje con el color por
defecto del navegador.

## Validation

- Probar el helper de escala en los límites y con factores de entrada inválidos.
- Validar por regresión que los botones viven en la barra de acciones, que solo
  se muestran en el preview de imagen independiente y que los gestos modifican
  la escala de la imagen sin escalar la barra.
- Ejecutar build y pruebas frontend focalizadas, regresiones de drag and drop,
  validación OpenSpec estricta y `git diff --check`.
- La confirmación visual del touchpad y de ambos botones requiere prueba manual
  en una ventana Tauri.
