# Tareas
## Relevamiento y contrato

- [x] 1.1 Leer `project.md`, `AGENTS.md`, este cambio OpenSpec y los artefactos
  vigentes de privacidad, catálogo Linux e iconos de Wayland antes de editar
  código.
- [x] 1.2 Confirmar en el diff de trabajo que el alcance queda limitado al
  orden y la presentación de iconos del selector, preservando los archivos
  archivados manualmente por la persona usuaria.

## Catálogo Linux

- [x] 2.1 Cambiar el orden de `LinuxApplicationCatalog::list()` para usar el
  nombre visible no vacío, con fallback a `identifier`, comparación
  case-insensitive y desempates deterministas.
- [x] 2.2 Actualizar o reemplazar las pruebas que esperan orden por
  identificador.
- [x] 2.3 Agregar pruebas representativas para nombres con mayúsculas, nombres
  ausentes o en blanco, nombres iguales y candidatas de las estrategias
  `WmClass` y `DesktopFileId`.

## Selector de Privacidad e iconos

- [x] 3.1 Cambiar la carga de iconos de candidatas en
  `PrivacyModal.svelte` para usar `sourceAppIconCommand` y el resolver
  existente, manteniendo el namespace `application-icons/`.
- [x] 3.2 Mantener el fallback a la inicial para iconos ausentes o fallidos,
  evitar imágenes rotas y asegurar que un fallo individual no bloquee las
  demás filas.
- [x] 3.3 Verificar el ciclo de vida de Blob URLs y referencias al cerrar,
  destruir o reemplazar el catálogo; cubrir cualquier caso faltante con un
  test frontend acotado.
- [x] 3.4 Verificar que el alta siga enviando únicamente el identificador opaco
  y que no se alteren los comandos ni la semántica de la blacklist.

## Bridge y compatibilidad de plataforma

- [x] 4.1 Revisar la cobertura existente de `clipvault_source_app_icon` y
  agregar solo la prueba de integración faltante que demuestre que el selector
  consume referencias `application-icons/`; no duplicar las pruebas de
  seguridad ya existentes.
- [x] 4.2 Confirmar que el camino Wayland basado en Desktop File ID continúa
  reutilizando el proveedor XDG y la rasterización SVG local, sin nuevas APIs
  de GNOME Shell ni acceso del frontend al filesystem.
- [x] 4.3 Si aparece una contradicción entre namespaces de iconos persistidos y
  de candidatas, pausar la implementación y solicitar una actualización de
  OpenSpec en lugar de copiar o convertir assets implícitamente.

## Verificación

- [x] 5.1 Ejecutar `cargo fmt --all -- --check` y los tests Rust dirigidos del
  catálogo y del bridge.
- [x] 5.2 Ejecutar los checks, tests y build frontend afectados.
- [x] 5.3 Ejecutar `openspec validate linux-blacklist-app-picker-icons-and-order
  --strict --type change`, `git diff --check` y revisar el diff completo.
- [x] 5.4 Confirmar que no se generaron assets, credenciales, logs con secretos
  ni archivos fuera de alcance.
- [ ] 5.5 Cerrar cualquier instancia anterior de ClipVault y probar manualmente
  en X11 y Wayland: orden alfabético, iconos, fallback, selección/cancelación y
  bloqueo de capturas para una aplicación blacklisteada. *(PENDIENTE: la prueba
  manual no puede ejecutarse desde este entorno CLI; queda a cargo de la
  persona operadora con una sesión X11 y otra Wayland).*

## Entrega

- [x] 6.1 Marcar cada tarea como completada inmediatamente después de verificar
  su resultado y reportar checks ejecutados, pruebas manuales y pendientes.
- [x] 6.2 No hacer commit, push ni archivar este cambio sin una instrucción
  explícita posterior.
