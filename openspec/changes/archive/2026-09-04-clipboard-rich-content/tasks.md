## 1. OpenSpec y contratos

- [x] 1.1 Leer `project.md`, `AGENTS.md` y todos los artefactos de este
  cambio antes de tocar código.
- [x] 1.2 Confirmar que el cambio permanece separado de
  `platform-permission-guidance`, `clipboard-type-detection`,
  `history-card-layout` y `clipboard-management`.
- [x] 1.3 Definir el tipo neutral `ClipboardPayload` o equivalente para
  `Text` e `Image` y documentar sus invariantes.
- [x] 1.4 Definir la prioridad text-first y el comportamiento para formatos
  unsupported.
- [x] 1.5 Definir los nombres estables de capabilities y los outcomes de
  lectura/escritura de imágenes.
- [x] 1.6 Confirmar los contratos Tauri de `EntryRecord`, captura, recientes,
  asset bridge y paste sin payload binario en comandos de entrada.

## 2. Modelo de payload y adapters

- [x] 2.1 Extender el adapter de clipboard con lectura/escritura de payloads
  sin romper `read_text`/`write_text`.
- [x] 2.2 Mantener fakes de texto compatibles y agregar fake de imagen con
  dimensiones y bytes controlados.
- [x] 2.3 Implementar lectura de texto primero y fallback determinista a
  imagen cuando no haya texto utilizable.
- [x] 2.4 Implementar la lectura real de imagen en macOS mediante el adapter
  existente y la dependencia aprobada.
- [x] 2.5 Implementar la lectura/escritura real compatible con Linux X11 y
  distinguir explícitamente Linux Wayland.
- [x] 2.6 Mapear errores de formato, tamaño, sesión y backend a errores tipados
  sin contenido sensible.
- [x] 2.7 Validar overflow de `width * height * 4`, dimensiones cero y límites
  antes de reservar o copiar bytes.

## 3. Modelo de dominio y persistencia

- [x] 3.1 Agregar `ContentType::Image` con wire format estable `image`.
- [x] 3.2 Extender `EntryRecord` con `asset_ref`, `mime_type`,
  `payload_width` y `payload_height` opcionales.
- [x] 3.3 Crear la migración aditiva y secuencial siguiente a las existentes,
  previsiblemente `0008_clipboard_assets`.
- [x] 3.4 Mantener `content` no nulo para filas antiguas y documentar el
  sentinel vacío exclusivo de entradas de imagen.
- [x] 3.5 Actualizar todas las consultas de entries, recientes y lecturas por
  id sin romper filas textuales.
- [x] 3.6 Cubrir la migración, rollback cuando corresponda y apertura de una
  base creada antes de la capability.
- [x] 3.7 Mantener `content_size`/`content_hash` de texto sin cambios y
  definirlos para PNG normalizado.

## 4. Asset store local

- [x] 4.1 Implementar un asset store testeable detrás de un trait o servicio
  del core, sin dependencia de Tauri o Svelte.
- [x] 4.2 Normalizar el bitmap soportado a PNG canónico con la dependencia
  mínima justificada en `design.md`.
- [x] 4.3 Persistir sólo bajo `<data_dir>/assets/clipboard/` con referencias
  `clipboard/<sha256-lowercase>.png`.
- [x] 4.4 Escribir mediante temporal dentro del mismo directorio y rename
  atómico, sin dejar una entrada parcialmente válida.
- [x] 4.5 Reutilizar un asset existente cuando el hash normalizado coincide y
  el archivo supera la validación.
- [x] 4.6 Validar firma PNG, tamaño máximo, dimensiones y symlinks antes de
  leer o reutilizar un asset.
- [x] 4.7 Implementar recolección idempotente de assets no referenciados
  después de delete, clear y retention.
- [x] 4.8 Mantener aislados los namespaces `clipboard`, `ignored-apps` y
  `application-icons`.
- [x] 4.9 Cubrir fallos de escritura, commit SQLite, temporales huérfanos y
  dos filas que compartan un asset.

## 5. Pipeline de captura y privacidad

- [x] 5.1 Hacer que el watcher consuma payloads textuales o de imagen mediante
  el mismo flujo de source-app snapshot y PrivacyGate.
- [x] 5.2 Evaluar PrivacyGate antes de crear la fila o el asset permanente.
- [x] 5.3 Mantener el source-app identifier del snapshot original y no usar
  el foco posterior de la ventana de ClipVault.
- [x] 5.4 Persistir imagen y metadata sólo después de validación y
  normalización exitosas.
- [x] 5.5 Aplicar deduplicación por hash PNG normalizado sin cambiar el
  comportamiento de dedupe de texto.
- [x] 5.6 Emitir `clipvault://history-updated` sólo como señal metadata-only
  y con la información ya persistida.
- [x] 5.7 Garantizar que una captura blacklistada no cree fila, asset,
  metadata de aplicación ni evento.
- [x] 5.8 Asegurar que ningún log, error o diagnóstico incluya bytes, imagen,
  thumbnail, hash, contenido, snippet, ruta absoluta o identificador crudo.

## 6. Paste y capacidades

- [x] 6.1 Extender `PasteService` para seleccionar el payload según el tipo
  de entrada.
- [x] 6.2 Escribir imágenes con el adapter de clipboard y ejecutar después el
  `PasteController` existente.
- [x] 6.3 Mantener el orden transient de quick-paste: seleccionar, ocultar,
  escribir, pegar y resolver guidance.
- [x] 6.4 Devolver `CapabilityUnavailable` o equivalente cuando el host no
  soporta escritura de imagen, sin convertirla a texto.
- [x] 6.5 Reutilizar `platform-permission-guidance` y no presentar un permiso
  falso para una limitación estructural de Wayland.
- [x] 6.6 Mantener intacta la entrada histórica si falla la escritura o el
  pegado de la imagen.
- [x] 6.7 Extender la matriz de capabilities sólo con resultados verificables
  por sesión, no por asumir que el texto funciona.

## 7. Tauri y frontend

- [x] 7.1 Mantener `clipvault_capture_tick`, el bucle automático y
  `clipvault_recent_entries` como comandos delgados.
- [x] 7.2 Agregar `clipvault_clipboard_asset` o equivalente para recibir sólo
  una referencia relativa y devolver bytes validados.
- [x] 7.3 Registrar el comando y su capability sin acceso irrestricto al
  filesystem.
- [x] 7.4 Extender el bridge frontend para crear y revocar Blob URLs de assets
  de clipboard sin duplicar el resolver de iconos de aplicación.
- [x] 7.5 Reutilizar `HistoryCard.svelte` y `HistoryCardRail.svelte` para
  renderizar thumbnails en el área de preview.
- [x] 7.6 Mantener la card cuadrada, título, icon-only source app, pin/unpin y
  menú actuales.
- [x] 7.7 Usar fallback accesible cuando el asset falte, sea inválido o no
  pueda decodificarse; nunca mostrar el sentinel de `content`.
- [x] 7.8 Actualizar quick-paste para reconocer y seleccionar entradas de
  imagen sin cambiar navegación, Escape ni listeners.
- [x] 7.9 Mantener búsqueda textual limitada a entradas textuales, sin OCR ni
  indexación de bytes.
- [x] 7.10 Actualizar tipos TypeScript y conservar compatibilidad de entradas
  textuales antiguas.

## 8. Tests unitarios e integración

- [x] 8.1 Test de prioridad texto sobre imagen cuando ambas representaciones
  existen.
- [x] 8.2 Test de imagen sin texto que produce `ContentType::Image`.
- [x] 8.3 Test de formato no soportado que no crea entrada y no detiene el
  watcher.
- [x] 8.4 Tests de dimensiones inválidas, overflow y límites de tamaño.
- [x] 8.5 Tests de PNG canónico, hash, asset_ref y escritura atómica.
- [x] 8.6 Tests de dedupe de imagen y reutilización del asset.
- [x] 8.7 Tests de migración y `EntryRecord` compatible con filas antiguas.
- [x] 8.8 Tests de lectura segura: namespace, traversal, symlink, MIME, PNG,
  tamaño y dimensiones.
- [x] 8.9 Tests de delete, clear, retention, favoritos y recolección de assets.
- [x] 8.10 Tests de PrivacyGate antes de persistencia y ausencia de side
  effects para blacklist.
- [x] 8.11 Tests de paste de imagen exitoso, capability unavailable y fallo
  sin mutar la entrada.
- [x] 8.12 Tests de capacidades independientes para texto e imagen y de
  guidance por sesión.
- [x] 8.13 Tests Tauri del comando de assets y de respuestas sin paths
  absolutos ni payloads.
- [x] 8.14 Tests frontend de thumbnail, fallback, Blob URL, cleanup y
  ausencia de texto interno visible.
- [x] 8.15 Tests frontend de quick-paste, refresh por history-updated y no
  duplicación de listeners.
- [x] 8.16 Tests de privacidad de logs y eventos sin bytes, hash, contenido o
  snippets.

## 9. Verificación automatizada

- [x] 9.1 Ejecutar `cargo fmt --all -- --check`.
- [x] 9.2 Ejecutar `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 9.3 Ejecutar `cargo test --workspace`.
- [x] 9.4 Ejecutar `npm run check` en `app/tauri/frontend`.
- [x] 9.5 Ejecutar `npm run build` en `app/tauri/frontend`.
- [x] 9.6 Ejecutar `npm test` en `app/tauri/frontend`.
- [x] 9.7 Ejecutar `openspec validate clipboard-rich-content --strict --type change`.
- [x] 9.8 Revisar el diff, dependencias nuevas, archivos generados y logs
  antes de entregar.

## 10. Verificación manual

- [ ] 10.1 En macOS, copiar una imagen desde una aplicación permitida y
  confirmar que aparece en la misma rail y card que el texto.
- [ ] 10.2 Confirmar thumbnail, icono Image, título, source-app icon-only y
  acciones existentes.
- [ ] 10.3 Confirmar que texto e imágenes conviven sin romper las cards
  existentes.
- [ ] 10.4 Copiar la misma imagen dos veces y confirmar que no duplica fila ni
  asset.
- [ ] 10.5 Reiniciar ClipVault y confirmar que el thumbnail persiste.
- [ ] 10.6 Seleccionar la imagen desde quick-paste y pegarla en una aplicación
  receptora.
- [ ] 10.7 Eliminar la entrada y comprobar que el asset queda sin referencia
  y puede limpiarse.
- [ ] 10.8 Copiar una imagen desde una aplicación blacklistada y confirmar que
  no crea card ni asset.
- [ ] 10.9 Repetir en Linux X11 y documentar el resultado de capacidades.
- [ ] 10.10 Repetir en Linux Wayland cuando exista una sesión disponible y
  confirmar guidance correcta, sin falso permiso.
- [x] 10.11 No marcar como completada la verificación manual pendiente de
  `platform-permission-guidance` si no fue realizada realmente.

## 11. Entrega

- [x] 11.1 Actualizar `tasks.md` sólo con tareas efectivamente verificadas.
- [ ] 11.2 Entregar archivos modificados, decisiones de arquitectura, tests,
  manual pendiente y limitaciones.
- [x] 11.3 No archivar automáticamente `clipboard-rich-content`.
