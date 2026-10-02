# Tareas: alinear Rust con los builds de release desktop

- [x] 1.1 Confirmar en los logs del workflow que los tres targets usan Rust
  `1.89.0` y fallan porque `tauri-plugin 2.7.1` y `tauri-utils 2.10.1`
  requieren Rust `1.90`.
- [x] 1.2 Confirmar que la contraseña vacía de la clave Tauri se entrega al
  build y que la validación de la configuración de firma pasa.
- [x] 2.1 Fijar `rust-toolchain.toml`, el workspace y el workflow de release
  en la base compatible Rust `1.90`.
- [x] 2.2 Actualizar la documentación de desarrollo y el delta de
  `repository-reproducibility`.
- [x] 3.1 Validar OpenSpec, formato del diff y consistencia de las versiones
  declaradas.
- [ ] 3.2 Integrar el arreglo en `main` y volver a ejecutar el workflow
  borrador `v0.0.16` para macOS Apple Silicon, macOS Intel y Linux.
- [ ] 3.3 Revisar instaladores, firmas y `latest.json` sin publicar el
  borrador.
