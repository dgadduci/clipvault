# Runtime de ventana Ubuntu Wayland

## ADDED Requirements

### Requirement: Baseline reproducible del desktop Wayland

ClipVault MUST evaluar el arranque de Ubuntu GNOME Wayland desde una rama
basada en `main`, con árbol limpio y sin procesos anteriores del mismo
workspace. La prueba inicial MUST usar el comando normal, sin forzar backend
GTK ni features opcionales de GNOME.

#### Scenario: Inicio limpio en GNOME Wayland

- GIVEN una sesión Ubuntu GNOME Wayland y una rama basada en `main`
- AND no hay instancia previa de ClipVault ni Vite del workspace
- WHEN el usuario inicia `CARGO_BUILD_JOBS=1 cargo tauri dev` desde `app/tauri`
- THEN verifica visualmente la ventana principal
- AND no usa `wmctrl` para inferir la ausencia de una ventana Wayland nativa

### Requirement: Diagnóstico de ciclo de vida metadata-only

Cuando está habilitado explícitamente, ClipVault MUST emitir sólo etapas
normalizadas de su ventana `main` y del ciclo Tauri necesario para identificar
la primera transición fallida.

#### Scenario: Traza sin datos sensibles

- GIVEN el diagnóstico de ventana está habilitado
- WHEN el shell crea o recibe un evento de la ventana `main`
- THEN la traza contiene sólo etapas y estados normalizados
- AND no incluye clipboard, títulos, PID, rutas, hashes, assets,
  identificadores externos de ventana ni variables de entorno

### Requirement: Corrección guiada por evidencia

El shell MUST NOT introducir solicitudes de ventana antes del mapeo sin que la
traza Ubuntu demuestre que esa solicitud es necesaria y que el momento es
válido.

#### Scenario: Monitor primario ausente durante setup

- GIVEN que `primary_monitor()` no está disponible durante `setup`
- WHEN se prepara la ventana inicial
- THEN ClipVault conserva la configuración nativa de Tauri
- AND no cambia monitor ni solicita tamaño, posición, foco o visibilidad antes
  de que el diagnóstico pruebe que corresponde

### Requirement: No regresiones de producto

Una corrección de ventana Wayland MUST preservar macOS, Linux X11, Quick Paste,
captura, pegado, imágenes persistidas, tags, colecciones, favoritos, búsqueda y
drag and drop.

#### Scenario: Cambio acotado al shell

- GIVEN una corrección del ciclo de vida Wayland
- WHEN se revisa el diff
- THEN no cambia SQLite, `EntryRecord`, assets de clipboard ni el controlador
  de drag and drop
