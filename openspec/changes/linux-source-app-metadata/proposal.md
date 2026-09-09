## Por qué

En macOS ClipVault persiste el nombre visible y el icono de la aplicación
desde la que se obtuvo una captura. En Linux el probe X11 ya puede obtener
`WM_CLASS`, pero el bootstrap instala `NoopApplicationMetadataProvider` para
cualquier host que no sea macOS. Por eso las filas pueden conservar un
identificador de origen, pero no reciben `source_app_name` ni
`source_app_icon_ref`.

En Ubuntu GNOME, además, la sesión habitual es Wayland. La variable
`DISPLAY` puede indicar que XWayland está disponible, pero no convierte una
ventana Wayland nativa en una ventana X11. ClipVault debe aprovechar XWayland
cuando exista una ventana X11 detectable y declarar explícitamente la
limitación cuando la aplicación activa sea Wayland nativa.

## Qué cambia

- Agregar un proveedor Linux de metadata de aplicación que implemente el
  trait `ApplicationMetadataProvider` existente.
- Resolver identificadores `WM_CLASS` contra archivos `.desktop` de los
  directorios de aplicaciones del usuario y del sistema.
- Obtener el nombre localizado y resolver el icono declarado por el archivo
  `.desktop` sin ejecutar comandos ni abrir la aplicación.
- Persistir el icono bajo el namespace existente
  `<data_dir>/assets/application-icons/` mediante una escritura segura y
  devolver solamente una referencia relativa controlada.
- Usar el adapter X11 existente también como fallback XWayland cuando la
  sesión Wayland exponga `DISPLAY` y exista una ventana X11 activa.
- Exponer en diagnósticos qué backend se utilizó y distinguir X11/XWayland de
  Wayland nativo sin filtrar rutas, contenido ni identificadores sensibles en
  logs o eventos.
- Reutilizar la hidratación, el bridge y la presentación de iconos/nombres ya
  usados por las cards de macOS.
- Agregar backfill acotado para filas existentes con `source_app` pero sin
  metadata, reutilizando el flujo actual y sin reescribir las capturas.

## No objetivos

- No implementar una integración genérica o una extensión específica de GNOME
  para consultar la ventana activa Wayland nativa.
- No afirmar que XWayland identifica aplicaciones Wayland nativas: sólo cubre
  ventanas que realmente estén publicadas en X11.
- No modificar el algoritmo del blacklist ni convertir un origen desconocido
  en una coincidencia inventada.
- No cambiar captura, historial, búsqueda, filtros, pegado, imágenes, rich
  text, tags, colecciones, favoritos o drag-and-drop.
- No modificar adapters de macOS ni el cambio `linux-x11-compatibility`.
- No añadir red, telemetría, embeddings, procesos externos ni dependencias
  innecesarias.

## Capacidades afectadas

### Capacidades modificadas

- `desktop-platform-integration`: Linux X11 y XWayland pueden proporcionar
  metadata de aplicación; Wayland nativo sigue reportando la limitación de
  forma tipada.
- `clipboard-history-cards`: las capturas Linux pueden persistir el nombre e
  icono de la aplicación origen mediante los campos existentes.

## Impacto esperado

- `crates/clipvault-platform`: proveedor Linux de metadata, resolución de
  `.desktop`, resolución segura de iconos y, si hace falta, clasificación
  explícita del backend XWayland.
- `crates/clipvault-core`: sólo cambios de integración mínimos para backfill,
  diagnóstico o propagación de metadata; no se debe duplicar la lógica de
  persistencia.
- `app/tauri/src-tauri`: selección del proveedor Linux y del fallback XWayland
  en el bootstrap; comandos existentes conservados.
- Frontend: reutilización del modelo, bridge y componentes existentes; sólo
  cambios de diagnóstico o etiquetas si son necesarios.
- Tests unitarios, integración y verificación manual en Ubuntu X11 y GNOME
  Wayland.
