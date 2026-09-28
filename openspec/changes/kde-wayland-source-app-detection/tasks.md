# Tareas de implementación

Codex mantiene el contrato y el diseño. MiniMax implementa sólo después de
que esta propuesta sea aprobada.

## 1. Diagnóstico y compatibilidad KWin

- [x] 1.1 Revisar `git status --short`, `git diff --check`, cambios activos y
  baseline; no modificar código, configuración ni assets ajenos.
- [x] 1.2 En Arch Linux KDE Plasma Wayland, determinar si falla la obtención de
  `source_app` o el enriquecimiento `.desktop`/icono; registrar sólo estados
  metadata-only.
- [x] 1.3 Registrar versiones de Arch, Plasma y KWin; verificar en runtime
  `activeWindow`, activación, `desktopFileName` y comportamiento con apps
  nativas y XWayland.
- [x] 1.4 Verificar `zbus ~5.13.2` con el MSRV del workspace y validar en KWin
  la activación persistente/reversible, preservación de claves ajenas y
  publicación del snapshot inicial cuando ClipVault arranca después de KWin.
  Si falla cualquiera de esos puntos o se requiere una API privada adicional,
  detener la fase 2 y actualizar `design.md`.

## 2. Integración de origen KWin

No comenzar esta fase hasta verificar y completar 1.4 en el runtime KDE
objetivo.

- [x] 2.1 Añadir el script KWin de usuario con snapshot inicial y seguimiento
  de foco; enviar únicamente estado e identificador validado.
- [x] 2.2 Añadir el bridge D-Bus Linux con validación de sender/protocolo,
  errores tipados, desconexión y limpieza del snapshot; no incluir contenido,
  título, PID ni rutas.
- [x] 2.3 Añadir instalación, consentimiento, activación, diagnóstico y
  desinstalación idempotentes que sólo afecten recursos propios de ClipVault.
- [x] 2.4 Seleccionar KWin sólo en KDE Plasma Wayland y conectarlo al probe/cache
  existente sin crear otro watcher.
- [x] 2.5 Mantener `PrivacyGate` antes de metadata/íconos y reutilizar el
  `LinuxApplicationMetadataProvider` y los campos persistidos existentes.

## 3. Regresiones automatizadas

- [x] 3.1 Probar selección y precedencia de probes en KDE Wayland, GNOME
  Wayland, GNOME X11, Linux X11 y macOS.
- [x] 3.2 Probar publicación, rechazo de identificadores inválidos,
  actualización, desconexión y limpieza sin identidad anterior obsoleta.
- [x] 3.3 Probar errores/versionado del bridge y que el diagnóstico no exponga
  títulos, PID, rutas ni valores crudos.
- [x] 3.4 Probar resolución local de nombre/icono y fallback cuando falten
  datos `.desktop`, sin bloquear la captura.
- [x] 3.5 Probar que una aplicación ignorada se rechaza antes de consultar el
  provider o crear assets.
- [x] 3.6 Verificar build/check Linux y macOS, `cargo fmt --all -- --check`,
  regresiones afectadas y `openspec validate` estricto.

## 4. Verificación manual

- [x] 4.1 En Arch Linux KDE Plasma Wayland, verificar nombre e icono en
  capturas de varias aplicaciones nativas compatibles.
- [ ] 4.2 En Arch Linux KDE Plasma Wayland, verificar que una app XWayland
  conserva su origen y que el estado de foco no queda obsoleto.
- [ ] 4.3 Confirmar que macOS conserva nombre e icono de origen.
- [ ] 4.4 Confirmar que Linux GNOME X11 conserva nombre e icono de origen.
- [ ] 4.5 Confirmar que Linux GNOME Wayland conserva la integración, nombre e
  icono actuales.
- [ ] 4.6 Verificar consentimiento, desactivación y desinstalación en KDE;
  registrar versiones y estados sin datos sensibles.

## 5. Cierre

- [x] 5.1 Revisar diff, warnings, privacidad y estado de assets; no tocar
  assets persistidos preexistentes.
- [x] 5.2 Dejar sin marcar cualquier plataforma/runtime que no se haya
  ejecutado manualmente; no sincronizar ni archivar automáticamente.
