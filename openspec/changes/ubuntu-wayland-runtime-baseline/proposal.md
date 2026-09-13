# Propuesta: baseline y diagnóstico del runtime Ubuntu Wayland

## Problema

ClipVault puede iniciar en Ubuntu GNOME Wayland, instalar el icono de bandeja y
ejecutar el capture loop, pero la ventana principal no aparece. La regresión
manual más reciente vuelve a observar este estado mientras el shell informa
`no primary monitor reported; keeping conf defaults`. Ese resultado no prueba
por sí solo que la ausencia de monitor sea la causa: la consulta sólo controla
el layout opcional y Tauri crea `main` visible por defecto. Los intentos
anteriores de forzar visibilidad, foco, tamaño o posición no solucionaron el
problema y añadieron operaciones antes del mapeo de Mutter.

El binario GTK/WebKit y el compositor reales sólo pueden verificarse en Ubuntu.
La compilación cruzada desde macOS no sustituye esa prueba.

## Objetivo

Restablecer un flujo ejecutable directamente en Ubuntu que reproduzca un
arranque limpio, registre el ciclo de vida de la ventana con metadatos seguros
y aplique únicamente una corrección demostrada por la traza. El cambio debe
distinguir explícitamente entre la falta de monitor para el layout opcional y
el fallo real de crear, mapear o conservar visible el toplevel `main`.

## Alcance

- Creación, presentación y restauración de la ventana principal Tauri/Wry.
- Diagnóstico local de runtime con datos de estado de ventana exclusivamente.
- Aislamiento acotado del handshake inicial del adaptador Wayland para que un
  compositor que no responde no bloquee el `setup` del shell.
- Pruebas Ubuntu GNOME Wayland y smoke test Ubuntu X11.

## Fuera de alcance

- Cambios funcionales de integración GNOME, semántica de detección de
  aplicación activa, clipboard, SQLite, imágenes, metadata de aplicaciones y
  EntryRecord. El ajuste de timeout sólo evita que el adaptador ya existente
  bloquee el arranque del shell.
- Quick Paste, tags, colecciones, favoritos, búsqueda y drag and drop.
- Cambios de compositor, extensiones o paquetes de sistema.

## Resultado esperado

En Ubuntu GNOME Wayland, `cargo tauri dev` abre el desktop visible y la acción
del tray restaura una ventana ocultada. Si el runtime no crea el toplevel, una
traza metadata-only identifica el primer paso fallido sin proponer fallbacks
basados en macOS.
