# Propuesta: actualizaciones firmadas de la aplicación desktop

## Problema

Las instalaciones desktop de ClipVault no disponen de una comprobación ni una
instalación de actualizaciones desde la aplicación. La versión actual tampoco
tiene bundling activo ni un workflow de releases de GitHub. Cada nueva versión
requiere distribuir manualmente nuevos instaladores y avisar a los usuarios.

## Qué cambia

- Publicar releases versionadas en GitHub con GitHub Actions y los artefactos
  de macOS y Linux.
- Integrar Tauri Updater con artefactos firmados y un manifiesto estático
  generado para cada release.
- Habilitar las actualizaciones también en paquetes de producción generados
  localmente con `cargo tauri build`; la clave pública de verificación forma
  parte de la configuración de la aplicación y la clave privada permanece
  fuera del repositorio.
- Mantener alineadas las versiones minor de las dependencias Rust y JavaScript
  de Tauri para que el build local de producción complete la validación CLI.
- Comprobar actualizaciones de forma asíncrona al iniciar una build de
  producción. Mostrar el resultado en “Acerca de” y solicitar confirmación
  antes de descargar e instalar una actualización o reiniciar.
- Actualizar instalaciones `.app`/`.dmg` en macOS y `.AppImage` y `.deb` en
  Linux. La actualización de `.deb` debe conservar la autorización del sistema
  necesaria para modificar una instalación gestionada por paquetes.
- Mantener el historial y demás datos de usuario localmente. La consulta y la
  descarga sólo contactan a GitHub para obtener información y artefactos de la
  aplicación; nunca incluyen contenido del portapapeles.
- Mantener los textos de estado y las acciones en los catálogos `en`, `es`,
  `pt`, `de` y `fr`.

Las instalaciones anteriores a esta capacidad necesitarán instalar una vez
manualmente la primera versión que incluya Tauri Updater.

## Fuera de alcance

- Actualizaciones de Windows, Android o iOS.
- Comprobaciones e instalaciones desde `cargo tauri dev`.
- Paquetes RPM, repositorios APT propios o gestores de paquetes adicionales.
- Instalación silenciosa, descargas automáticas o reinicio sin consentimiento.
- Servidor de actualización propio, cuentas, telemetría o envío de datos de
  uso.
- Rollback automático de versiones.

## Impacto esperado

- Tauri: activar bundling, registrar los plugins updater y process, configurar
  la clave pública y el endpoint de GitHub Releases, y permitir las capacidades
  Tauri necesarias.
- Frontend: coordinar la comprobación de inicio y el estado de actualización
  desde “Acerca de”, con progreso, errores recuperables y confirmación previa a
  instalar/reiniciar.
- Releases: añadir un workflow de GitHub Actions disparado por tags SemVer,
  construir los targets soportados, firmar los artefactos updater y crear
  releases en borrador para revisión antes de publicarlos. Los builds macOS
  usan firma ad hoc, sin Developer ID ni notarización, y documentan la
  aprobación manual que Gatekeeper puede exigir.
- Seguridad operativa: guardar la clave privada del updater como secreto de
  CI, entregar la clave pública al build como variable de GitHub Actions y
  conservar un respaldo seguro de la clave privada.
- Verificación: probar el manifiesto y la instalación en cada formato
  publicado, con atención especial al flujo `.deb` con autorización del
  sistema.
