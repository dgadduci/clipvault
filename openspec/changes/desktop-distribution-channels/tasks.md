# Tareas: canales de distribución RPM y Homebrew

## 1. Paquete RPM y actualización

- [x] 1.1 Configurar el bundler RPM de Tauri para x86_64, incluidos el archivo
  de escritorio, sus archivos adicionales, metadatos y dependencias. La CLI
  aceptó la configuración y el bundler aplicó el marcador RPM al binario local.
- [ ] 1.2 Completar y revisar un RPM: arquitectura, metadatos, archivos,
  dependencias declaradas y marcador en el binario empaquetado.
- [x] 1.3 Ampliar el workflow de releases para adjuntar el RPM al borrador,
  conservando la igualdad de versión y los artefactos actuales.
- [ ] 1.4 Confirmar que el release incluye el RPM firmado por Tauri y que
  latest.json contiene linux-x86_64-rpm; verificar la actualización con
  autorización del sistema y que nunca entrega un AppImage o DEB a RPM.
- [ ] 1.5 Probar instalación, actualización aceptada, actualización cancelada y
  desinstalación en Fedora y openSUSE; documentar los mínimos efectivamente
  soportados.
- [x] 1.6 Actualizar README y guías de instalación/releases para distinguir el
  RPM futuro del contenido actual de `v0.0.20`, e indicar que la compatibilidad
  de distribuciones RPM sigue pendiente de pruebas manuales.

## 2. Tap propio de Homebrew

- [ ] 2.1 Crear dgadduci/homebrew-tap como repositorio público con el cask de
  ClipVault y las URLs oficiales de DMG para ambas arquitecturas.
- [ ] 2.2 Mantener la versión y SHA-256 de cada arquitectura sincronizados con
  los artefactos del release; no usar sha256 :no_check.
- [ ] 2.3 Verificar el cask e instalarlo en macOS Apple Silicon e Intel,
  comprobando el flujo real de Gatekeeper y la actualización propia de la app.
- [ ] 2.4 Documentar cómo añadir el tap, instalar ClipVault y aprobar
  puntualmente su primer inicio si Gatekeeper lo bloquea.

## 3. Documentación y cierre

- [ ] 3.1 Actualizar las guías de instalación, releases y README en los idiomas
  aplicables, diferenciando claramente RPM descargable, tap propio y canales
  existentes.
- [ ] 3.2 Confirmar que no se afirma compatibilidad con distribuciones RPM no
  verificadas ni que Homebrew elimina las advertencias de Gatekeeper.
- [ ] 3.3 Ejecutar las verificaciones relevantes, validar OpenSpec y revisar el
  diff y los archivos generados antes de cerrar el cambio.
