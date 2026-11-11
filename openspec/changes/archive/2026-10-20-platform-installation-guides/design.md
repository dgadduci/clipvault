# Diseño: instalación y primeros pasos por plataforma

## Estructura

Crear `docs/install/README.md` como selector de idioma y dos guías mantenibles,
`docs/install/en.md` y `docs/install/es.md`. Cada guía cubrirá macOS, Ubuntu
GNOME y Arch KDE Plasma, con secciones diferenciadas por método de instalación
y por sesión Linux cuando cambien los pasos.

## Selección de artefacto

- **macOS:** usar el `.dmg` oficial de la arquitectura correspondiente. Explicar
  el aviso de Gatekeeper de los builds ad-hoc solo con los pasos seguros que ya
  documenta `docs/releases.md`; no recomendar desactivar Gatekeeper.
- **`.deb` Linux:** presentarlo para Ubuntu y derivados compatibles de Debian/
  Ubuntu (por ejemplo, Linux Mint), x86_64, sujeto a que estén disponibles las
  dependencias declaradas por el paquete. Describir como flujo comprobado sólo
  la instalación gráfica probada en Ubuntu GNOME; los nombres de menús pueden
  variar en otras distribuciones.
- **AppImage Linux:** presentarlo como el formato portátil publicado para la
  mayoría de distribuciones Linux x86_64, incluida Arch. Aclarar que puede
  requerir soporte FUSE y que la disponibilidad del formato no equivale a una
  prueba de cada distribución, escritorio o sesión.
- **Arch KDE Plasma:** usar el AppImage oficial si es la ruta verificada. No
  presentar el `.deb` como paquete de Arch ni afirmar que exista paquete AUR,
  RPM o integración del sistema que el proyecto no publique.
  Mantenerlo en una carpeta estable del usuario (`~/Applications`) y crear el
  acceso del panel desde esa ubicación, para que no quede apuntando a un
  AppImage de versión anterior en Descargas. La guía puede indicar que un
  lanzador viejo siga presente si no se pudo quitar; su limpieza no es requisito
  para iniciar el AppImage nuevo y no debe describirse como un paso verificado.

El workflow actual publica macOS Apple Silicon e Intel y Linux x86_64 `.deb` y
`.AppImage`. La guía debe revisar los nombres actuales de cada release antes de
enlazarlos y preferir una URL estable a la última versión publicada.

## Combinaciones y primera ejecución

La guía distinguirá macOS, Ubuntu GNOME y Arch KDE Plasma. El selector de
descargas describirá las familias de distribuciones compatibles con los
formatos publicados, mientras la matriz anotará sólo las combinaciones
Linux/sesión probadas manualmente. Para Linux anotará por separado X11 y
Wayland, el formato usado y el resultado manual conocido.
Como mínimo registrará las combinaciones que el mantenedor confirme como
probadas: Ubuntu GNOME Wayland y Arch KDE Plasma Wayland. El estado de Ubuntu
GNOME X11 se comprobará por separado; no se deducirá del resultado Wayland.

Los pasos de primera ejecución deben coincidir con la UI del release: splash,
permisos relevantes, integración opcional y accesos de ajustes. Cuando un paso
solo aplique a un entorno, se presentará como opcional y no como requisito para
abrir ClipVault. Las instrucciones no deben recomendar comandos `cargo`, `npm`
ni cambios amplios a la seguridad del sistema.

## Mantenimiento y seguridad

Cada página indicará su versión/fecha de revisión o una fuente de versión
verificable. La prueba de las instrucciones usará un usuario limpio o un
entorno de prueba, sin depender de contenido del portapapeles personal. No se
subirán logs con contenido capturado, rutas privadas o credenciales.

El release público comprobado al preparar estas guías es `v0.0.20`, publicado
el 2026-10-07. Los enlaces llevan a la página estable de releases y se
registran los nombres de artefactos visibles en esa versión. Las pruebas
manuales anteriores de la aplicación no cuentan por sí solas como validación
de una instalación limpia de cada formato de paquete.

Toda función, paquete y permiso se comprobará contra la última versión
publicada y las guías de soporte. Si una combinación cambia, actualizar ambos
idiomas y la matriz en una misma revisión.
