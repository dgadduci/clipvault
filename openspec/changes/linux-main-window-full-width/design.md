# Diseño: ventana principal Linux a ancho completo

## Estado actual

- `tauri.conf.json` configura la ventana `main` con ancho inicial de 1080 px y
  `minWidth` de 720 px.
- Durante `setup`, `main.rs::resize_main_window_to_monitor` consulta el
  monitor primario y entrega su `work_area` junto con el factor de escala a
  `compute_main_window_layout`.
- `main_window_layout.rs` convierte ese ancho físico a coordenadas lógicas,
  aplica el mínimo de 720 y calcula el tamaño y posición iniciales.
- `App.svelte` renderiza `.layout` con `width: 100%`; el grid tiene una
  columna flexible con `minmax(0, 1fr)`.
- El tamaño inicial se aplica una vez. La implementación actual permite que
  el usuario redimensione la ventana después del arranque.

El resultado esperado ya está expresado en el código, pero el reporte de
Linux muestra que la geometría visible no cumple el contrato. Por eso la
implementación debe comparar los límites del monitor y área disponible, la
escala, el tamaño solicitado por Tauri y el tamaño final aplicado por el
backend/compositor. La causa no se atribuye a un backend específico hasta
observar esos valores en una sesión afectada; la ruta Linux registra sólo
geometría numérica para hacer esa comparación al iniciar.

## Aplicación de geometría en Linux

- En `setup`, consultar primero el monitor actual y usar el primario como
  fallback. Preferir `work_area` cuando sus coordenadas y dimensiones sean
  válidas; si no, usar los límites del monitor. Sin geometría utilizable,
  conservar el tamaño configurado.
- Convertir el ancho disponible a coordenadas lógicas con el factor de escala
  reportado y reducir el mínimo nativo cuando una pantalla tenga menos de 720
  píxeles lógicos de ancho.
- Registrar antes del tamaño inicial un listener de `Resized`. En su primer
  evento efectivo, comparar el ancho físico aplicado con el objetivo. Si el
  compositor aplicó otro ancho, repetir una vez el tamaño y la posición
  calculados. Desarmar el listener antes de corregir para que no interfiera
  con los cambios manuales posteriores.
- Emitir únicamente las medidas geométricas necesarias para contrastar el
  objetivo y el tamaño efectivo; no incluir contenido del portapapeles.
- Mantener el camino existente de macOS y no modificar QuickVault ni el CSS
  del workspace, cuyo ancho ya sigue al WebView.

## Contrato de geometría

- Aplicar el requisito sólo a la ventana `main` de Linux.
- Usar el monitor donde se abre la ventana según el flujo actual. Preferir el
  área de trabajo válida del monitor; si el backend no la proporciona o
  devuelve dimensiones inválidas, usar los límites del monitor disponibles.
- Convertir ancho lógico y físico usando el factor de escala del monitor; no
  comparar valores de unidades distintas.
- El ancho efectivo de la superficie del desktop debe cubrir el ancho
  horizontal disponible, sin columnas laterales vacías ni un límite fijo
  heredado de la configuración. El mínimo nativo de ancho no puede exceder el
  área disponible; el contenido interno debe poder adaptarse a esa anchura.
- Si el sistema no proporciona información utilizable del monitor, conservar
  el fallback actual de configuración y permitir que ClipVault inicie.
- Aplicar el tamaño inicial cuando la ventana nativa ya pueda aceptar la
  geometría que comunica el compositor. Evitar que una aplicación tardía del
  tamaño deje visible el ancho de configuración inicial.

El área utilizable es el rectángulo horizontal que el sistema reporta para el
monitor elegido. Sombras y bordes de decoración nativos no cuentan como
márgenes del contenido de ClipVault.

## Restricciones y compatibilidad

- Mantener la altura mínima/máxima y la alineación superior actuales; llenar
  el ancho no significa maximizar la altura ni entrar en fullscreen.
- Mantener la operación inicial acotada al arranque/apertura normal. Después
  de ese ajuste, respetar cualquier movimiento o redimensionamiento manual del
  usuario durante la sesión.
- No cambiar QuickVault, cuyos límites fijos son independientes.
- No cambiar el comportamiento de macOS, que sirve como referencia visual.
- Mantener el layout frontend al 100% del espacio interno de la ventana y
  evitar overflow horizontal en la página. No tocar drag-and-drop, payloads,
  cards o assets.
- No agregar una dependencia arquitectónica. Cualquier dato de diagnóstico
  añadido debe limitarse a geometría numérica local; nunca incluir contenido
  del portapapeles.

## Verificación

- Tests unitarios del cálculo para work areas completas y con origen distinto
  de cero, factores de escala, valores inválidos y áreas menores que el
  `minWidth` actual.
- Regresión del flujo de setup que compruebe el tamaño final efectivo de la
  ventana principal, no sólo el valor calculado antes de aplicarlo.
- Confirmar que `.layout` ocupa el ancho disponible del WebView sin causar
  overflow horizontal.
- Verificar manualmente Ubuntu/GNOME Wayland, KDE Plasma Wayland y Linux/X11;
  incluir GNOME/KDE sobre X11 cuando estén disponibles.
- Repetir la verificación con escala 1x y una escala HiDPI cuando el entorno
  lo permita. Comprobar también que cambiar el tamaño manualmente no provoca
  que la aplicación vuelva a expandir la ventana.
- Revisar macOS y QuickVault como no-regresión.
