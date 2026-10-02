# Diseño: alinear Rust con los builds de release desktop

## Decisión

Fijar Rust `1.90.0` como toolchain del repositorio y del workflow de releases.
Los errores de los tres targets muestran que las dependencias ya bloqueadas
`tauri-plugin 2.7.1` y `tauri-utils 2.10.1` requieren al menos esa versión.
Elevar también `workspace.package.rust-version` a `1.90` para que los
metadatos no prometan compilar con una versión que no satisface el grafo
actual.

## Cambios acotados

- `rust-toolchain.toml` selecciona la versión exacta `1.90.0`.
- `Cargo.toml` declara el mínimo `1.90`.
- El job `desktop-release` instala `1.90.0` para cada target de macOS y Linux.
- `docs/development.md` y OpenSpec reflejan la misma base.
- `Cargo.lock` no cambia; no se actualizan ni reemplazan dependencias.

## Verificación y release de prueba

La verificación decisiva es compilar los tres targets mediante el workflow
`v0.0.16`. Tras integrar el cambio en `main`, se actualiza el tag de prueba
fallido para que apunte al nuevo commit alcanzable desde `main` y se vuelve a
ejecutar el workflow. El release permanece como borrador; no se publica.
Cuando termine, se revisan que estén los instaladores, sus firmas y
`latest.json` antes de guiar la instalación manual en Linux y después en
macOS.
