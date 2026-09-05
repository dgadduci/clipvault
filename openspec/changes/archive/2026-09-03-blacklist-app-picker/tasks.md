## 1. OpenSpec y contrato de dominio

- [x] 1.1 Crear tipos de dominio/DTO para una aplicación ignorada con id, nombre e icono/referencia opcional.
- [x] 1.2 Definir errores tipados para cancelación, selección inválida, metadata ausente, backend no disponible y sesión no soportada.
- [x] 1.3 Documentar normalización, idempotencia y compatibilidad con registros antiguos que sólo contienen id.

## 2. Plataforma

- [x] 2.1 Crear un trait sustituible de selector/extractor de aplicaciones.
- [x] 2.2 Implementar selector macOS inicializado en /Applications.
- [x] 2.3 Validar bundles .app sin ejecutar aplicaciones ni invocar shell.
- [x] 2.4 Extraer identificador y nombre visible desde metadata nativa.
- [x] 2.5 Extraer o generar una referencia de icono local segura con fallback.
- [x] 2.6 Mantener una ruta explícita de unsupported_session/backend_unavailable para Linux cuando no exista mapeo seguro a WM_CLASS/app id.
- [x] 2.7 Verificar que el selector AppKit no bloquee ni genere deadlock con el event loop de Tauri.

## 3. Persistencia y core

- [x] 3.1 Agregar una migración SQLite aditiva y reversible para metadata opcional de aplicaciones ignoradas.
- [x] 3.2 Extender el repositorio para insertar, listar, actualizar y eliminar sin duplicar por id normalizado.
- [x] 3.3 Mantener PrivacyGate consumiendo sólo ids normalizados.
- [x] 3.4 Crear el servicio core que coordine selección, validación y commit.
- [x] 3.5 Garantizar que un fallo de icono no impida guardar una app válida.

## 4. Shell Tauri

- [x] 4.1 Añadir un comando delgado para seleccionar y agregar una aplicación.
- [x] 4.2 Mapear errores tipados a respuestas seguras para la UI.
- [x] 4.3 Registrar el comando y sus capacidades sin habilitar acceso irrestricto al filesystem.
- [x] 4.4 Mantener compatibilidad con los comandos existentes de listar, agregar y eliminar cuando sea necesario.

## 5. Frontend

- [x] 5.1 Reemplazar el input manual y el botón de añadir por Seleccionar aplicación.
- [x] 5.2 Mostrar icono, nombre y acción de eliminar en cada fila.
- [x] 5.3 Implementar fallback visual para iconos ausentes y registros antiguos.
- [x] 5.4 Mostrar carga, cancelación, error y éxito sin bloquear la pestaña.
- [x] 5.5 No reimplementar parsing, matching ni lectura del filesystem en Svelte.
- [x] 5.6 Mantener el identificador fuera de la presentación principal, salvo tooltip o atributo accesible justificado.

## 6. Tests

- [x] 6.1 Tests unitarios del contrato de metadata e idempotencia.
- [x] 6.2 Tests del adapter con fake para cancelada, válida, archivo inválido, metadata ausente e icono ausente.
- [x] 6.3 Tests de normalización, persistencia, migración de filas antiguas y supervivencia tras reinicio.
- [x] 6.4 Tests de que PrivacyGate continúa usando el id seleccionado.
- [x] 6.5 Tests Tauri/frontend para seleccionar, cancelar, duplicar, eliminar, errores y fallback visual.
- [x] 6.6 Tests de privacidad: no clipboard content, hash, snippet ni logs sensibles en el flujo.

## 7. Verificación

- [x] 7.1 Ejecutar cargo fmt --all -- --check.
- [x] 7.2 Ejecutar cargo clippy --workspace --all-targets -- -D warnings.
- [x] 7.3 Ejecutar cargo test --workspace.
- [x] 7.4 Ejecutar npm run check, npm run build y npm test en frontend.
- [x] 7.5 Ejecutar openspec validate blacklist-app-picker --strict --type change.
- [ ] 7.6 Completar la prueba manual macOS documentada.
- [x] 7.7 No archivar automáticamente el cambio.

## 8. Corrección del bug del selector en hilo principal

- [x] 8.1 Mantener `NSOpenPanel` en el hilo principal de macOS a través de `AppHandle::run_on_main_thread`.
- [x] 8.2 Recibir `AppHandle` en `clipvault_ignored_app_pick_and_add` y delegar en `bootstrap::pick_and_add_ignored_app`.
- [x] 8.3 Detectar el hilo principal con `is_main_thread` y evitar deadlock cuando el comando ya corre en él.
- [x] 8.4 Transportar el resultado con un canal síncrono acotado por `PICK_AND_ADD_WAIT`.
- [x] 8.5 Mapear errores de scheduling y timeout a `IgnoredAppError::BackendUnavailable` con mensaje seguro.
- [x] 8.6 Mantener la lógica de persistencia y matching en `clipvault-core`; Tauri solo coordina el hilo.
- [x] 8.7 Tests del flujo off-main, main-thread inline, schedule failure, timeout, cancelación, aplicación válida, idempotencia, metadata, icono y privacidad.
- [x] 8.8 Re-ejecutar fmt, clippy, test --workspace, npm checks y openspec validate.

## 9. Cableado real del feature `macos-native`

El selector macOS seguía mostrando "El selector nativo no está disponible en
esta sesión" aunque la coordinación con `run_on_main_thread` ya estaba
implementada. La causa era que el feature `macos-native` de `clipvault-core`
estaba declarado como `macos-native = []` (vacío) en `Cargo.toml` y nunca se
activaba en la dependencia `clipvault-core` de `app/tauri/src-tauri`. La
guardia `#[cfg(all(target_os = "macos", feature = "macos-native"))]` en
`ignored_apps_service::pick_and_add_with_default_picker` siempre evaluaba a
`false` y la llamada terminaba en el brazo `BackendUnavailable` con
"macos-native feature not enabled".

- [x] 9.1 `crates/clipvault-core/Cargo.toml` reenvía el feature
      `macos-native = ["clipvault-platform/macos-native"]` en lugar de la
      lista vacía. La guardia `cfg` ahora activa la rama real.
- [x] 9.2 `app/tauri/src-tauri/Cargo.toml` añade un bloque
      `[target.'cfg(target_os = "macos")'.dependencies]` que activa
      `clipvault-core` con `features = ["macos-native"]` en builds macOS.
      Los builds Linux y otros hosts no reciben objc2 ni APIs macOS.
- [x] 9.3 Eliminar el `return self.pick_and_add(...)` que la guardia
      `cfg` antes ocultaba. Con el feature reenviado el bloque se compila
      y `clippy::needless_return` deja de ser un linter mudo.
- [x] 9.4 Test de regresión `macos_native_feature_resolves_the_macos_application_picker`
      en `crates/clipvault-core/src/ignored_apps_service.rs` que referencia
      `MacOsApplicationPicker::new` por la vía del trait
      `ApplicationPicker`. Si el feature vuelve a quedar declarado pero
      vacío el símbolo no existe y el test falla al compilar — la regresión
      queda pinchada en build time, no en runtime.
- [x] 9.5 `cargo tree -e features -p clipvault-core` muestra
      `clipvault-core feature "macos-native" → clipvault-platform feature
      "macos-native"` cuando la dependencia proviene de `clipvault-app`.
- [x] 9.6 `cargo fmt --all -- --check`, `cargo clippy --workspace
      --all-targets -- -D warnings` y `cargo test --workspace` en verde.
- [x] 9.7 `npm run check`, `npm run build` y `npm test` en el frontend sin
      errores ni warnings.
- [x] 9.8 `openspec validate blacklist-app-picker --strict --type change`
      sigue reportando `Change 'blacklist-app-picker' is valid`.
- [x] 9.9 `cargo clean -p clipvault-app && cargo build -p clipvault-app
      --target aarch64-apple-darwin` produce un binario cuyo
      `nm -gU target/aarch64-apple-darwin/debug/clipvault-app | grep
      MacOsApplicationPicker` lista cuatro símbolos
      (`new`, `ApplicationPicker::name`, `ApplicationPicker::pick`, y el
      drop glue del struct) y cuyo `nm | grep LinuxApplicationPicker`
      devuelve cero. La desensamblación de
      `IgnoredAppsService::pick_and_add_with_default_picker` invoca
      `MacOsApplicationPicker::new` directamente, confirmando que la rama
      activa es la nativa y no el brazo `BackendUnavailable`.
- [x] 9.10 La prueba manual 7.6 permanece pendiente y el cambio no se
      archiva automáticamente; sigue a la espera del run humano sobre
      macOS.

## 10. Corrección de la presentación del icono original

La selección añadía la fila correctamente pero `SettingsPanel.svelte`
ignoraba `icon_ref` y siempre renderizaba el fallback con la inicial.
Además, `icon_ref` almacena una referencia relativa
(`ignored-apps/com.apple.textedit.png`) que el WebView no puede
cargar como URL directa. Esta sección cierra el hueco:

- [x] 10.1 Nuevo módulo `clipvault-platform/src/app_assets.rs` que
      expone `resolve_icon_path` y `read_icon_bytes`. La función
      rechaza referencias vacías, absolutas, con `..`, fuera del
      prefijo `ignored-apps/` o cuyo `canonicalize` escape del
      directorio de assets (defensa contra symlinks). Aplica un tope
      de 512 KiB y verifica la firma PNG.
- [x] 10.2 Nuevo comando Tauri `clipvault_ignored_app_icon` en
      `app/tauri/src-tauri/src/commands.rs`. Toma el `icon_ref`
      relativo, llama a `read_icon_bytes` con `state.context().platform().data_dir`
      y devuelve `Vec<u8>` (PNG) o `CommandError` con `kind` estable
      (`invalid_icon_ref` o `icon_read_error`). Nunca devuelve una
      ruta absoluta al frontend. Se registra en
      `tauri::generate_handler!` desde `main.rs`.
- [x] 10.3 CSP de `tauri.conf.json` ampliada a `img-src 'self' data: blob:`
      para que el WebView pueda cargar los Object URLs creados por
      el frontend. No se añade acceso remoto al filesystem ni se
      cargan iconos remotos.
- [x] 10.4 Nuevo target `lib` en `app/tauri/src-tauri/Cargo.toml`
      que re-exporta los módulos `bootstrap`, `commands`, `state` y
      `tray`. Permite que los tests de integración ejecuten la misma
      canalización de validación sin levantar un runtime Tauri.
- [x] 10.5 Nuevo helper `clipvault_ignored_app_icon_for_test` en
      `commands.rs` (marcado `#[allow(dead_code)]`) usado por los
      tests de integración para ejercitar el wrapper sin Tauri.
- [x] 10.6 Wrapper frontend `ignoredAppIconCommand` en
      `app/tauri/frontend/src/lib/tauri.ts` que invoca el comando
      `clipvault_ignored_app_icon` con `{ iconRef }` y devuelve la
      `number[]` que Tauri serializa a partir de `Vec<u8>`.
- [x] 10.7 Nuevo módulo `app/tauri/frontend/src/lib/iconResolver.ts`
      con la función pura `resolveIconUrl(ref, loader)` y la fábrica
      `createIconResolver(loader)` que cachea `blob:` URLs, libera
      con `URL.revokeObjectURL` en `release`/`releaseFor` y reaprovecha
      el `Blob` cuando se vuelve a pedir el mismo `ref`.
- [x] 10.8 `SettingsPanel.svelte` ahora carga el icono por entrada
      con el resolver cuando `icon_ref` está presente y la respuesta
      no falla. Renderiza un `<img class="icon-image">` con `blob:`
      URL como `src` y vuelve a la inicial cuando `icon_ref === null`,
      cuando el backend rechaza la referencia o cuando el `<img>`
      emite `on:error`. La inicial se mantiene en un fallback
      `muted` para distinguir "sin metadata" de "metadata inválida".
      El componente libera todas las Object URLs en `onDestroy` y
      por entrada cuando se elimina una fila.
- [x] 10.9 Tests del módulo `app_assets`: rechazo de vacío, absoluto,
      `..`, fuera de scope, archivo inexistente, symlink escape
      (Unix), lectura correcta de un PNG válido, rechazo de payload
      no-PNG, rechazo de archivos sobre el tope y errores
      `kind_str`/`Display` estables.
- [x] 10.10 Tests de integración del comando en
      `app/tauri/src-tauri/tests/icon_command.rs`: bytes correctos
      para un ref válido; rechazo de absolutos, traversal, scope,
      archivos faltantes, symlinks fuera del directorio, payload no
      PNG y archivos sobre el tope; verificación de que la respuesta
      no contiene rutas absolutas, paths del usuario ni contenido
      del clipboard.
- [x] 10.11 Tests del resolver en
      `app/tauri/frontend/tests/iconResolver.test.ts`: rechazo de
      `null` sin invocar el loader, rechazo cuando el loader
      rechaza o devuelve `null`, conversión a `Blob` con tipo
      `image/png`, aceptación de payloads `Uint8Array`, cache de
      URLs (un único `createObjectURL` por `ref`), `release` y
      `releaseFor` que invocan `revokeObjectURL`, y comprobación de
      que el resolver sigue usable después de `release`.
- [x] 10.12 Tests del bridge Tauri en
      `app/tauri/frontend/tests/iconBridge.test.ts` y extensiones en
      `privacySettings.test.ts`: forwarding del argumento, respuesta
      cruda como `number[]`, propagación de rechazos tipados,
      verificación de que el wrapper no muta el `ref` ni filtra
      paths absolutos, contenido del clipboard, hashes ni snippets.
- [x] 10.13 `cargo fmt --all -- --check`, `cargo clippy --workspace
      --all-targets -- -D warnings` y `cargo test --workspace` en
      verde. `npm run check`, `npm run build` y `npm test` en verde.
      `openspec validate blacklist-app-picker --strict --type change`
      sigue reportando `Change 'blacklist-app-picker' is valid`.
- [x] 10.14 La tarea manual 7.6 permanece pendiente y el cambio no
      se archiva automáticamente.

## 11. Corrección de iconos grandes y nueva fila sin icono

La versión previa del selector macOS generaba PNGs a 1024×1024 para
paquetes como Chrome o Affinity, lo que producía archivos por encima
del límite de lectura de 512 KiB y forzaba la caída al fallback de
inicial incluso cuando el `icon_ref` estaba bien formado. Esta
sección reduce el icono en el momento de la selección, admite los
archivos legacy durante la transición y refuerza el contrato entre
`icon_ref` y la entrega real de bytes en la UI.

- [x] 11.1 Nueva constante `MAX_ICON_DIM = 256` exportada desde
      `clipvault_platform::app_assets`. El picker genera un bitmap
      cuyo lado mayor nunca supera este valor preservando el aspect
      ratio original.
- [x] 11.2 Nueva constante `MAX_ICON_BYTES_LEGACY = 4 MiB`. La
      función `read_icon_bytes` admite ahora archivos hasta ese
      tope para que los PNGs de 1024×1024 ya en disco sigan
      renderizándose; los nuevos iconos se quedan bajo el cap
      estricto de 512 KiB por la downsampling del paso 11.1.
- [x] 11.3 `macos_app_picker::render_bundle_icon_png` ahora delega
      en una nueva función `downscale_bitmap` que crea un
      `NSBitmapImageRep` a `MAX_ICON_DIM × MAX_ICON_DIM`,
      configura la calidad de interpolación `NSImageInterpolation::High`
      y dibuja el `CGImage` original escalado mediante
      `CGContext::draw_image`. El picker pasa a escribir PNGs de
      tamaño estable por debajo de 512 KiB.
- [x] 11.4 El comentario en `macos_app_picker::persist_icon_for_bundle`
      y `render_bundle_icon_png` documenta la invariante: el icono
      persistido siempre cabe en `MAX_ICON_DIM` y por tanto en
      `MAX_ICON_BYTES`.
- [x] 11.5 `SettingsPanel.svelte` ya distingue "icon_ref existe"
      (`iconUrls[entry.id]` poblado) de "el backend entregó bytes"
      (loader resolvió sin error). El comentario de `refreshIcons`
      ahora explica explícitamente la diferencia entre los tres
      estados visuales: imagen, fallback silenciado y fallback
      brillante transitorio durante la resolución.
- [x] 11.6 Regresiones añadidas en `app_assets`: el límite de
      legacy se acepta, el límite superior se sigue rechazando y la
      constante `MAX_ICON_DIM` queda anclada a 256.
- [x] 11.7 Regresiones añadidas en `macos_app_picker`: `target_dimensions`
      mantiene los bitmaps ya pequeños, capa el lado mayor a
      `MAX_ICON_DIM` y siempre produce dimensiones válidas para
      razones de aspecto no cuadradas.
- [x] 11.8 Regresiones añadidas en `icon_command.rs`: el comando
      Tauri acepta los iconos grandes legacy y rechaza cualquier
      archivo por encima de `MAX_ICON_BYTES_LEGACY` con la clase
      `icon_read_error`.
- [x] 11.9 Regresión añadida en `iconResolver.test.ts` que
      distingue entre refs que entregan bytes (cache poblada) y
      refs que el loader rechaza (`ok: false` sin URL), verificando
      además que las refs cacheadas no se vuelven a cargar.
- [x] 11.10 `cargo fmt --all -- --check`, `cargo clippy --workspace
      --all-targets -- -D warnings`, `cargo test --workspace`,
      `npm run check`, `npm run build`, `npm test` y
      `openspec validate blacklist-app-picker --strict --type change`
      en verde.
