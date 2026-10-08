# Proposal: canales de distribución RPM y Homebrew

## Resumen

Ampliar la distribución de escritorio de ClipVault con dos canales: un paquete
RPM Linux publicado junto a los artefactos actuales de GitHub Releases y un tap
propio de Homebrew para macOS que instala los DMG oficiales del proyecto.

El cambio amplía el alcance del cambio desktop-app-updates, que originalmente
dejaba RPM fuera. El tap será un repositorio público del proyecto, separado del
repositorio de la aplicación. No requiere una cuenta paga de Apple Developer;
por eso la instalación desde Homebrew conserva las limitaciones de Gatekeeper
de los DMG actuales.

## Motivación

ClipVault ya publica AppImage y DEB para Linux, y DMG para macOS. Un RPM hará
más directa la instalación en distribuciones que usan dnf o zypper. El tap
permitirá instalar ClipVault con Homebrew sin mantener un manifiesto ajeno al
repositorio ni depender de que el proyecto sea aceptado en homebrew/cask.

## Qué cambia

- Agregar el objetivo RPM x86_64 al empaquetado y al borrador de cada release
  de GitHub, manteniendo versionado y manifiesto alineados con los artefactos
  existentes.
- Ampliar la actualización de aplicaciones para que una instalación RPM sólo
  reciba e instale un RPM compatible, con la autorización del sistema que
  requiera el gestor de paquetes.
- Crear el tap público dgadduci/homebrew-tap con un cask de ClipVault que
  consuma los DMG oficiales para Apple Silicon e Intel, con versión y SHA-256
  verificables.
- Documentar los comandos de instalación, actualización y los requisitos de
  compatibilidad para cada canal.
- Explicar que Homebrew no elimina la cuarentena ni la advertencia de
  Gatekeeper: el primer inicio puede requerir la aprobación manual de ClipVault
  en macOS.

## Fuera de alcance

- Firma Developer ID, notarización de Apple o eliminación de las advertencias
  de Gatekeeper.
- Publicación del cask en homebrew/cask.
- Repositorios RPM administrados por OBS, Flathub, Snap Store, AUR u otros
  canales de paquetes.
- Cambiar los canales actuales AppImage, DEB o descarga directa DMG.
- Automatizar escrituras desde el workflow de ClipVault hacia otro repositorio
  mediante un token personal de larga duración.

## Impacto

Se extiende application-updates para contemplar RPM y se agregan las
capacidades linux-rpm-distribution y macos-homebrew-tap. La implementación
requiere mantener el cask sincronizado con cada release estable y verificar
paquetes RPM en las distribuciones que se declaren compatibles.
