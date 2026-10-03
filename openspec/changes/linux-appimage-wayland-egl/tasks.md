# Tareas: corregir el renderizado AppImage en Linux Wayland

## 1. Diagnóstico

- [x] 1.1 Relacionar el bootstrap correcto y el ajuste de ancho con el aborto
  posterior de WebKit en `EGL_BAD_PARAMETER`.
- [x] 1.2 Confirmar que el workflow fija Tauri CLI `2.11.5` y Rust `1.90.0`.
- [x] 1.3 Identificar el reporte upstream coincidente y la corrección incluida
  en Tauri CLI `2.12.0`.

## 2. Empaquetado

- [x] 2.1 Actualizar el pin de Tauri CLI a `2.12.0` y sincronizar las
  versiones canónicas a `0.0.18`.
- [x] 2.2 Ejecutar el workflow de release para crear el borrador `v0.0.18`;
  comprobar AppImage, `.deb`, bundles macOS, firmas y `latest.json`.
- [x] 2.3 Inspeccionar la AppImage y confirmar que no incluye
  `libwayland-client.so`, de modo que WebKit resuelva la biblioteca Wayland
  del host compatible con Mesa/EGL.

## 3. Verificación

- [x] 3.1 Probar manualmente la nueva AppImage en Arch KDE Wayland y registrar
  que la interfaz se dibuja sin el aborto EGL.
- [ ] 3.2 Confirmar que reemplazar la AppImage conserva historial y assets
  locales en `~/.clipvault`.
- [x] 3.3 Confirmar que el `.deb` de Ubuntu y los bundles macOS siguen
  empaquetándose correctamente en CI.
- [x] 3.4 Ejecutar la validación OpenSpec estricta y revisar `git diff --check`.
