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

#### Scenario: El monitor primario no determina la causa

- GIVEN una sesión Wayland donde `primary_monitor()` devuelve ausencia durante
  `setup`
- WHEN el diagnóstico de ventana está habilitado
- THEN registra `monitor_available = false` como estado normalizado
- AND continúa registrando las etapas de creación y runtime de `main`
- AND no infiere que el toplevel sea invisible únicamente por esa ausencia

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

### Requirement: El handshake Wayland no bloquea el setup del shell

El adaptador nativo Wayland MUST tener un plazo acotado durante su handshake
inicial. El vencimiento MUST completar como no disponible y MUST permitir que
el `setup` de Tauri continúe; no debe impedir que la ventana `main` llegue al
runtime.

#### Scenario: El compositor acepta el socket pero no responde el registro

- GIVEN una sesión Wayland donde el socket del compositor acepta la conexión
  pero no responde el intercambio de registro inicial
- WHEN `build_state` construye el adaptador de aplicación activa
- THEN el handshake termina como no disponible dentro de su plazo acotado
- AND el hilo de I/O puede finalizar antes de que el shell haga `join`
- AND Tauri continúa hasta `setup_completed` y `runtime_ready`
- AND el shell no solicita foco, posición, tamaño ni visibilidad como
  recuperación especulativa
