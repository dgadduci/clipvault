# Tareas: canales de distribución RPM y Homebrew

## 1. Paquete RPM y actualización

- [x] 1.1 Configurar el bundler RPM de Tauri para x86_64, incluidos el archivo
  de escritorio, sus archivos adicionales, metadatos y dependencias. La
  configuración Linux está en `tauri.linux.conf.json`; el build local generó
  el RPM y marcó el binario como RPM.
- [x] 1.2 Completar y revisar un RPM: `cargo tauri build --bundles rpm`
  produjo `ClipVault-0.0.20-1.x86_64.rpm`. La cabecera RPM confirma nombre
  `clip-vault`, versión `0.0.20`, release `1`, arquitectura `x86_64` y
  dependencias `gtk3`, Ayatana AppIndicator y WebKitGTK 4.1; el payload
  contiene 13 rutas (binario, recursos, entradas `.desktop` e íconos) y el
  binario empaquetado contiene el marcador `__TAURI_BUNDLE_TYPE_VAR_RPM`.
  La inspección se hizo sobre el formato RPM porque `rpm` no está instalado
  en Ubuntu. Como prueba adicional, Fedora 44 en contenedor instaló y retiró
  el paquete con DNF, y `ldd` no mostró bibliotecas faltantes; la prueba
  openSUSE reportada tampoco mostró `not found` en `ldd`.
- [x] 1.3 Ampliar el workflow de releases para adjuntar el RPM al borrador,
  conservando la igualdad de versión y los artefactos actuales.
- [ ] 1.4 Confirmar que el release incluye el RPM firmado por Tauri y que
  latest.json contiene linux-x86_64-rpm; verificar la actualización con
  autorización aceptada/cancelada y que nunca entrega un AppImage o DEB a RPM.
  El RPM está adjunto a `v0.0.20` como descarga directa, pero no tiene firma
  Tauri y no figura en el `latest.json` existente. La inspección estática de
  `tauri-plugin-updater 2.12.0` confirmó selección por tipo de bundle,
  validación del formato RPM y ejecución de `rpm -U` mediante `pkexec`/`sudo`;
  faltan la publicación firmada en el manifiesto y la prueba del flujo real de
  actualización.
- [x] 1.5 Ejecutar smoke tests del RPM en contenedores Fedora y openSUSE. En
  Fedora 44, DNF instaló y retiró el paquete, las consultas RPM funcionaron y
  `ldd` no mostró `not found`; la prueba openSUSE reportada verificó `ldd` y
  tampoco mostró bibliotecas faltantes. No se registró la versión openSUSE ni
  una instalación/eliminación allí. Estos checks validan el paquete y sus
  dependencias, sin declarar compatibilidad de escritorio.
- [x] 1.6 Actualizar README y guías para indicar que `v0.0.20` incluye el RPM
  como descarga directa, que aún no está en el manifiesto firmado del
  actualizador y que las pruebas de contenedor no establecen compatibilidad de
  escritorio.
- [ ] 1.7 En entornos de escritorio Fedora/openSUSE que se quieran declarar
  compatibles, arrancar ClipVault desde el RPM y comprobar que la desinstalación
  conserva los datos del usuario; registrar las distribuciones y versiones que
  pasen esas pruebas antes de ampliar la matriz de compatibilidad.

## 2. Tap propio de Homebrew

- [x] 2.1 Crear dgadduci/homebrew-tap como repositorio público con el cask de
  ClipVault y las URLs oficiales de DMG para ambas arquitecturas. El tap
  público contiene el commit `4ab8b36` con `Casks/clipvault.rb`.
- [x] 2.2 Mantener la versión y SHA-256 de cada arquitectura sincronizados con
  los artefactos del release; no usar sha256 :no_check. El cask de `v0.0.20`
  declara ambos SHA-256 oficiales y pasó `brew style`; los valores se
  contrastaron con los digests de assets de GitHub Releases.
- [ ] 2.3 Verificar el cask e instalarlo en macOS Apple Silicon e Intel,
  comprobando el flujo real de Gatekeeper y la actualización propia de la app.
  En Apple Silicon, Homebrew exigió confiar explícitamente en el cask; tras
  instalarlo, Gatekeeper bloqueó el primer inicio y la aprobación puntual en
  macOS Privacy & Security permitió abrir ClipVault. La misma instalación,
  bloqueo esperado de Gatekeeper, aprobación puntual y apertura funcionaron en
  Intel. Faltan persistencia tras reinstalación y una actualización real.
- [x] 2.4 Documentar cómo añadir el tap, instalar ClipVault y aprobar
  puntualmente su primer inicio si Gatekeeper lo bloquea, en el README del
  tap y las guías de instalación en inglés y español. No se documentan
  bypasses de cuarentena ni de Gatekeeper. La documentación también indica la
  confianza limitada por cask que Homebrew exige para dgadduci/tap.

## 3. Documentación y cierre

- [x] 3.1 Actualizar las guías de instalación, releases y README en los idiomas
  aplicables, diferenciando claramente RPM descargable, tap propio y canales
  existentes. Se actualizaron los README inglés/español, las guías de
  instalación inglés/español y `docs/releases.md`.
- [x] 3.2 Confirmar que no se afirma compatibilidad con distribuciones RPM no
  verificadas ni que Homebrew elimina las advertencias de Gatekeeper. Las guías
  conservan RPM como descarga directa sin soporte de escritorio declarado y
  explican que Homebrew conserva cuarentena y Gatekeeper.
- [ ] 3.3 Ejecutar las verificaciones relevantes, validar OpenSpec y revisar el
  diff y los archivos generados antes de cerrar el cambio. En esta sesión
  se verificó el bundle RPM y `git diff --check`; la validación por CLI queda
  pendiente porque `openspec` no está instalado. Esta tarea global tampoco
  cierra mientras sigan pendientes los pasos de otros canales.
