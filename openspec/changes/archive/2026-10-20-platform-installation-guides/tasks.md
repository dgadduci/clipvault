## 1. Inventario de versiones y combinaciones

- [x] 1.1 Confirmar los artefactos y nombres del último release público para
  macOS Apple Silicon/Intel y Linux x86_64.
- [x] 1.2 Confirmar los pasos gráficos en Ubuntu GNOME Wayland y X11. El `.deb`
  publicado se instaló y abrió en ambas sesiones; no se recomienda AppImage
  para Ubuntu porque esa ruta sigue sin validarse.
- [x] 1.3 Confirmar el flujo AppImage y la primera ejecución en Arch KDE Plasma
  Wayland desde `~/Applications`, incluido un nuevo acceso del panel. El
  lanzador anterior permanece apuntando a la ruta vieja, pero no bloquea el
  acceso nuevo.
- [x] 1.4 Registrar qué pasos son requeridos para iniciar y cuáles activan una
  integración opcional. GNOME/KDE no se requieren para abrir la app; activan
  atribución de aplicación y atajos globales en Wayland compatible.

## 2. Guías bilingües

- [x] 2.1 Crear el índice de instalación con selector EN/ES.
- [x] 2.2.1 Preparar las instrucciones de macOS para el DMG y Gatekeeper a
  partir de `docs/releases.md`; registrar la validación de instalación limpia
  completada en la tarea 2.2.2.
- [x] 2.2.2 Validar la instalación y el primer inicio del DMG publicado desde
  una instalación de prueba de macOS. El usuario confirmó que ambos pasos
  funcionaron; el procesador y la versión probados no quedaron registrados.
- [x] 2.3.1 Preparar la ruta gráfica principal de Ubuntu GNOME con `.deb`,
  separar Wayland/X11 y no presentar AppImage como alternativa probada.
- [x] 2.3.2 Confirmar la instalación `.deb` en GNOME Wayland y X11. AppImage
  queda sin recomendar para Ubuntu mientras esa ruta no esté validada.
- [x] 2.4.1 Preparar la guía de Arch KDE Plasma Wayland con el AppImage oficial
  y aclarar que ClipVault no publica un paquete Arch nativo ni AUR.
- [x] 2.4.2 Confirmar los pasos de primera ejecución del AppImage publicado en
  Arch KDE Plasma Wayland. El usuario confirmó que inicia desde `~/Applications`
  y que creó un nuevo acceso del panel. El lanzador viejo sigue apuntando a
  otra ubicación y no se pudo quitar; el nuevo acceso funciona.
- [x] 2.5 Añadir la matriz de probado, pendiente y no ofrecido, con equivalencia
  entre inglés y español.
- [x] 2.6 Enlazar desde ambas guías a releases, soporte y ajustes de integración
  correspondientes a la versión actual.
- [x] 2.7 Ampliar las dos filas Linux de “Choose your download” para indicar
  `.deb` en Ubuntu y derivados Debian/Ubuntu compatibles (por ejemplo, Linux
  Mint) y AppImage para la mayoría de distribuciones x86_64, sin agregar filas.
- [x] 2.8 Distinguir la compatibilidad esperada del formato de las distribuciones
  y sesiones verificadas manualmente; mencionar dependencias `.deb` y FUSE.

## 3. Validación

- [x] 3.1 Seguir los pasos desde instalaciones de prueba en macOS, Ubuntu GNOME
  Wayland y X11, y Arch KDE Plasma Wayland. El usuario confirmó las
  instalaciones y primeros inicios; no indicó los procesadores/versiones de
  sistema probados en macOS.
- [x] 3.2 Comprobar en el release `v0.0.20` que existen los artefactos
  macOS Apple Silicon/Intel y Linux x86_64 `.deb`/AppImage.
- [x] 3.3 Revisar que Wayland/X11, paquetes y pasos no verificados estén
  identificados y que no se prometa compatibilidad universal.
- [x] 3.4 Validar OpenSpec en modo estricto para el cambio y el spec paraguas;
  revisar EN/ES en paralelo. Las validaciones finalizaron correctamente.
- [x] 3.5 Validar el alcance de los formatos Linux y la paridad de las dos guías;
  conservar en la matriz únicamente las configuraciones verificadas.
