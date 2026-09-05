## Why

ClipVault necesita ofrecer al usuario controles locales y verificables para
decidir qué se captura, qué se conserva y qué se excluye del historial. Hoy la
configuración y la blacklist de aplicaciones son requisitos documentados del
MVP, pero todavía no están implementados, y los logs pueden contener contenido
del portapapeles o valores sensibles sin ninguna redactoría determinística.

Este cambio introduce los tres controles básicos del MVP — aplicaciones
ignoradas, ajustes locales de retención/atajos/hotkey y diagnósticos
redactados — sin telemetría, sin red y sin exponer el contenido capturado.

## What Changes

- Persistir localmente una lista configurable de identificadores de aplicación
  ignorados, editable desde la ventana principal, con API para añadir,
  eliminar y consultar.
- Permitir que el pipeline de captura descarte de forma determinística los
  eventos cuyo origen pertenece a la blacklist, sin registrar el contenido
  descartado ni etiquetarlo como histórico.
- Exponer ajustes locales para la política de retención del historial, la
  lista de aplicaciones ignoradas y el atajo de quick-paste, con validación y
  preservación del último valor válido cuando se ingresan valores no
  soportados.
- Implementar un redactor determinístico para logs que omita o enmascare el
  contenido completo del portapapeles, contraseñas, tokens, claves privadas y
  otros valores sensibles detectados antes de escribirlos en `tracing`.
- Garantizar que los tests del core y los flujos manuales de la GUI no
  registren contenido completo del portapapeles; los logs sólo deben
  contener contexto y categorías de error.
- Mantener la lógica de privacidad aislada detrás de traits en
  `clipvault-platform` y módulos puros en `clipvault-core` / `clipvault-db`,
  de modo que pueda ejercitarse en tests sin GUI ni clipboard real.

## Capabilities

### New Capabilities

- `privacy-settings`: controles locales de privacidad, blacklist de
  aplicaciones, ajustes mínimos del MVP y redacción determinística de logs.

### Modified Capabilities

- `clipboard-text-history`: el motor de captura y dedupe debe consultar la
  blacklist de aplicaciones ignoradas antes de almacenar una entrada y debe
  respetar la política de retención configurada al expirar o podar
  elementos.

## Impact

- `crates/clipvault-db`: nueva tabla `ignored_apps` con migración explícita y
  tabla `settings` (clave/valor) para ajustes del MVP; nuevas consultas
  idempotentes y reversibles para blacklist y ajustes.
- `crates/clipvault-core`: nuevos tipos `IgnoredApp`, `IgnoredAppId`, ajustes
  (`RetentionPolicy`, `IgnoredAppsSetting`, `QuickPasteHotkey`), validador de
  ajustes, pipeline de captura con hook de blacklist y módulo `redact` para
  mensajes de log.
- `crates/clipvault-platform`: se reutiliza el trait `ActiveApplicationProbe`
  existente (con adaptadores macOS `NSWorkspace` y X11 `EWMH`) para resolver
  el identificador de la aplicación origen. No se introduce un trait paralelo
  `ApplicationBlacklistMatcher`: el matching contra la blacklist vive en el
  core como un `CoreBlacklistMatcher` que envuelve el probe.
- `app/tauri/src-tauri`: comandos delgados `settings_get`, `settings_set`,
  `ignored_apps_list/add/remove`, `redact_preview` (sólo cuando aplique) y
  apertura del panel de ajustes en la ventana principal.
- `app/tauri/frontend`: sección de privacidad y aplicaciones ignoradas en la
  ventana principal con formularios validados, vista de ajustes efectivos y
  advertencias visibles cuando un valor es rechazado.
- `tests/`: pruebas unitarias para el validador, el redactor, el pipeline de
  captura con blacklist y el repositorio de ajustes; pruebas de integración
  para la migración SQLite y para la combinación blacklist + redacción.
