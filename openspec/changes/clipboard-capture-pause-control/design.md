# Design: clipboard-capture-pause-control

## Fronteras de arquitectura

```text
Atajo global ───────────────┐
Configuración general ──────┼──> comando Tauri delgado
Menú tray/menu bar ─────────┘          │
                                       v
                           SettingsService / AppContext
                              │                │
                              v                v
                     SQLite app_settings   CaptureWatcher
                              │                │
                              └──── estado efectivo ────┘
```

El estado persistido es la fuente única de verdad. El shell puede adaptar las
acciones nativas y los eventos globales, pero la regla de captura y la
persistencia permanecen en Rust. El frontend no accede directamente a SQLite.

## Estado y ciclo de vida

- Agregar un valor booleano a los ajustes locales de captura. Su valor por
  defecto es `true`, para mantener el comportamiento actual en instalaciones
  nuevas y en bases existentes que todavía no contengan la clave.
- Cargar el estado efectivo antes de que el loop de captura pueda persistir un
  payload al iniciar la aplicación.
- Pausar actualiza el ajuste local y la puerta compartida usada por el watcher.
  Cuando la operación termina correctamente, ningún trabajo en curso puede
  guardar una captura local iniciada antes de la pausa.
- Mientras está pausado, el watcher no entrega payloads locales al pipeline de
  persistencia. Al reanudar, el primer tick establece un baseline y no guarda
  el contenido que ya estaba en el portapapeles; sólo los cambios posteriores
  vuelven a capturarse.
- Si falla la persistencia del ajuste, se conserva el último estado efectivo y
  las interfaces informan el error en vez de mostrar un cambio no aplicado.

## Atajo global

Registrar `Cmd+Option+Shift+B` en macOS y `Ctrl+Alt+Shift+B` en Linux, una
combinación menos habitual que evita los atajos comunes de búsqueda y edición.
Usar el manager de hotkeys existente en macOS y X11; en Wayland, usar el
registro nativo de atajos de las integraciones GNOME Shell y KDE KWin ya
incluidas. El atajo debe permanecer registrado aunque la captura esté pausada,
para que pueda reanudarse sin abrir la ventana. Configuración general debe
indicar que una sesión Wayland requiere habilitar la integración de su
escritorio. Si el compositor no ofrece una ruta compatible o hay un conflicto,
los controles de Configuración general y tray permanecen utilizables.

## Superficies de control

- Configuración general presenta una fila `Captura del portapapeles` con el
  estado activo/pausado, un control de alternancia y la etiqueta del atajo.
- El menú del tray/menu bar incluye una acción dinámica `Pausar capturas` o
  `Reanudar capturas`, con el atajo visible.
- Los tres puntos de entrada llaman la misma operación idempotente de estado;
  no implementan toggles independientes.
- Después de un cambio confirmado, Configuración general y el menú nativo
  reciben/leen el mismo estado persistido. La acción nativa reconstruye su
  etiqueta cuando el estado cambia desde otra superficie.

## Compatibilidad y privacidad

El cambio cubre la lectura local del portapapeles en macOS y Linux, incluyendo
X11 y Wayland, y los payloads admitidos por el pipeline compartido. No altera
el historial existente, ni registra contenido durante la pausa, ni incorpora
contenido de portapapeles a ajustes, eventos, logs o diagnósticos. La pausa de
la captura local no detiene la recepción o importación de capturas enviadas por
equipos vinculados; son flujos independientes.

No se agregan dependencias. Los adaptadores de plataforma y las interacciones
con tray/hotkeys siguen fuera del core; las reglas del estado capturado y su
persistencia permanecen testeables sin clipboard real ni sesión gráfica.

## Verificación prevista

- Tests del core/settings para el valor predeterminado, alternancia,
  persistencia, reinicio y fallo al guardar.
- Tests del watcher para no almacenar durante la pausa, bloquear escrituras en
  curso al confirmar la pausa y evitar backfill en el primer tick al reanudar.
- Tests de integración para que el shortcut, Configuración general y tray
  invoquen una única transición y reciban el estado actualizado.
- Checks/builds de Tauri/frontend y regresiones del registro de hotkeys,
  puentes nativos Wayland y menú tray, sin tocar cards ni sus listeners.
- Pruebas manuales en macOS, Ubuntu/X11, Ubuntu/Wayland y Arch KDE/Wayland:
  pausar y reanudar desde cada superficie; copiar contenido en pausa;
  comprobar que no se registra entonces ni al reanudar; copiar de nuevo después
  de reanudar; reiniciar en ambos estados y verificar persistencia y atajos.
