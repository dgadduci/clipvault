# Propuesta: alinear Rust con los builds de release desktop

## Problema

El primer build de release de `v0.0.16` falla en macOS Apple Silicon, macOS
Intel y Linux porque el workflow fija Rust `1.89.0`, mientras que
`tauri-plugin 2.7.1` y `tauri-utils 2.10.1`, presentes en `Cargo.lock`, exigen
Rust `1.90`. El workspace declara además un mínimo `1.85` y el archivo de
toolchain local no coincide con la versión exacta que indican la
documentación y el requisito vigente de reproducibilidad.

La contraseña vacía de `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` no causa el fallo:
la validación de firma pasa y el build llega a resolver las dependencias Rust.

## Qué cambia

- Fijar Rust y Cargo en `1.90.0` para el desarrollo local y los builds de
  release.
- Elevar `workspace.package.rust-version` a `1.90` para reflejar el mínimo
  efectivo de las dependencias bloqueadas.
- Actualizar la documentación y el delta OpenSpec de reproducibilidad.
- Reejecutar el build de release de prueba `v0.0.16` y revisar sus artefactos
  en el borrador.

## Fuera de alcance

- Cambios de comportamiento de la aplicación o de Tauri Updater.
- Actualizaciones o downgrades de dependencias y cambios a `Cargo.lock`.
- Publicar el release borrador.

## Resultado esperado

Los builds de macOS Apple Silicon, macOS Intel y Linux compilan con la misma
toolchain que declara el repositorio y permiten completar el release borrador
para las pruebas manuales.
