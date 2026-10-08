# Diseño: canales de distribución RPM y Homebrew

## Decisiones

### Fuente de releases

GitHub Releases continúa siendo la fuente canónica de versiones y artefactos.
Los trabajos existentes agregan el RPM al borrador de release junto con
AppImage y DEB; no se introduce un servicio de publicación o repositorio
adicional. El tap referencia los DMG ya publicados por ese mismo release.

La excepción intencional es el repositorio Git del tap:
dgadduci/homebrew-tap contendrá únicamente la definición del cask y la
documentación propia que requiera Homebrew. Su actualización inicial será un
cambio revisado en ese repositorio, sin credenciales de escritura entre
repositorios guardadas en el workflow de ClipVault.

### Relación con desktop-app-updates

Este cambio amplía el alcance que desktop-app-updates dejó originalmente
fuera. Conserva la verificación de firma del updater, la aprobación explícita
del usuario, la autorización del sistema para reemplazar paquetes y la
preservación de los datos locales. El artefacto de una instalación RPM debe
ser RPM; nunca debe recibir el AppImage o el DEB por compartir la plataforma
Linux.

El updater Tauri reconoce RPM como tipo de instalador y busca un destino
específico por sistema, arquitectura y tipo de paquete. La release debe incluir
el artefacto y su firma para linux-x86_64-rpm; la acción oficial de Tauri genera
el manifiesto desde los artefactos updater firmados. La implementación debe
confirmar en el RPM que el binario quedó marcado con su tipo de bundle y que la
versión fijada del updater usa su ruta de instalación RPM con autorización del
sistema. No agregar un adaptador propio ni ejecutar comandos privilegiados
desde la interfaz si el flujo de Tauri ya cubre el caso. La firma Tauri protege
la actualización y no se debe describir como firma GPG del paquete RPM.

### Linux RPM

- Activar el bundler RPM de Tauri v2 para x86_64 y publicar el RPM en el mismo
  borrador de GitHub Release que los otros artefactos.
- Definir metadatos de paquete y dependencias desde la configuración Tauri.
  Usar la capacidad `gtk3` y los requisitos SONAME de Ayatana AppIndicator y
  WebKitGTK 4.1, para que cada gestor RPM resuelva sus propios nombres de
  paquetes. Confirmar que el RPM final declara esos requisitos. No agregar
  scripts de instalación que hagan cambios ajenos al registro normal del
  paquete de escritorio.
- Extender el manifiesto de actualización para distinguir RPM de AppImage y
  DEB. La instalación de una actualización debe solicitar autorización del
  sistema y cancelar sin alterar la versión instalada si el usuario la
  rechaza.
- Declarar compatibilidad sólo para distribuciones y versiones verificadas.
  Fedora y openSUSE son los primeros objetivos de validación; los requisitos
  efectivos de glibc y WebKitGTK deben decidir el mínimo publicable.
- No afirmar que el paquete crea un repositorio dnf/zypper: la primera versión
  se instala desde el archivo RPM adjunto al release.

### Tap y cask de Homebrew

- Crear el repositorio público dgadduci/homebrew-tap con Casks/clipvault.rb.
- El cask debe seleccionar el DMG oficial correspondiente a Apple Silicon o
  Intel, declarar la versión del release y un SHA-256 por arquitectura. No usar
  sha256 :no_check.
- Mantener la actualización propia de ClipVault como mecanismo de actualización
  en la aplicación (auto_updates true), y mantener el cask actualizado para
  nuevas instalaciones. Homebrew no debe reemplazar ni parchear el contenido
  del DMG.
- Conservar la cuarentena que aplica macOS a las descargas de casks. El tap no
  usará --no-quarantine, xattr ni instrucciones para desactivar Gatekeeper.
- Documentar la aprobación puntual de la aplicación mediante las opciones de
  macOS Privacy & Security si Gatekeeper bloquea el primer inicio.

La firma ad hoc actual no acredita una identidad de desarrollador ante
Gatekeeper. La firma Ed25519 del updater verifica integridad de actualizaciones
de ClipVault, pero tampoco sustituye Developer ID ni notarización.

## Riesgos y mitigaciones

- **Dependencias RPM distintas entre distribuciones:** declarar un conjunto
  pequeño de sistemas probados y revisar dependencias del paquete en Fedora y
  openSUSE antes de ampliar la matriz.
- **Confusión entre RPM y updater:** probar que cada formato instalado recibe
  sólo el artefacto y flujo de instalación compatibles.
- **Cask desactualizado:** incluir la actualización del cask en la lista de
  publicación de cada versión estable y comprobar versión, URLs y checksums.
- **Expectativas sobre Gatekeeper:** explicar antes de instalar que el tap
  simplifica la descarga e instalación, pero no elimina la aprobación de
  seguridad de macOS.

## Verificación

- Validar el RPM generado, su arquitectura, metadatos y dependencias; instalar,
  actualizar y desinstalarlo en sistemas RPM de la matriz aprobada.
- Probar la actualización desde una instalación RPM y el rechazo/cancelación
  de autorización, verificando que la aplicación y sus datos sigan disponibles.
- Validar el cask con las herramientas de Homebrew e instalarlo en macOS Intel
  y Apple Silicon usando los releases públicos.
- Confirmar el comportamiento real de cuarentena y Gatekeeper en una cuenta
  limpia, y comprobar la actualización desde la aplicación.
- Ejecutar validación OpenSpec y revisar diffs antes de cerrar el cambio.

## Referencias técnicas

- [Tauri v2: distribución RPM](https://v2.tauri.app/distribute/rpm/)
- [Tauri v2: updater](https://v2.tauri.app/plugin/updater/)
- [Tauri updater: selección de artefactos por tipo de bundle](https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/updater/src/updater.rs)
- [Tauri Action: generación de latest.json](https://github.com/tauri-apps/tauri-action/blob/v1/src/upload-version-json.ts)
- [Homebrew: crear y mantener un tap](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap)
- [Homebrew: Cask Cookbook](https://docs.brew.sh/Cask-Cookbook)
- [Homebrew: política de casks y Gatekeeper](https://github.com/Homebrew/brew/blob/main/docs/Acceptable-Casks.md)
- [Homebrew: seguridad y cuarentena](https://github.com/Homebrew/brew/blob/main/docs/Homebrew-Security-and-Supply-Chain.md)
