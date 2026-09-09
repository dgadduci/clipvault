# Diseño: repository-reproducibility

## Decisiones de toolchain

### Rust

La base validada en Ubuntu es Rust `1.89.0`. El repositorio debe fijar esa
versión exacta en `rust-toolchain.toml`, junto con `rustfmt` y `clippy`, en
perfil mínimo. El workspace debe declarar `rust-version = "1.89"` para que el
MSRV publicado no prometa una versión que ya no puede resolver el lockfile
actual. El cambio debe actualizar `Cargo.lock` sólo si el propio cambio de
toolchain lo requiere; no se deben actualizar dependencias por oportunidad.

La versión exacta es una decisión deliberada: `stable` permitió que macOS y
Ubuntu compilaran con compiladores diferentes, y el `rust-version` actual de
1.85 es inconsistente con los requisitos MSRV observados. No se deben fijar
targets de una sola máquina dentro de la toolchain; macOS ARM y Ubuntu x86_64
deben añadir sus targets sólo cuando sean necesarios para la build.

### Node y npm

El gestor oficial es npm y el lockfile oficial es
`app/tauri/frontend/package-lock.json`. Se debe declarar Node.js 20 LTS y npm
10 como base compatible. La implementación debe inspeccionar las versiones
disponibles en los entornos antes de fijar el formato final (`.nvmrc` y/o
`package.json`), y dejar documentadas las versiones exactas usadas por CI. Si
la versión patch instalada en macOS y Ubuntu difiere, no se debe resolver con
un `npm install` que regenere el lockfile: se debe usar la versión declarada.

El archivo `package-lock.json` existente se conserva. `npm ci` se ejecuta
exclusivamente desde `app/tauri/frontend`; no se deben crear lockfiles en la
raíz, `app/` ni `app/tauri/`.

### Tauri CLI

La documentación debe declarar cómo comprobar la versión de `cargo-tauri` y
qué línea mayor compatible con Tauri 2 debe utilizarse. La ejecución canónica
de desarrollo es `cd app/tauri && cargo tauri dev`, porque el
`beforeDevCommand` ya está configurado relativo a esa raíz. No se deben
añadir `--manifest-path` a `cargo tauri dev` ni cambiar el
`beforeDevCommand` para compensar un directorio de trabajo incorrecto.

## Comandos canónicos

Desde la raíz del repositorio:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Desde `app/tauri/frontend`:

```text
npm ci
npm run check
npm run build
npm test
```

Para desarrollo Tauri:

```text
cd app/tauri
cargo tauri dev
```

La documentación debe indicar el orden recomendado: instalar toolchains,
instalar npm con `npm ci`, ejecutar checks, iniciar una sola instancia de la
app y cerrar la instancia anterior antes de recompilar o probar una nueva.

## Flujo Git/OpenSpec multiplataforma

El flujo documentado será:

```text
main actualizado
  → rama feat/fix/chore
  → OpenSpec
  → implementación en macOS
  → commit y push
  → pull de la misma rama en Ubuntu
  → checks + prueba manual Linux
  → correcciones adicionales en la misma rama
  → PR/merge a main
  → sincronización y archive OpenSpec cuando corresponda
```

La rama `main` es la referencia de integración y no se deben mezclar en una
misma rama dos cambios funcionales no relacionados. Cada reporte debe incluir
commit, rama, toolchain, plataforma/sesión Linux, comandos ejecutados y
limitaciones manuales. La prueba manual 10/10 de Ubuntu se registra en
`linux-x11-compatibility/tasks.md`, pero el código y ese cierre documental no
se deben ocultar dentro del nuevo cambio.

## CI

Se debe agregar un workflow de checks no interactivos con:

- macOS ARM compatible con el desarrollo local;
- Ubuntu compatible con la build Linux del proyecto;
- setup-node usando la versión declarada y `npm ci` desde el frontend;
- Rust fijado explícitamente al mismo `1.89.0` que selecciona
  `rust-toolchain.toml` (la acción `dtolnay/rust-toolchain` con canal
  `stable` ignora el `rust-toolchain.toml` del repositorio y adopta el
  estable de la imagen del runner, por lo que se debe usar
  `dtolnay/rust-toolchain@master` con `toolchain: 1.89.0` y `components:
  rustfmt, clippy`, o `dtolnay/rust-toolchain@1.89.0`);
- un paso de verificación que ejecute `rustc --version` y
  `cargo --version` y falle el job si `rustc` no es `1.89.0`, de modo que
  un "CI verde" no se pueda conseguir con una versión estable distinta
  del pin del repositorio;
- cache segura de Cargo/npm;
- dependencias de sistema mínimas para compilar Tauri en Ubuntu;
- fmt, clippy, tests Rust, `cargo check` del shell Linux con
  `clipboard-arboard,hotkey-global`, y checks/build/tests del frontend.

CI no debe ejecutar `cargo tauri dev`, abrir ventanas, usar el portapapeles
real, simular hotkeys, consultar la aplicación activa ni modificar
`~/.clipvault`. Las pruebas GUI de macOS, X11 y Wayland siguen siendo
manuales. Si un job sólo puede compilar una ruta por restricciones del runner,
debe reportarlo de forma explícita en vez de falsear soporte.

## Integridad de datos y lockfiles

- `Cargo.lock` y `app/tauri/frontend/package-lock.json` permanecen
  rastreados.
- `target/`, `node_modules/`, `dist/`, logs y datos locales permanecen
  ignorados.
- `pnpm-lock.yaml` y `pnpm-workspace.yaml` no forman parte del flujo oficial;
  sus variantes locales deben quedar ignoradas para no volver a ensuciar el
  status, sin borrar los archivos de una máquina automáticamente.
- Ningún script de CI, test o OpenSpec debe apuntar a rutas del usuario como
  `~/.clipvault` ni eliminar assets para “limpiar” una build.
- No se deben registrar contenido del clipboard, hashes, rutas absolutas,
  bytes de assets ni credenciales.

## Compatibilidad con cambios existentes

Este cambio es de infraestructura de desarrollo. Debe tratar como contratos
protegidos los adapters Linux recién validados, los adapters macOS, el
fallback Wayland, la captura de imágenes, la carga tras reinicio, el
drag-and-drop, Quick Paste y `platform-permission-guidance`. No debe reabrir,
archivar ni modificar otros changes salvo los archivos de documentación o
toolchain estrictamente necesarios.
