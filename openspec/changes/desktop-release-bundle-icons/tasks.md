# Tareas: generar iconos válidos para los bundles desktop

- [x] 1.1 Confirmar que el fallo de macOS es `No matching IconType` al crear
  el bundle, después de compilar correctamente con Rust `1.90.0`.
- [x] 1.2 Confirmar que el único icono versionado es un PNG de 1×1 y que no
  existe un ICNS u otra fuente gráfica.
- [x] 2.1 Crear y revisar el PNG cuadrado basado en el logo provisto por el
  usuario como fuente única.
- [x] 2.2 Configurar el overlay temporal de release para seleccionar el ICNS
  de macOS y PNG generados para Linux.
- [x] 2.3 Hacer que el workflow genere los formatos con `tauri icon` antes
  del build de cada target.
- [ ] 3.1 Validar OpenSpec, el diff y que la configuración referencia los
  nombres de archivo generados.
- [ ] 3.2 Reejecutar el workflow de release borrador `v0.0.16` y comprobar
  que los bundles de macOS Apple Silicon/Intel y Linux se crean.
- [ ] 3.3 Revisar instaladores, firmas y `latest.json` sin publicar el
  borrador.
