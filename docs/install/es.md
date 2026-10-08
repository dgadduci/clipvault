# Instalar ClipVault

Descarga ClipVault desde el [último release oficial](https://github.com/dgadduci/clipvault/releases/latest). Estas instrucciones se prepararon con `v0.0.20` (2026-10-07). La página de releases siempre muestra la versión actual.

## Elige la descarga

| Equipo | Archivo del release `v0.0.20` | Instalación |
| --- | --- | --- |
| Mac con Apple silicon | `ClipVault_0.0.20_aarch64.dmg` | Imagen de disco (`.dmg`) |
| Mac con procesador Intel | `ClipVault_0.0.20_x64.dmg` | Imagen de disco (`.dmg`) |
| Ubuntu GNOME, Intel/AMD de 64 bits | `ClipVault_0.0.20_amd64.deb` | Instalador de software de Ubuntu |
| Arch Linux con KDE Plasma Wayland, Intel/AMD de 64 bits | `ClipVault_0.0.20_amd64.AppImage` | AppImage |

Los nombres incluyen la versión del release. Si aparece una versión más nueva, elige el archivo que tenga el mismo sistema operativo y sufijo de procesador. No se publican paquetes Linux ARM, RPM, Flatpak, Snap, AUR ni un paquete nativo para Arch.

## macOS

1. En la Mac, abre **Menú Apple → Acerca de esta Mac**. Elige el archivo `aarch64` para Apple silicon o `x64` para Intel.
2. Abre el archivo `.dmg` descargado.
3. En la ventana que aparece, arrastra **ClipVault** a **Aplicaciones**. Cuando termine la copia, expulsa la imagen de disco.
4. Abre **Aplicaciones** e inicia ClipVault. Verás una pantalla breve mientras la aplicación se prepara y luego se abrirá la ventana principal.

### Si macOS bloquea el primer inicio

El release se compila sin notarización de Apple, por lo que macOS podría mostrar un aviso de seguridad. Confirma primero que descargaste ClipVault del release oficial indicado arriba. Intenta abrir ClipVault una vez; si macOS lo bloquea, ve a **Configuración del Sistema → Privacidad y seguridad → Abrir igualmente** y confirma el aviso. No desactives Gatekeeper globalmente. Consulta [las notas de seguridad de los releases](../releases.md#macos-builds-without-an-apple-developer-account).

## Ubuntu GNOME

Los pasos para instalar el `.deb` son iguales en las sesiones Ubuntu GNOME con Wayland y X11. La sesión puede afectar algunas integraciones durante el uso, pero no cambia qué paquete debes descargar.

1. Descarga `ClipVault_0.0.20_amd64.deb` desde el [último release](https://github.com/dgadduci/clipvault/releases/latest). Es para equipos Intel/AMD de 64 bits.
2. Abre **Archivos** y luego **Descargas**.
3. Haz doble clic en el archivo `.deb`. Ubuntu abrirá su instalador de software. Selecciona **Instalar** e ingresa la contraseña del equipo si te la solicita.
4. Abre **Mostrar aplicaciones**, busca **ClipVault** e inicia la aplicación.

El `.deb` publicado se instaló y abrió correctamente en Ubuntu GNOME Wayland y X11. También se publica un AppImage, pero su instalación en Ubuntu no se validó; por eso esta guía recomienda la ruta del `.deb`.

## Arch Linux con KDE Plasma

La descarga oficial para esta configuración es el AppImage x86_64. ClipVault no publica un paquete AUR ni un paquete nativo para Arch.

1. En Dolphin, abre la carpeta **Personal** y crea una carpeta llamada `Applications` si todavía no existe.
2. Descarga `ClipVault_0.0.20_amd64.AppImage` desde el [último release](https://github.com/dgadduci/clipvault/releases/latest) y muévelo de **Descargas** a `Personal/Applications`.
3. Cambia el nombre del archivo a `ClipVault.AppImage`. Al mantenerlo en esta ubicación, el acceso del panel podrá encontrarlo.
4. Haz clic derecho en el archivo, elige **Propiedades** y luego **Permisos**. Activa **Es ejecutable** y cierra la ventana.
5. Haz doble clic en el AppImage. Si KDE pregunta si quieres ejecutar o mostrar el archivo, elige **Ejecutar**.
6. Cuando ClipVault esté abierto, haz clic derecho en su icono del panel inferior y elige **Anclar al Administrador de tareas**.

ClipVault se inició correctamente desde `Personal/Applications` en Arch KDE
Plasma Wayland y se creó un nuevo acceso del panel desde esa ubicación. El
El lanzador anterior todavía apunta a una ubicación antigua y no se pudo quitar;
usa el nuevo acceso para el AppImage actual. El lanzador viejo no impide usar el
nuevo. Al actualizar, reemplaza `ClipVault.AppImage` en esa misma carpeta para
conservar la ruta. No hace falta contraseña de administrador ni un gestor de
paquetes.

## Primer inicio e integraciones de escritorio opcionales

ClipVault muestra una pantalla breve mientras se inicia y luego abre la ventana principal. Puedes empezar a usar la aplicación sin activar una integración de escritorio.

En GNOME Wayland o KDE Plasma Wayland, una integración opcional puede identificar la aplicación de origen de una captura y habilitar los atajos globales de ClipVault. Para configurarla después, abre **Configuración → Integraciones de escritorio** en ClipVault y sigue el estado que se muestra. Puedes omitir o postergar este paso; no hace falta para instalar o abrir ClipVault.

Linux X11 y Wayland son sesiones diferentes. Una prueba en una no confirma el comportamiento de la otra. Consulta la matriz de verificación antes de depender de una función específica de la plataforma.

## Estado de verificación

| Configuración | Prueba manual de la aplicación | Pasos de instalación limpia de esta guía |
| --- | --- | --- |
| macOS | El DMG publicado se instaló y abrió; no consta el procesador ni la versión | Completado; procesador/versión no registrados |
| Ubuntu GNOME Wayland | El `.deb` publicado se instaló y abrió correctamente | Completado |
| Ubuntu GNOME X11 | El `.deb` publicado se instaló y abrió correctamente | Completado |
| Arch KDE Plasma Wayland | El AppImage publicado se abrió desde `Personal/Applications` y se creó un nuevo acceso del panel; el lanzador anterior aún apunta a una ruta vieja | Completado; el lanzador anterior permanece |
| Otras distribuciones y escritorios Linux | Estas pruebas no lo determinan | No se documentan como compatibles |

## Privacidad y ayuda

El historial del portapapeles permanece en tu equipo. Para buscar actualizaciones, ClipVault se conecta a GitHub mediante HTTPS y envía la versión de la aplicación, el sistema operativo y la arquitectura del procesador; no envía el contenido del portapapeles. Consulta las [notas del release y el actualizador](../releases.md).

Si necesitas ayuda, consulta [Soporte](../../SUPPORT.md). No incluyas contenido del portapapeles, contraseñas, tokens ni claves privadas en una solicitud de soporte.
