# Tareas de implementación

El usuario reportó el 2026-09-28 un estado inicial `awaiting_consent` pese al
consentimiento y paquete habilitado. La consulta directa a KWin confirmó que
el script no estaba cargado (`isScriptLoaded=false`): `kwinrc` contenía la clave
mal formada `Plugins/<id>Enabled` dentro de `[Plugins]`; KWin espera
`<id>Enabled`. La build corrigió y migró la clave preservando su valor. El
2026-09-29 la prueba manual aprobó la detección del nombre e icono de origen en
las capturas de Arch/KDE/Wayland. El usuario aprobó el 2026-09-29 las pruebas
manuales de origen XWayland y del ciclo de vida KDE, además de las pruebas de
regresión macOS/Ubuntu; no se observaron regresiones.

## 1. Diagnóstico y compatibilidad KWin

- [x] 1.1 Revisar `git status --short`, `git diff --check`, cambios activos y
  baseline; no modificar código, configuración ni assets ajenos.
- [x] 1.2 Validar por comportamiento la normalización de `desktopFileName` en
  aplicaciones nativas Wayland y XWayland: nombre e icono de origen correctos
  en las capturas manuales aprobadas el 2026-09-29. No se inspeccionan ni
  registran valores crudos; KWin no expone ese identificador en los logs de
  diagnóstico por privacidad.
- [x] 1.3 Registrar versiones de Arch, Plasma y KWin; verificar en runtime
  `activeWindow`, activación, `desktopFileName` y comportamiento con apps
  nativas y XWayland.
- [x] 1.4 Validar en KWin la activación persistente/reversible, preservación de
  claves ajenas y publicación del snapshot inicial cuando ClipVault arranca
  después de KWin. Pruebas manuales aprobadas por el usuario el 2026-09-29.

## 2. Integración de origen KWin

La implementación está corregida y compilada; la validación en runtime KDE
de 1.4 sigue pendiente antes de cerrar el cambio.

- [x] 2.1 Mantener el script KWin JavaScript con snapshot inicial y seguimiento
  de foco; publicar argumentos compatibles con `callDBus` y el protocolo 2.
- [x] 2.2 Alinear el bridge D-Bus Linux con `Publish(s id, s state, s version)`
  y mantener validación de sender/protocolo, errores tipados, desconexión y
  limpieza del snapshot; no incluir contenido, título, PID ni rutas.
- [x] 2.3 Actualizar también el KPackage propio al arrancar o reintentar una
  integración ya consentida, retirar el `main.qml` legado del propio paquete,
  descargar el script activo propio antes de reconfigurar y preservar estados
  deshabilitado y desinstalado.
- [x] 2.4 Seleccionar KWin sólo en KDE Plasma Wayland y conectarlo al probe/cache
  existente sin crear otro watcher.
- [x] 2.5 Mantener `PrivacyGate` antes de metadata/íconos y reutilizar el
  `LinuxApplicationMetadataProvider` y los campos persistidos existentes.
- [x] 2.6 Canonicalizar el `desktopFileName` de KWin: recortar una ruta cuando
  exista y añadir `.desktop` a un basename sin extensión antes de publicarlo.
  Conservar la validación estricta del receiver D-Bus. La captura manual aprobó
  el enriquecimiento de nombre e icono; el formato bruto queda pendiente en 1.2.
- [x] 2.7 Añadir confirmaciones locales de carga y publicación sin registrar el
  identificador de aplicación, para distinguir fallos del script, D-Bus y lookup.
- [x] 2.8 Corregir el acceso a `kwinrc`: usar la clave `<id>Enabled` dentro de
  `[Plugins]`, migrar sólo la clave heredada mal formada de ClipVault y
  preservar su valor y todas las claves ajenas. Build Linux y validación
  OpenSpec pasaron; el relanzamiento y snapshot inicial quedaron verificados
  manualmente en 1.4.

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
  regresiones afectadas y `openspec validate` estricto para la corrección.
  Verificado el 2026-09-29: `cargo check -p clipvault-app`, formato, 13 tests
  de integración afectados y OpenSpec estricto pasaron en Linux; la prueba
  manual aprobada por el usuario en macOS confirma que la build de esa máquina
  funciona. No se hizo cross-check de macOS desde este host Linux.

## 4. Verificación manual

- [x] 4.1 En Arch Linux KDE Plasma Wayland, verificar nombre e icono en
  capturas. Prueba manual aprobada el 2026-09-29: ClipVault detecta nombre e
  icono de la aplicación de origen.
- [x] 4.2 En Arch Linux KDE Plasma Wayland, verificar que una app XWayland
  conserva su origen y que el estado de foco no queda obsoleto. Prueba manual
  aprobada por el usuario el 2026-09-29.
- [x] 4.3 Verificar manualmente macOS sin regresiones. Aprobado por el usuario
  el 2026-09-29.
- [x] 4.4 Verificar manualmente Ubuntu/X11 sin regresiones. Aprobado por el
  usuario el 2026-09-29.
- [x] 4.5 Verificar manualmente Ubuntu/Wayland sin regresiones de captura.
  Aprobado por el usuario el 2026-09-29; el estado específico del warning de
  fallback se registra por separado en el cambio
  `linux-wayland-clipboard-fallback-logging`.
- [x] 4.6 Verificar consentimiento, desactivación y desinstalación en KDE;
  registrar versiones y estados sin datos sensibles. Prueba manual aprobada
  por el usuario el 2026-09-29.

## 5. Cierre

- [x] 5.1 Revisar diff, warnings, privacidad y estado de assets; no tocar
  assets persistidos preexistentes.
- [x] 5.2 Dejar sin marcar cualquier plataforma/runtime que no se haya
  ejecutado manualmente; no sincronizar ni archivar automáticamente.
