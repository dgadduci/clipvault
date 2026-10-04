# Diseño: desktop-tray-menu-actions

## Contexto

`app/tauri/src-tauri/src/tray.rs` construye el menú nativo y traduce sus
identificadores a `TrayAction`. `main.rs` despacha cada acción al adaptador
Tauri. Actualmente `OpenMainWindow` busca la ventana `main` y trata de
mostrarla y enfocarla; las acciones de favoritos, limpieza y ajustes todavía
se notifican como capacidades no disponibles. El frontend ya contiene los
flujos `requestClearHistory`, `clearUnorganizedHistoryCommand`, el modal de
Configuración general y `AboutModal`.

## Decisiones

### Reutilizar y enfocar la ventana principal

`Open ClipVault` opera sobre la ventana Tauri `main`. Si ya existe, la
restaura cuando está minimizada, la muestra y solicita el foco. Activaciones
repetidas actúan sobre esa misma ventana y no crean otras. Si no existe, el
adaptador la recrea a partir de la configuración canónica de ventanas de
Tauri, la muestra y la enfoca. La solicitud de foco se repite en el hilo
principal después del callback del menú nativo para que el cierre del menú no
deje la ventana en segundo plano. La solicitud no cambia de pantalla ni crea
una ventana secundaria.

### Mantener funcionales las decoraciones nativas de Wayland

El ciclo cerrar la ventana principal → ocultarla en la bandeja → volver a
mostrarla debe conservar el comportamiento de sus controles nativos. La
versión bloqueada actualmente usa `tauri-runtime-wry` 2.11.4 con Tao 0.35.3;
esa versión presenta una regresión upstream en los botones de la barra de
título bajo Wayland. Se actualizará el runtime Tauri a 2.12.1, que incorpora
el arreglo de decoraciones de Tao, y se mantendrá el cierre a la bandeja
existente.

No se usará el workaround que alterna `resizable` al recibir foco: depende de
forzar un relayout de GTK, puede alterar el tamaño/estado de la ventana y queda
obsoleto con la corrección upstream disponible. La dependencia se actualiza
por el arreglo de plataforma; no se añade una dependencia nueva.

### Despachar acciones de interfaz como eventos delgados

El callback de menú nativo identifica la acción y la entrega a la ventana
principal. No contiene lógica de SQLite ni implementa por sí mismo los
formularios de interfaz. Para `Settings`, `About` y la confirmación de
`Clear History`, la ventana se muestra y enfoca antes de abrir el modal o
diálogo correspondiente. La UI reutiliza sus componentes actuales.

### Reutilizar la limpieza de historial no organizado

El tray debe llamar al flujo existente de confirmación de Historial y a
`clear_unorganized_history`, no al comando genérico `clear_history`. El
alcance son capturas no favoritas que pertenecen a `Historial` y no tienen
membresía en ninguna colección de usuario. Se conserva toda captura favorita
o asociada a una colección de usuario, incluidas las colecciones vinculadas a
pares. La cancelación no cambia datos; la operación confirmada conserva la
atomicidad, la procedencia local y la recolección segura de assets del flujo
existente.

### Compartir Configuración y Acerca de

`Settings` abre el `GeneralSettingsModal` existente. `About` abre el
`AboutModal` existente, cuyo nombre y versión siguen viniendo de los
diagnósticos canónicos. Los dos puntos de acceso de Acerca de muestran el
mismo modal y no crean una ventana About separada. Al cerrarlo, el foco queda
en la ventana principal.

### Retirar solo la acción de favoritos del menú

La entrada `Favorites` desaparece del menú nativo y deja de tener un
identificador despachable desde ese menú. Los favoritos y sus controles en el
resto de la aplicación no cambian. El resto de las acciones del menú, su
orden relativo salvo la nueva entrada `About`, y su comportamiento se
conservan.

### Localización y plataformas

`About` utiliza una clave de catálogo con valores para `en`, `es`, `pt`, `de`
y `fr`. Los adaptadores de menú conservan la separación actual entre macOS,
Linux X11 y Linux Wayland; el comportamiento solicitado no agrega una API
específica de un solo entorno.

## Riesgos y mitigaciones

- **Una acción nativa llega mientras la ventana está oculta**: mostrar,
  restaurar y enfocar `main` antes de enviar la petición al frontend.
- **Wayland pierde la interacción de los controles de título al ocultar/mostrar**:
  usar la versión corregida del runtime y repetir el ciclo en una sesión
  Wayland real; no intentar compensarlo con cambios de tamaño de la ventana.
- **La confirmación elimina más capturas de las previstas**: mantener la
  operación `clear_unorganized_history` y verificar explícitamente que
  favoritos y membresías de colecciones de usuario se preservan.
- **El tray implementa reglas de negocio**: delegar en comandos y servicios
  existentes; el manejador nativo solo despacha la acción.
- **Acerca de tiene dos entradas y dos implementaciones**: ambos accesos
  abren el único modal y la versión continúa siendo provista por diagnósticos.
- **Una etiqueta falta en un idioma**: comprobar paridad de claves en los
  cinco catálogos.

## Migración

No se requieren migraciones de SQLite ni dependencias nuevas. Se reutilizan
las operaciones y los modales existentes.
