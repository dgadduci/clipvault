# Propuesta: baseline y diagnóstico del runtime Ubuntu Wayland

## Problema

ClipVault puede iniciar en Ubuntu GNOME Wayland, instalar el icono de bandeja y
ejecutar el capture loop, pero la ventana principal no aparece. Los intentos de
forzar visibilidad, foco, tamaño o posición desde macOS no solucionaron el
problema y añadieron operaciones antes del mapeo de Mutter.

El binario GTK/WebKit y el compositor reales sólo pueden verificarse en Ubuntu.
La compilación cruzada desde macOS no sustituye esa prueba.

## Objetivo

Establecer un flujo ejecutable directamente en Ubuntu que parta de `main`,
reproduzca un arranque limpio, registre el ciclo de vida de la ventana con
metadatos seguros y aplique únicamente una corrección demostrada por la traza.

## Alcance

- Creación, presentación y restauración de la ventana principal Tauri/Wry.
- Diagnóstico local de runtime con datos de estado de ventana exclusivamente.
- Pruebas Ubuntu GNOME Wayland y smoke test Ubuntu X11.

## Fuera de alcance

- Integración GNOME, detección de aplicación activa, clipboard, SQLite,
  imágenes, metadata de aplicaciones y EntryRecord.
- Quick Paste, tags, colecciones, favoritos, búsqueda y drag and drop.
- Cambios de compositor, extensiones o paquetes de sistema.

## Resultado esperado

En Ubuntu GNOME Wayland, `cargo tauri dev` abre el desktop visible y la acción
del tray restaura una ventana ocultada. Si el runtime no crea el toplevel, una
traza metadata-only identifica el primer paso fallido sin proponer fallbacks
basados en macOS.
