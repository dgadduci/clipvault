# Tareas: configuración de atajos de teclado

## 1. Inventario y contrato

- [x] 1.1 Confirmar el inventario de acciones de `proposal.md` contra los
  matchers, hints visibles, atributos accesibles, diálogos y hotkeys nativos.
- [x] 1.2 Separar los atajos configurables con modificadores de la navegación
  estándar; conservar el contrato de flechas, Home/End, Enter, Escape, F2 y
  Tab/Shift+Tab, y registrar `Shift+Enter` como acción configurable de
  QuickVault.
- [x] 1.3 Definir diez IDs estables, defaults por host, claves admitidas y reglas
  de colisión entre acciones locales y globales.

## 2. Modelo, validación y persistencia

- [x] 2.1 Añadir al core el registro tipado de acciones y bindings sin
  introducir dependencia de Tauri o del navegador.
- [x] 2.2 Persistir los diez bindings en `app_settings` como datos locales,
  conservar perfiles existentes y evitar una migración de esquema SQLite.
- [x] 2.3 Leer `quick_paste_hotkey` como binding heredado de `open_quick_paste`
  y aplicar los defaults de plataforma ante ausencia o valor inválido.
- [x] 2.4 Validar una tecla con modificadores, teclas soportadas,
  combinaciones duplicadas y colisiones reportadas por el sistema.
- [x] 2.5 Mantener el binding anterior activo y persistido ante cualquier fallo
  de validación, registro o escritura.

## 3. Runtime multiplataforma

- [x] 3.1 Extender el servicio `HotkeyManager` para reemplazar un binding por
  ID sin desregistrar las otras acciones globales.
- [x] 3.2 Implementar actualización en caliente de QuickVault y alternar
  captura en macOS y Linux/X11, ampliando el set de teclas del adaptador.
- [x] 3.3 Extender el bridge GNOME Wayland para aceptar y confirmar cambios de
  ambos aceleradores sin transportar contenido del portapapeles.
- [x] 3.4 Extender KWin Wayland para registrar QuickVault además de alternar
  captura y reemplazar cualquiera de los dos bindings durante la sesión.
- [x] 3.5 Actualizar los matchers de ventana principal, QuickVault y diálogos
  desde el registro activo, sin listeners duplicados ni recargas de WebView.
- [x] 3.6 Emitir un evento acotado de `keyboard-shortcuts-changed` para
  sincronizar ambas ventanas y reconstruir el menú nativo de bandeja.

## 4. Configuración y presentación

- [x] 4.1 Añadir a Configuración general la opción Atajos de teclado y abrir
  un modal dedicado con una fila por acción.
- [x] 4.2 Implementar el modo de captura de teclado que se activa por fila,
  presenta la combinación propuesta y descarta capturas incompletas al cerrar.
- [x] 4.3 Aplicar inmediatamente bindings aceptados y mostrar errores
  localizados manteniendo visible el valor anterior.
- [x] 4.4 Derivar los hints, tooltips y atributos `aria-keyshortcuts` del
  mismo registro activo en la ventana principal, QuickVault y bandeja.
- [x] 4.5 Añadir y validar cada texto nuevo en los catálogos en, es, pt, de y
  fr.
- [x] 4.6 Actualizar la fila del modal inmediatamente al aplicar un binding,
  sin cerrar ni volver a abrir el diálogo.
- [x] 4.7 Reemplazar `save_text` por abrir/editar la nota seleccionada y añadir
  navegación a Historial y creación de captura de texto.
- [x] 4.8 Quitar el hint visible junto al nombre de Historial y conservar
  `aria-keyshortcuts` con el binding activo.

## 5. Pruebas y verificación

- [x] 5.1 Añadir pruebas puras de IDs, defaults, normalización, colisiones,
  teclado grabado y fallbacks seguros.
- [ ] 5.2 Probar persistencia, reinicio, binding heredado y rollback al fallar
  el guardado.
- [ ] 5.3 Probar registro, reemplazo, conflicto y restauración anterior en
  adaptadores nativos, GNOME y KWin.
- [ ] 5.4 Probar que las dos ventanas, los diálogos y el tray actualizan
  labels y handlers durante la sesión.
- [x] 5.5 Ejecutar regresiones de teclado, cards y drag and drop si los
  matchers o componentes de cards/listeners se modifican.
- [x] 5.6 Ejecutar checks/builds frontend y Tauri, tests relevantes, validador
  de catálogos, `cargo fmt`, `git diff --check` y validación OpenSpec.
- [ ] 5.7 Verificar manualmente en macOS, Ubuntu GNOME Wayland, Ubuntu/X11 y
  KDE Plasma Wayland: editar cada binding, probar activación inmediata,
  reinicio, conflicto, rollback y etiquetas de bandeja. Repetir también los
  flujos de nota seleccionada, navegación a Historial y creación de captura.
- [x] 5.8 Añadir y ejecutar regresiones para refresco inmediato de la fila y
  los tres nuevos atajos de la ventana principal.

Las verificaciones integradas de reemplazo/rollback (5.2–5.4) y la prueba
manual multiplataforma (5.7) siguen pendientes; el entorno actual solo permite
ejecutar pruebas automatizadas y compilaciones locales.
