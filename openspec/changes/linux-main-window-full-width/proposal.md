# Propuesta: ventana principal Linux a ancho completo

## Problema

En macOS, el desktop principal ocupa todo el ancho disponible de la pantalla.
En Linux queda una franja horizontal sin usar aunque la ventana está abierta.
El problema debe corregirse para X11 y Wayland, incluidos GNOME y KDE Plasma.

El código actual ya calcula el ancho inicial a partir del `work_area` del
monitor primario y el workspace de Svelte declara `width: 100%`. El síntoma
reportado indica que el ancho que entrega o aplica el backend Linux no coincide
con el ancho visible final. La causa puede estar en las métricas del monitor,
la escala lógica/física, el momento en que el compositor aplica el tamaño o
las restricciones nativas de la ventana; todavía no está confirmada.

## Qué cambia

- La ventana principal debe llenar horizontalmente el área disponible del
  monitor en Linux al mostrarse por primera vez.
- El comportamiento debe ser consistente en X11 y Wayland con GNOME y KDE
  Plasma.
- El ancho se debe calcular y aplicar en las unidades correctas para el
  monitor y su escala, sin quedar limitado por el ancho inicial o mínimo
  predeterminado cuando ese límite impide llenar la pantalla.
- La altura compacta, la posición vertical inicial, la ventana QuickVault y
  el comportamiento existente de macOS se conservan.
- El redimensionamiento manual posterior no debe ser sobrescrito por
  observadores o actualizaciones del frontend.

## Fuera de alcance

- Maximizar verticalmente la ventana, usar fullscreen o cambiar su decoración.
- Cambiar el layout interno, las cards, el rail, las colecciones o sus
  interacciones salvo que el diagnóstico demuestre que el contenedor visual
  impide ocupar el ancho de la ventana.
- Modificar la ventana QuickVault, las integraciones de clipboard o las
  preferencias del usuario.
- Añadir dependencias, telemetría o servicios externos.

## Impacto esperado

- Tauri: geometría inicial de la ventana `main`, selección de área de monitor,
  escala y restricciones nativas.
- Frontend: confirmar que el workspace sigue llenando el ancho del WebView;
  sólo cambiar CSS si la medición muestra un límite interno.
- Verificación: tests puros de conversión de geometría y pruebas manuales en
  X11, Wayland/GNOME y Wayland/KDE Plasma.
