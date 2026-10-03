# Propuesta: corregir el renderizado AppImage en Linux Wayland

## Problema

En Arch Linux con KDE Plasma Wayland, la AppImage de ClipVault inicia el shell
Tauri y registra el cálculo de ancho de la ventana, pero el desktop queda en
blanco. WebKit termina con `Could not create default EGL display:
EGL_BAD_PARAMETER`. Los avisos de módulos GTK opcionales ausentes aparecen
antes y no explican por sí solos el fallo del renderizador.

La AppImage de release se construye con Tauri CLI `2.11.5`. El upstream de
Tauri documentó que la ruta por defecto del bundler incluía bibliotecas de
Wayland/GLib incompatibles con Mesa reciente y causaba este mismo aborto del
proceso WebKit en Wayland.

## Qué cambia

- Actualizar la versión fijada de Tauri CLI usada para crear los artefactos
  desktop al primer release que incluye la corrección del bundler AppImage.
- Preparar `v0.0.18` con las versiones canónicas sincronizadas y la release
  automatizada en borrador para probar el nuevo paquete.
- Mantener Rust `1.90.0`, el formato de firma, los targets de release y la
  implementación de la ventana.
- Inspeccionar los artefactos resultantes y verificar el renderizado de la
  AppImage en Arch KDE Wayland.

## Fuera de alcance

- Cambiar el layout, el cálculo de ancho o el frontend de ClipVault.
- Añadir flags gráficos permanentes o dependencias de hardware/driver.
- Cambiar el comportamiento de `.deb` o macOS, salvo verificar que continúan
  empaquetándose correctamente.
- Publicar la release para todos los usuarios antes de completar la prueba
  manual en Arch KDE Wayland.

## Resultado esperado

La AppImage muestra el desktop en Arch KDE Wayland sin el aborto de EGL; el
historial y demás datos existentes en `~/.clipvault` permanecen intactos.
