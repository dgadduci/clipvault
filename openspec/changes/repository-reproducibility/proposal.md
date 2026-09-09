# Propuesta: reproducibilidad del repositorio

## Problema

ClipVault se desarrolla en macOS y se valida en Ubuntu desde el mismo
repositorio remoto, pero la base actual permite que cada máquina use una
versión diferente de Rust, Node/npm, CLI de Tauri o dependencias del sistema.
Esto ya produjo una diferencia concreta: Ubuntu requirió fijar localmente
Rust 1.89.0 para compilar y ejecutar la rama Linux, mientras que el archivo
`rust-toolchain.toml` todavía declara `stable` y el workspace declara
`rust-version = "1.85"`, aunque el lockfile actual contiene dependencias que
requieren Rust más nuevo.

También aparecieron lockfiles `pnpm` accidentales fuera del flujo oficial de
frontend, y no existe una matriz CI que valide de forma continua macOS y
Ubuntu. Sin una convención única, una prueba local puede ser exitosa sólo por
el estado particular de una máquina.

## Objetivos

- Declarar una toolchain Rust común, exacta y verificable en ambos sistemas.
- Declarar una base Node/npm común para el frontend y conservar
  `app/tauri/frontend/package-lock.json` como lockfile oficial.
- Documentar directorios, comandos y orden de validación sin repetir variantes
  incompatibles de `npm`, `cargo` o `cargo tauri`.
- Incorporar CI multiplataforma para detectar regresiones de compilación,
  tests, frontend y features Linux antes de integrar a `main`.
- Formalizar el flujo Git/OpenSpec: rama de trabajo, commit, push, validación
  remota, prueba en Ubuntu y merge controlado.
- Proteger explícitamente la base SQLite y los assets locales de usuario; las
  compilaciones, tests, OpenSpec y Git no deben borrar, mover ni regenerar
  `~/.clipvault`.

## Alcance

- `rust-toolchain.toml` y `Cargo.toml` del workspace.
- Metadatos de toolchain del frontend y su lockfile oficial.
- Documentación de desarrollo multiplataforma.
- Workflow(s) de GitHub Actions para checks no interactivos.
- `.gitignore` para artefactos locales de gestores no usados y archivos
  generados.
- Tests o scripts pequeños de verificación que no accedan al portapapeles
  real ni a datos personales.

## Fuera de alcance

- No cambiar el comportamiento de captura, paste, búsqueda, tags,
  colecciones, quick-paste, imágenes o permisos.
- No cambiar adapters macOS, X11 o Wayland ni agregar soporte funcional nuevo.
- No migrar de npm a pnpm, yarn, bun ni introducir un monorepo JavaScript.
- No borrar lockfiles rastreados ni datos del usuario.
- No ejecutar `npm audit fix --force` como parte de esta tarea.
- No hacer releases, publicar paquetes ni cambiar configuraciones remotas de
  protección de ramas en GitHub.
- No marcar por inferencia las pruebas manuales de
  `platform-permission-guidance` ni las pruebas GUI de plataforma.

## Resultado esperado

Una persona puede clonar el repositorio en macOS o Ubuntu, seleccionar las
versiones declaradas, instalar dependencias con `npm ci`, ejecutar los mismos
checks desde las mismas raíces y saber qué parte debe confirmarse en el host
real. El mismo commit de una rama produce el mismo grafo de dependencias y CI
verifica las rutas de plataforma sin tocar el historial local del usuario.
