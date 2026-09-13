# Tareas: selector visual de aplicaciones Linux para blacklist

- [x] 1.1 Leer `project.md`, `AGENTS.md`, las specs de privacidad y los
  adaptadores activos de X11, Wayland y GNOME.
- [x] 1.2 Auditar qué identificadores publica cada sesión y documentar sus
  correspondencias posibles con entradas `.desktop`.
- [x] 1.3 Definir la matriz de asociación estable, ambigua y no soportada.
- [x] 1.4 Actualizar proposal, design y specs antes de implementar.
- [x] 2.0 Nuevo: introducir el resolver tipado
  [`clipvault_core::linux_picker`] que decide el backend del picker
  a partir del estado real del runtime (probe activo + GNOME
  integration) y expone `LinuxPickerBackend` (`clipvault-platform`).
- [x] 2.1 Implementar el adaptador Linux detrás del trait de application
  picker usando un catálogo testeable con un método `find(...)` para
  revalidar el identificador.
- [x] 2.2 Mantener el fallback de ingreso manual y errores tipados.
- [x] 2.3 Cubrir X11, Wayland/XWayland, cancelación y sesiones no soportadas.
- [x] 2.4 Ejecutar los tests relevantes y las regresiones de privacidad.
- [x] 2.5 Nuevo: el comando `add` revalida contra el catálogo vivo y
  rechaza identificadores arbitrarios, sesiones `Unsupported` o
  catálogos vacíos.
- [x] 2.6 Nuevo: dedup por identificador normalizado en minúsculas con
  precedencia XDG y `display_name`/`icon_ref` consistentes con la
  entrada `.desktop` seleccionada.
- [x] 2.7 Nuevo: el modal del settings panel resuelve los iconos a
  través del `iconResolver` existente; fallback a la inicial cuando
  no se pueda resolver.
- [x] 3.0 Reescribir tests contradictorios: `catalog_unsupported...` debe
  verificar `Unsupported`, `add_preserves_unknown_identifier...`
  debe verificar rechazo.
- [x] 3.1 Tests adicionales: X11 soportado, XWayland bajo rama X11,
  Wayland nativo con backend real, Wayland sin backend estable,
  GNOME aceptado y operativo, GNOME aceptado pero ausente/inactivo,
  catálogo vacío, cancelación y rechazo de identificadores no
  presentes en el catálogo.
- [x] 3.2 Verificar manualmente la selección visual en las sesiones
  soportadas. Validado el 2026-09-13 en X11 y Wayland con el bundle frontend
  y el binario Tauri del mismo checkout: el catálogo mostró la lista de
  aplicaciones disponibles y las aplicaciones añadidas a la blacklist no
  generaron nuevas capturas.
- [x] 3.3 Revisar diff, validar OpenSpec y preparar el handoff.
- [x] 3.4 Corrección: `linux_picker_session_state()` lee el backend del
  probe activo (`CachedActiveApplication::name()`) en vez del
  `ActiveAppDiagnosticsState.backend` capturado en el bootstrap, así
  `swap_active_app_probe()` se refleja sin reiniciar la captura.
- [x] 3.5 Corrección: el resolver deja de exigir `cache_populated` para
  las ramas X11, XWayland y Wayland nativo, y para la rama GNOME
  sólo admite `Connected` / `Identified` / `NoActiveApplication`.
  `ActivationPending` (y los demás estados no operativos) caen a
  `Unsupported`.
- [x] 3.6 Corrección: la rama `Cancelled => Added { entry: empty }` de
  los comandos `clipvault_ignored_app_linux_add*` se reemplaza por
  `unreachable!()` para que ningún `Cancelled` pueda filtrarse como
  fila fantasma.
- [x] 3.7 Wiring test: `swap_active_app_probe()` con un probe GNOME en
  contexto X11 inicial + consentimiento aceptado + estado conectado
  cambia `linux_picker_backend()` a `GnomeShellExtension`; swap a
  `wayland_wlr_foreign_toplevel` cambia a `WaylandNative`.
- [x] 3.8 Corregir el gate de compilación de la rama Wayland en
  `app/tauri/src-tauri/src/bootstrap.rs`: debe depender de
  `target_os = "linux"` y de la feature efectiva de
  `clipvault-platform`, no de una feature homónima no activada por
  defecto en `clipvault-app`.
- [x] 3.9 Agregar/verificar una regresión estructural que confirme el
  wiring nativo Wayland en la build Linux normal y ejecutar el check del
  shell con las features efectivas; repetir después la prueba manual del
  selector en Ubuntu.
- [x] 3.10 Repetir la prueba manual X11 y Wayland con el bundle y binario del
  mismo checkout. Validado el 2026-09-13: en ambas sesiones aparece la lista
  de aplicaciones para agregarlas a la blacklist y una aplicación incluida
  en la blacklist no agrega capturas, como se esperaba. El cableado frontend
  quedó implementado en el cambio separado
  `linux-blacklist-app-picker-ui-wiring`; el bundle regenerado contiene
  `linux-picker-modal` y `ignoredAppLinuxCatalogCommand`.

  La brecha de wiring observada quedó resuelta en
  `linux-blacklist-app-picker-ui-wiring`: el modal Linux vive en la superficie
  activa `PrivacyModal.svelte`, y el bundle actualizado contiene sus símbolos.
  La verificación manual real en ambas sesiones gráficas quedó cerrada con
  el resultado registrado arriba.
- [x] 3.11 Corrección de runtime GNOME: mantener un estado técnico transitorio
  en memoria, separado del valor persistido, y sincronizarlo desde el
  `SharedGnomeSnapshot` vivo antes de los comandos Linux `catalog` y `add`.
  No escribir SQLite por las transiciones de foco. La matriz debe aceptar
  `NoActiveApplication` cuando el bridge está conectado y seguir rechazando
  `ActivationPending`.
- [x] 3.12 Agregar regresiones que reproduzcan el fallo manual: consentimiento
  aceptado, probe `gnome_shell_extension`, valor persistido
  `activation_pending` y snapshot vivo `no_active_application`. El catálogo
  debe abrirse con estrategia `desktop_file_id`; si no hay snapshot vivo,
  debe permanecer en el fallback conservador.
- [ ] 3.13 Repetir la prueba manual en Ubuntu GNOME Wayland después de 3.11:
  abrir Privacidad (donde ClipVault toma el foco), confirmar
  `no_active_application` y comprobar que el catálogo visual aparece. Añadir
  y eliminar un candidato, y confirmar que no se modifica la blacklist al
  cancelar ni cuando el estado sea `activation_pending`.
  - La prueba del 2026-09-13 confirma la apertura del catálogo y que una
    aplicación incluida en la blacklist no genera capturas; aún no cubre
    eliminación, cancelación ni el caso `activation_pending`.

## Garantías contractuales

- El ingreso manual sigue disponible bajo `clipvault_ignored_apps_add`.
- No se inventan identificadores: el selector sólo ofrece candidatos
  que el catálogo puede reproducir deterministamente.
- No se leen títulos, PID, `/proc`, contenido del clipboard ni payloads
  en ningún punto del flujo del picker.
- Cancelación, sesión `Unsupported`, catálogo vacío o identificador
  no presente en el catálogo: la blacklist y los assets permanecen
  inalterados.
- El comando `add` usa siempre la metadata del catálogo, nunca la del
  frontend.
