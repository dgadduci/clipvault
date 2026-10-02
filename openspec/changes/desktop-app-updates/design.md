# Diseño: actualizaciones firmadas de la aplicación desktop

## Estado actual

- `Cargo.toml`, `app/tauri/src-tauri/tauri.conf.json` y
  `app/tauri/frontend/package.json` declaran la versión `0.0.15`.
- `tauri.conf.json` mantiene `bundle.active` en `false` y no configura
  `createUpdaterArtifacts`, endpoint ni clave pública.
- `main.rs` construye el shell Tauri sin los plugins updater ni process.
- La capacidad Tauri predeterminada no concede permisos de updater o relaunch.
- `AboutModal.svelte` ya muestra la versión canónica recibida desde
  diagnostics y es la superficie existente adecuada para el estado de
  actualización.
- El frontend tiene catálogos para `en`, `es`, `pt`, `de` y `fr`.
- No hay workflow de GitHub Actions para empaquetado/release ni directorio
  `.github` en el repositorio.

## Arquitectura elegida

Usar GitHub Releases como alojamiento público de instaladores y manifiesto, el
plugin oficial Tauri Updater para comprobar, verificar e instalar paquetes, y
`tauri-apps/tauri-action` dentro de GitHub Actions para crear releases. No se
añade servidor propio ni código de actualización al core Rust de ClipVault.

El frontend invoca los bindings oficiales del updater. Un servicio de frontend
único coordina la comprobación automática de producción y la comprobación
manual desde “Acerca de”, evitando consultas concurrentes y exponiendo estados
tipados a la interfaz. El plugin process se usa para relanzar la aplicación
cuando la instalación termina. El frontend no descarga ni verifica paquetes
por cuenta propia.

## Flujo de comprobación e instalación

1. En una build de producción, iniciar una comprobación de versión sin esperar
   su resultado para mostrar la interfaz ni para habilitar el uso de ClipVault.
2. Si no hay red, GitHub responde con error o no existe una versión posterior,
   conservar la aplicación actual y permitir el uso normal. El usuario también
   puede volver a intentar desde “Acerca de”.
3. Si hay una versión nueva, mostrarla en “Acerca de” con una acción explícita
   para descargar e instalar. No iniciar descargas ni reinicios sin esa acción.
4. Descargar el artefacto compatible con OS, arquitectura y tipo de
   instalación. El plugin valida su firma Tauri antes de instalarlo.
5. En Linux `.deb`, solicitar la autorización elevada requerida por el
   instalador del sistema. Si el usuario cancela o la instalación falla,
   mantener la versión actual y presentar un error recuperable.
6. Tras una instalación correcta, ofrecer/requerir el relanzamiento para abrir
   la nueva versión.

La consulta de actualización contacta a GitHub desde una build instalada y
puede exponer la petición de red y la versión/plataforma de la aplicación al
servicio que entrega el manifiesto. No se envía el historial, contenido del
portapapeles, nombres de entradas ni identificadores locales. Los datos
persistidos no forman parte del paquete de actualización ni se borran durante
el proceso.

## Firma y configuración Tauri

- Generar una clave de firma Tauri una sola vez antes de habilitar el canal
  estable. El workflow recibe la clave pública desde la variable de GitHub
  `TAURI_UPDATER_PUBLIC_KEY`, genera un overlay temporal para que Tauri CLI
  firme los artefactos con esa misma clave y la compila en el plugin updater;
  la clave privada y su contraseña sólo se exponen al job de release mediante
  secretos. Así el repositorio no lleva una clave pública desconectada de la
  clave privada operativa.
- Respaldar la clave privada en un lugar seguro y restringido. Rotarla o
  perderla requiere un plan explícito para que las instalaciones existentes
  puedan confiar en claves futuras.
- Configurar el endpoint HTTPS de `latest.json` de GitHub Releases y
  `createUpdaterArtifacts`. Los builds locales y de desarrollo omiten el
  plugin updater si no tienen la variable pública; el workflow de release
  falla antes de compilar si faltan la variable o los secretos de firma.
- Exigir `requireSignedVersion` para vincular la versión del manifiesto con la
  versión protegida por la firma del artefacto. La configuración Tauri conserva
  un `pubkey` vacío de desarrollo; el `Builder` del plugin lo reemplaza con la
  variable pública antes de registrar el updater en los builds de release.
- Registrar los plugins updater y process en el bootstrap Tauri y conceder en
  la capability sólo los permisos necesarios para comprobar/instalar y
  relanzar.
- No guardar claves, contraseñas ni certificados en el repositorio, artefactos
  frontend, fixtures o logs.

## Releases y compatibilidad

- Crear el release a partir de tags `vMAJOR.MINOR.PATCH`; validar que el tag y
  las versiones de Cargo, Tauri y frontend coinciden.
- Usar el proyecto Tauri en `app/tauri` y ejecutar la instalación/build
  frontend desde `app/tauri/frontend`, respetando `docs/development.md`.
- Producir builds Linux x86_64 en una base compatible con el runtime objetivo y
  publicar `.AppImage` y `.deb`. El manifiesto debe distinguir el tipo de
  instalador para que cada cliente reciba el formato que tiene instalado.
- Producir builds macOS Apple Silicon e Intel con identidad de firma ad hoc
  (`-`). No requieren certificados Developer ID ni credenciales de
  notarización. macOS puede exigir que el usuario autorice manualmente la app
  en Privacidad y seguridad; documentar ese flujo y probarlo en los equipos
  objetivo.
- La firma ad hoc no identifica al publisher frente a Gatekeeper ni equivale a
  Developer ID/notarización. La firma Ed25519 de Tauri sigue protegiendo la
  integridad de cada paquete updater, de forma independiente a Gatekeeper.
  Migrar a Developer ID y notarización más adelante requerirá configurar
  credenciales Apple en CI.
- Crear cada release inicialmente como borrador, revisar artefactos y
  `latest.json`, y publicarlo para que los clientes lo vean. El workflow exige
  que la etiqueta apunte a un commit alcanzable desde `main` y limita la
  escritura de `GITHUB_TOKEN` al job de build.
- Limitar `GITHUB_TOKEN` a permisos de escritura de contenido necesarios para
  crear la release. Ejecutar el job con secretos sólo para tags/revisiones de
  confianza.

## Interfaz

“Acerca de” conserva la versión canónica existente y agrega estados para:

- comprobando;
- actualización disponible con versión objetivo;
- descarga e instalación con progreso cuando el updater lo ofrece;
- aplicación actualizada y necesidad de reiniciar;
- sin actualizaciones;
- error de red, firma o instalación, con posibilidad de reintentar.

La comprobación no debe bloquear el arranque, abrir ventanas de terminal ni
mostrar fallos transitorios como errores fatales. Todo el texto de producto y
accesible se traduce mediante claves presentes en los cinco catálogos.

## Verificación

- Validar la consistencia de versiones/tag, targets del manifiesto, URLs y
  firmas de los artefactos de release.
- Probar la interfaz ante los estados de actualización, incluida ausencia de
  red y fallos recuperables, sin usar datos reales del portapapeles.
- Probar instalación y relanzamiento en macOS Intel y Apple Silicon; en Linux,
  en AppImage y `.deb`, incluyendo aceptación y cancelación de elevación.
- Confirmar que un artefacto con firma inválida nunca se instala y que el
  cliente conserva datos locales durante una actualización.
- Revisar que `npm run build` valida la paridad de traducciones y que los
  permisos Tauri sólo habilitan las operaciones requeridas.
- Validar el cambio con OpenSpec y revisar `git diff --check`.
