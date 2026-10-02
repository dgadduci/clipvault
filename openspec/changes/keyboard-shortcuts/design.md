# Diseño: configuración de atajos de teclado

## Estado actual

- `Cmd/Ctrl+F` enfoca la búsqueda de la ventana principal desde
  `App.svelte` y `searchShortcut.ts`.
- `Cmd/Ctrl+K` enfoca la búsqueda de QuickVault desde `QuickPaste.svelte`.
- `Cmd/Ctrl+Enter` previsualiza la selección en ambas ventanas, con el helper
  compartido `clipboardPreview.ts`.
- `Cmd/Ctrl+E` edita la captura textual seleccionada en ambas ventanas, con
  el helper compartido `editTextShortcut.ts`.
- El botón de la barra superior abre el diálogo existente para crear una
  captura de texto; las tarjetas permiten crear o editar la nota de cada
  captura. La nota seleccionada no tiene actualmente un atajo propio.
- `Shift+Enter` en QuickVault copia texto sin formato cuando la captura tiene
  una representación rica; `Enter` mantiene la selección de representación
  predeterminada.
- QuickVault se abre con `Cmd+Shift+V` en macOS y `Ctrl+Shift+V` en Linux.
  `register_default_hotkey` elige hoy estos defaults en `bootstrap.rs`.
- Alternar captura usa `Cmd+Alt+Shift+B` en macOS y
  `Ctrl+Alt+Shift+B` en Linux. El atajo se muestra en Configuración general y
  en la bandeja.
- `Settings.quick_paste_hotkey` ya se persiste en `app_settings`; el modal
  actual solo informa el atajo. El registro de arranque usa defaults y la
  extensión de GNOME y el script KWin tienen aceleradores fijos.
- En Linux Wayland el `HotkeyManager` nativo es `NoopHotkeyManager`. GNOME
  registra abrir QuickVault y alternar captura en su extensión. KWin registra
  alternar captura, pero hoy no abre QuickVault desde un hotkey global.

Las teclas simples de listas, cards, menús, filtros y diálogos son navegación
o activación contextual. Permanecen fuera de la reasignación de atajos con
modificadores.

## Modelo de acciones y valores

Mantener un registro único de acciones con IDs estables y un binding
normalizado. Las diez acciones de `proposal.md` cubren los matchers de
frontend y los dos hotkeys globales. `preview_selected` se comparte entre las
ventanas principal y QuickVault; `edit_selected_text` también. `open_entry_note`,
`open_history` y `create_text_capture` actúan en la ventana principal.
`open_entry_note` requiere una captura seleccionada y usa el diálogo existente
de la tarjeta, que carga la nota actual si ya existe. `open_history` selecciona
la colección de sistema Historial y actualiza su lista. `create_text_capture`
abre el mismo diálogo que el botón de la barra superior. `copy_plain_text`
controla `Shift+Enter` solo en la lista de QuickVault; `Enter` conserva la
acción predeterminada. La fila configurable `save_text` se elimina; los
diálogos conservan sus botones explícitos para guardar.

Un binding contiene una tecla principal y uno o más modificadores (`primary`,
`shift`, `alt` y `meta`). `primary` se presenta como Command en macOS y
Control en Linux. Los defaults se resuelven para la plataforma del equipo
cuando no hay una preferencia guardada. El usuario puede grabar una tecla más
modificadores; no puede grabar una tecla suelta ni una secuencia.

La configuración es local al perfil del equipo. Reutilizar `app_settings`,
sin migración de esquema SQLite. Al leer datos antiguos, convertir el valor
válido de `quick_paste_hotkey` en el binding `open_quick_paste`; si falta o es
inválido, usar el default de la plataforma. No perder el binding anterior si
una nueva combinación no se puede activar.

## Flujo de configuración

1. Configuración general muestra la acción Atajos de teclado. Al activarla,
   abre un modal dedicado con las diez acciones y sus combinaciones actuales.
   La opción existente de atajo en el menú de la ventana principal abre ese
   mismo modal, en lugar de conservar una vista informativa separada.
2. El usuario activa **Cambiar** en una fila. El modal entra en modo de
   captura y escucha teclado solo mientras está abierto y enfocado.
3. Al completar una combinación válida, la UI muestra el binding propuesto y
   consulta la validación del core y de la plataforma.
4. Si es válido, el runtime reemplaza el binding. Solo después de confirmar
   que la activación tuvo éxito se persiste la preferencia y se actualizan el
   store compartido y la fila visible en el modal.
5. Si la persistencia falla luego de registrar, se restaura el binding
   anterior. Si la validación o el registro falla, el binding anterior sigue
   activo y guardado, y el modal muestra un error localizado.
6. Al cerrar el modal mientras se captura una combinación, se descarta la
   captura incompleta.

La lista también identifica los atajos cuya disponibilidad depende de una
integración de plataforma y muestra el estado de registro o conflicto. No
presenta un atajo como activo si el sistema no confirmó su registro.

## Validación y conflictos

- El capturador acepta un solo chord con al menos un modificador y una tecla
  principal soportada por todos los backends objetivo.
- La validación rechaza teclas modificadoras solas, combinaciones incompletas,
  combinaciones no registrables y colisiones de ClipVault en contextos que
  podrían ejecutarse simultáneamente.
- Las acciones en contextos excluyentes pueden compartir un chord. Las tres
  nuevas acciones de la ventana principal colisionan entre sí y con los otros
  atajos principales; la validación las trata como un mismo contexto.
- Ningún hotkey global puede colisionar con un atajo de ventana de ClipVault:
  el global podría consumir la pulsación antes de que llegue a la ventana.
- Si el sistema informa que otra aplicación posee una combinación, no se
  guarda ni se reemplaza el valor anterior.

No guardar la secuencia de teclas de captura, contenido de campos, ni eventos
de teclado en logs. Los listeners de captura se eliminan al salir del modo de
grabación o desmontar el modal.

## Arquitectura y aplicación en caliente

```text
GeneralSettingsModal
  → KeyboardShortcutsModal (captura y presenta bindings)
  → comando Tauri delgado
  → servicio de configuración en clipvault-core
      → validación / persistencia local
      → adaptadores de hotkey para acciones globales
  → evento de shortcuts-changed
  → frontend principal + QuickVault + etiqueta de bandeja
```

El core mantiene IDs, defaults, normalización, validación y almacenamiento sin
depender de Tauri ni del navegador. El frontend consume el mismo registro para
matchers, hints visibles y `aria-keyshortcuts`; no conserva tablas de
combinaciones paralelas.

Los adaptadores de plataforma reemplazan bindings por ID y devuelven
`registered`, `conflict`, `unsupported` o `failed`. Evitar
`unregister_all` para una actualización individual, porque borraría ambos
hotkeys globales. La sustitución mantiene disponible el binding anterior
hasta que el nuevo se registre y la persistencia termine.

- **macOS:** adaptar el `HotkeyManager` global actual y extender el conjunto
  de claves admitidas.
- **X11:** conservar el worker y la semántica de passive/raw grab; ampliar el
  keymap y reemplazar solo la acción solicitada.
- **GNOME Wayland:** extender el canal local de la extensión para recibir una
  solicitud tipada de reconfiguración y confirmar el registro de ambos
  aceleradores. El canal no transporta contenido del portapapeles.
- **KDE Plasma Wayland:** extender el script y el bridge KWin para registrar
  abrir QuickVault además de alternar captura y reconfigurar ambos sin
  reiniciar ClipVault. La actualización preserva el bridge de aplicación
  activa y el callback de alternar captura.
- **Atajos locales:** los matchers consultan el store reactivo de bindings;
  el evento `clipvault://keyboard-shortcuts-changed` actualiza las dos
  ventanas y los hints sin recargar WebViews. QuickVault consulta el binding
  `copy_plain_text` solo en la lista activa. La ventana principal dirige
  `open_entry_note` por el rail hasta la tarjeta seleccionada y reutiliza su
  modal, `open_history` por el selector de colección y `create_text_capture`
  por el callback que comparte con el botón superior.
- **Historial:** la fila no muestra un hint visible con el binding junto al
  nombre; conserva `aria-keyshortcuts` con el valor accesible del registro
  activo.
- **Bandeja:** reconstruir el menú con el binding actual de alternar captura
  cuando cambie.

El comando Tauri no incluye lógica de plataforma o parsing extensa: delega al
servicio core y al adaptador adecuado. No se agrega una dependencia.

## Verificación requerida

- Pruebas puras para la tabla de diez acciones, defaults por plataforma,
  normalización, validación y detección de conflictos.
- Pruebas de persistencia/reinicio que cubran los diez bindings y la
  compatibilidad con `quick_paste_hotkey`.
- Pruebas de registro, reemplazo y rollback para el `HotkeyManager`, X11,
  GNOME y KWin usando adaptadores determinísticos.
- Pruebas frontend de captura de combinaciones, labels accesibles, fallos,
  listeners, refresco inmediato de la fila, y ejecución de los atajos de
  nota, Historial y captura de texto.
- `npm run locales:check`, `npm run check`, `npm run build`, tests frontend
  dirigidos, tests Rust relevantes, `cargo fmt --all -- --check`, validación
  OpenSpec y revisión del diff.
- Verificación manual de cambio instantáneo y persistencia en macOS, Ubuntu
  GNOME Wayland, Ubuntu/X11 y KDE Plasma Wayland; probar también una
  combinación en conflicto y confirmar que queda activa la anterior.
