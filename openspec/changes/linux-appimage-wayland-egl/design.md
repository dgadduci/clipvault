# Diseño: corregir el renderizado AppImage en Linux Wayland

## Diagnóstico

La secuencia del log separa el arranque del shell del fallo visual:

1. ClipVault completa el bootstrap de Rust, instala el tray y calcula/corrige
   el ancho de la ventana.
2. El contenido WebView no llega a dibujarse.
3. WebKit aborta al crear el display EGL con `EGL_BAD_PARAMETER`.

Esto coincide con el problema de AppImage registrado en Tauri. El reporte
upstream señala que builds con Tauri `2.11.2` a `2.11.5` podían incluir una
`libwayland-client` que sombrea la del host y rompe la combinación de Mesa/EGL
en Wayland. La actualización del bundler y del plugin GTK se integró en Tauri
CLI `2.12.0`; su nota de cambio menciona expresamente la corrección de
`EGL_BAD_PARAMETER` y la compatibilidad de AppImage con Wayland nativo.

Referencias primarias:

- [Tauri issue #15665: AppImages y EGL_BAD_PARAMETER](https://github.com/tauri-apps/tauri/issues/15665)
- [Tauri PR #16062: actualizar linuxdeploy y el plugin GTK](https://github.com/tauri-apps/tauri/pull/16062)
- [Release oficial Tauri CLI 2.12.0](https://github.com/tauri-apps/tauri/releases/tag/tauri-cli-v2.12.0)

Los mensajes `Failed to load module "colorreload-gtk-module"` y
`Failed to load module "window-decorations-gtk-module"` indican que GTK no
encontró esos módulos opcionales del entorno; el fallo fatal reportado es la
creación del display EGL por WebKit.

## Decisión

Fijar el workflow desktop en Tauri CLI `2.12.0`, que incluye la actualización
del bundler AppImage identificada en el reporte upstream. El workflow ya fija
Rust `1.90.0`, suficiente para esta versión. Sincronizar los manifiestos de la
aplicación en `0.0.18` para generar la AppImage corregida. La CLI `2.12.0`
incluye la versión de la aplicación en el comentario confiable de la firma;
el plugin updater fijado en `2.12.0` y `requireSignedVersion` ya soportan esta
validación. No modificar el cálculo de ancho, el WebView, `GDK_BACKEND`,
variables EGL ni la configuración de drivers: la corrección debe estar en el
empaquetado generado.

La compilación debe conservar la firma Ed25519 de updater y producir el
manifiesto habitual. El workflow existente adjunta los artefactos a una
release de GitHub en borrador; mantener `v0.0.18` sin publicar hasta completar
la prueba de Arch. Debido a que la ventana blanca impide usar la UI de
actualización de la instalación afectada, la primera validación del arreglo
debe sustituir manualmente la AppImage. La base de datos y los assets locales
no forman parte del bundle y no deben tocarse.

## Verificación

- Confirmar que el workflow utiliza Tauri CLI `2.12.0` y mantiene Rust
  `1.90.0`.
- Construir los bundles configurados y comprobar la firma y `latest.json`.
- Inspeccionar la AppImage para confirmar que el bundler actualizado no
  vuelve a sombrear las bibliotecas del host que requiere Wayland/Mesa.
- Probar el artefacto en Arch KDE Wayland: el desktop debe renderizarse, el
  proceso WebKit no debe abortar y el historial local debe seguir disponible.
- Confirmar que `.deb` en Ubuntu y los bundles macOS siguen empaquetándose.
