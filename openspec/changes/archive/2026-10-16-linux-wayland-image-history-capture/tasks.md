# Tareas de implementación

## 1. Investigación y reproducción

- [x] 1.1 Reproducción original reportada por el usuario en Ubuntu Wayland:
  primera captura correcta y capturas posteriores ausentes. La verificación
  manual posterior fue aprobada por el usuario; no se guardó contenido de
  imágenes.
- [x] 1.2 Seguir la revisión y el payload por el adapter y el watcher. Causa
  confirmada: `FallbackRevisionState::observe_payload` fabricaba una revisión
  conocida solo con el texto; todas las imágenes sucesivas tenían `None` y
  repetían la revisión, que el watcher trataba como autoritativa antes de
  comparar el fingerprint de imagen.
- [x] 1.3 Actualizar este diseño con la causa confirmada y la decisión de usar
  `UNKNOWN` cuando no existe una revisión confiable; no requiere una decisión
  arquitectónica nueva.

## 2. Corrección y regresiones

- [x] 2.1 El fallback de `ArboardClipboard` devuelve `UNKNOWN` si no existe
  un evento confiable; se conservan las revisiones XFixes para X11.
- [x] 2.2 La regresión
  `watcher_captures_successive_images_without_a_trusted_revision` captura tres
  imágenes distintas como entradas y assets separados.
- [x] 2.3 La misma regresión confirma que una imagen idéntica no se vuelve a
  capturar y que el watcher se recupera de una lectura no compatible.
- [x] 2.4 La corrección no agrega logs ni diagnósticos con payload; en la suite
  `clipboard_rich_content` pasan las regresiones de mensajes y resultados
  metadata-only.

## 3. Verificación

- [x] 3.1 Pasaron `cargo test -p clipvault-platform --lib
  --features clipboard-arboard,linux-x11,arboard-wayland-data-control
  runtime::clipboard_arboard::tests` (17 tests) y `cargo test -p clipvault-core
  --test clipboard_rich_content` (49 tests).
- [x] 3.2 El usuario aprobó la prueba manual de varias capturas en Ubuntu
  Wayland con ClipVault abierto.
- [x] 3.3 Pasaron `openspec validate linux-wayland-image-history-capture
  --strict --type change`, `cargo fmt --all -- --check` y `git diff --check`.
