## Context

ClipVault ya captura texto, lo deduplica, lo persiste en SQLite con migraciones
explícitas y expone capabilities de plataforma tipadas para pegado sintético y
reintentos. El MVP v0.1 todavía debe incorporar tres controles locales:

- una blacklist de aplicaciones ignoradas configurable por el usuario,
- un set mínimo de ajustes locales (retención, blacklist, hotkey de
  quick-paste) persistidos y validados, y
- redactoría determinística para que los logs no registren contenido del
  portapapeles ni posibles secretos.

Estos tres controles viven hoy sólo como requirements (`privacy-settings`) y
como sección de "Privacidad y seguridad" en `project.md`. El pipeline de
captura todavía no consulta blacklist alguna, no existe una tabla de
settings, y `tracing-subscriber` se configura sin un redactor que enmascare
valores sensibles.

El estado actual separa el dominio en crates (`clipvault-core`,
`clipvault-db`, `clipvault-platform`, `clipvault-search`) más una GUI Svelte en
Tauri. Las reglas arquitectónicas prohíben duplicar reglas de negocio entre
core y frontend, acceder directamente a SQLite desde la GUI o incluir lógica
sustantiva en los comandos Tauri. Esta propuesta mantiene esa separación.

## Goals / Non-Goals

**Goals:**

- Persistir en SQLite, mediante migraciones reversibles y explícitas, una
  `ignored_apps` y una `settings` consultables desde `clipvault-core`.
- Hacer que el pipeline de captura descarte contenido cuyo origen pertenece a
  la blacklist, sin escribir ese contenido en logs y sin exponer una API que
  lo devuelva.
- Exponer ajustes locales (retention, ignored apps, quick-paste hotkey) con
  validación estricta y preservación del último valor válido, доступных desde
  Tauri (comandos delgados) y la futura CLI.
- Introducir un redactor determinístico y puro en `clipvault-core`,
  integrado en `tracing-subscriber` como un `Layer`/`fmt::Layer` o un
  wrapper de eventos, que enmascare secretos y contenido del portapapeles
  antes de escribirlos.
- Permitir que el motor de captura y la sweep de retención sean testables
  con un `MockPlatformClipboard` y un `MockBlacklistMatcher`, sin GUI ni
  clipboard real.

**Non-Goals:**

- No se agrega telemetría, red, cuentas, LLMs, embeddings ni servicios
  externos.
- No se modifica el formato del historial más allá de campos derivados de
  la sweep de retención.
- No se introduce un editor completo del quick-paste hotkey desde la GUI
  (sólo se muestra y se persistirá; el binding real viene del crate
  `clipvault-platform`).
- No se rediseña el formato binario de la base de datos; las migraciones
  agregan tablas e índices sin tocar filas existentes.
- No se agrega import/export ni transformaciones; siguen siendo objetivos de
  v0.2/v0.3.

## Decisions

### 1. Tablas SQLite nuevas con migración explícita y reversible

- `ignored_apps(id TEXT PRIMARY KEY, created_at INTEGER NOT NULL)` donde
  `id` es el identificador normalizado.
- `settings(key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at INTEGER
  NOT NULL)` con `value` como JSON serializado a partir de tipos fuertes en
  `clipvault-core`.
- Índices: `ignored_apps(id)` (PK), `settings(key)` (PK).
- Migración: nueva `Migration` registrada en el runner, reversible bajando
  con un `DROP TABLE ...` envuelto en transacción; tests de integración
  ejercen la subida desde la versión anterior y la bajada.

**Alternativas consideradas:**

- Almacenar todo en un único `JSON` blob: rechazado porque perderíamos
  índices y haríamos imposible auditar la blacklist con SQL directo.
- Usar `serde_json` como única fuente de verdad también para
  `ignored_apps`: rechazado por la misma razón y porque la lista necesita
  crecer a cientos de elementos sin penalizar latencia.

### 2. Trait `ApplicationBlacklistMatcher` con adapters por plataforma

- Nuevo trait en `clipvault-platform`:
  ```rust
  pub trait ApplicationBlacklistMatcher {
      fn current_source(&self) -> Option<String>;
  }
  ```
- Adapter `MacosBundleMatcher` (vía `objc2` + `NSWorkspace`), `X11ClassMatcher`
  (vía `x11rb`), `WaylandAppIdMatcher` (vía `wayland-client` o equivalente
  ya integrado), y `UnknownSessionMatcher` que devuelve `None`.
- En `clipvault-core`, una `BlacklistMatcher` neutral con método
  `matches(&self, source: Option<&str>) -> bool` que envuelve al trait y
  aplica normalización (lowercase + dedupe de espacios).
- En tests, `MockBlacklistMatcher` configurable para devolver `Some(id)`,
  `Some(other)` o `None`.

**Alternativas consideradas:**

- Hacer que la captura pida al clipboard directamente el origen y omita la
  indirección: rechazado porque obligaría al core a depender del crate de
  plataforma; mantenemos el trait para conservar la separaciòn de capas.

### 3. `PrivacyGate` en el pipeline de captura

- `clipvault-core` recibe el evento de captura y lo pasa por un
  `PrivacyGate` antes de llamar al `HistoryRepository`.
- `PrivacyGate` tiene acceso a:
  - una snapshot reciente de la blacklist (`Arc<[String]>` recargable al
    cambiar la configuración),
  - la `RetentionPolicy` vigente,
  - el `BlacklistMatcher` de plataforma.
- Si el `PrivacyGate` rechaza el evento, devuelve `CaptureDecision::Discard`
  con una razón tipada (`Blacklisted`, `MalformedSource`, etc.) para que el
  caller registre un `tracing::info!` sin contenido.
- Esto evita que la lógica de privacidad se filtre al GUI o al
  almacenamiento.

### 4. Redactor puro + capa de tracing

- Módulo `redact` en `clipvault-core` con función pura:
  ```rust
  pub fn redact<S: AsRef<str>>(input: S) -> String
  ```
- Detecta categorías: JWT, AWS access keys, basic auth headers, private
  keys (`-----BEGIN ... PRIVATE KEY-----`), high-entropy strings (token
  bearer), cookies `Set-Cookie` largas, prefijos comunes (`password=`,
  `token=`, `secret=`).
- Reemplaza cada match por `[REDACTED:secret]` y conserva el resto.
- La integración con `tracing-subscriber` se hace mediante un `Layer`
  personalizado que reemplaza el `format_event` para que cualquier log que
  contenga `clipboard` o `password` sea pasado por `redact::redact` antes
  de escribirse.
- La función `redact` es testeable sin subsistemas.

**Alternativas consideradas:**

- Sobrescribir manualmente `Display` en cada tipo que pueda llevar
  contenido sensible: rechazado por dispersion y por no cubrir valores
  que ya estén formateados en `format!`.
- Usar un crate externo de redactoría: rechazado por privacidad (no
  queremos otra dependencia sin justificar) y porque las reglas son
  pocas y determinísticas.

### 5. `SettingsService` y validación tipada

- Tipo `Settings` con campos:
  ```rust
  pub struct Settings {
      pub retention: RetentionPolicy,
      pub ignored_apps: Vec<IgnoredApp>,
      pub quick_paste_hotkey: Option<HotkeySpec>,
  }
  ```
- `RetentionPolicy` es un enum `Days7 | Days30 | Days90 | Forever`.
- `HotkeySpec` serializable y parseable, con validación de modificadores
  válidos en el backend.
- Validación: una función pura `validate(&Update) -> Result<Settings,
  ValidationError>`; cuando la GUI o CLI recibe un valor inválido, el core
  devuelve `ValidationError` con código legible y mantiene el valor
  anterior. Tauri lo traduce a `Result<...>` con un mensaje sin payload.

### 6. `RetentionService` con sweep al inicio y periódico

- Componente en `clipvault-core` que:
  1. Lee la `RetentionPolicy` efectiva al construirse.
  2. Calcula el horizonte (`now - policy`) y borra entradas no favoritas
     con `captured_at < horizonte`.
  3. Hace lo mismo en un loop con `interval` derivado de la policy
     (snap a 5 minutos mínimo para evitar trabajo).
  4. Devuelve `RetentionReport { removed: usize }` para diagnóstico local.
- La sweep corre dentro de una transacción SQLite para no romper el
  historial en caso de error.

**Alternativas consideradas:**

- Hacer la sweep en cada captura: rechazado por latencia impredecible y
  por no cubrir el caso de arranque en frío.
- Borrado físico en SQLite con `DELETE FROM`: preferido sobre marcar
  expirados en esta iteración porque el spec todavía no modela
  `expired_at`; si más adelante hace falta mantenerlas visibles, se
  introduce una columna sin migración destructiva.

### 7. Comandos Tauri delgados

Nuevos comandos, todos thin wrappers:

- `settings_get() -> Settings`
- `settings_set(update: SettingsUpdate) -> Result<Settings, String>`
- `ignored_apps_list() -> Vec<IgnoredAppDto>`
- `ignored_apps_add(id: String) -> Result<IgnoredAppDto, String>`
- `ignored_apps_remove(id: String) -> Result<(), String>`

Estos no contienen lógica de validación; la delega vive en
`clipvault-core`. Los errores serializados no llevan contenido del
portapapeles.

### 8. UI Svelte: pestaña Privacidad y blacklist

- Nueva pestaña "Privacidad" en la ventana principal (`App.svelte` +
  `SettingsPanel.svelte`).
- Lista editable de aplicaciones ignoradas con `add` / `remove` por id.
- Dropdown de retention con etiquetas legibles (7/30/90 días o Conservar
  siempre).
- Vista read-only del hotkey efectivo.
- Mensaje claro cuando un valor es rechazado (campo + razón).
- Un mini panel "Vista previa de redacción" que muestra un ejemplo de log
  antes/después. Calcula la redacción en WASM/JS sólo si hay un mock
  local; de lo contrario se calcula en Rust a través de un comando
  opcional `redact_preview` que aplica la misma función.

## Risks / Trade-offs

- **[Riesgo: identificadores de plataforma cambian entre sesiones]**
  → **Mitigación:** el `ApplicationBlacklistMatcher` se invoca en cada
  captura con un TTL corto; la normalización es case-insensitive y los
  tests cubren sesiones que cambian de app cada pocos eventos.

- **[Riesgo: la sweep de retención borra contenido útil]**
  → **Mitigación:** la sweep es no destructiva hasta que el usuario
  confirma; los favoritos nunca se borran; el reporte `RetentionReport`
  permite auditar.

- **[Riesgo: el redactor produce falsos negativos]**
  → **Mitigación:** mantenerlo determinístico con una lista cerrada de
  patrones; documentar que es una redactoría defensiva, no una
  detección exhaustiva; cualquier log que mencione `clipboard`, `password`
  o `token` es además truncado a 200 caracteres antes de pasar por el
  redactor.

- **[Riesgo: el formato JSON de `settings` quede inconsistente entre
  versiones]**
  → **Mitigación:** incluir un campo `schema_version` en el JSON y validar
  al deserializar; la migración de la tabla no toca filas pre-existentes.

- **[Riesgo: el matcher de Linux requiera una dependencia nativa
  nueva]**
  → **Mitigación:** mantener el trait agnóstico; el adapter puede ser
  opcional vía feature flag igual que `linux-x11`. Si no hay adapter
  utilizable, el matcher devuelve `None` y la blacklist no se evalúa
  (documentado).

- **[Riesgo: el redactor introduzca regresiones en logs ya útiles]**
  → **Mitigación:** el redactor sólo afecta el formateo final, no la
  captura de campos estructurados; los tests verifican que campos como
  `error_category` o `entry_id` no se ven alterados.

## Migration Plan

1. Subir el esquema de SQLite con la migración `m_ignored_apps_and_settings`
   que crea las dos tablas nuevas y registra su bajada.
2. Sembrar defaults en `settings` al primer arranque
   (`retention=forever`, `hotkey=null`, blacklist vacía).
3. Activar el `PrivacyGate` en el pipeline de captura. La blacklist vacía
   equivale al comportamiento actual.
4. Activar el `RetentionService` en el `AppContext`.
5. Activar el `Layer` redactor en `tracing-subscriber` para logs de
   aplicación, sin afectar `tracing-test` u otras herramientas internas.
6. Rollback: ejecutar `m_ignored_apps_and_settings.down`, descartar las
   instancias del `PrivacyGate`, `RetentionService` y del `Layer`
   redactor. No hay migración de datos irreversible.

## Open Questions

- ¿Cómo se obtiene el `application_id` en Wayland cuando `wl_app_id` no
  está presente? Si ningún adapter portable existe, podemos dejar la
  blacklist efectiva sólo para macOS y X11 en MVP.
- ¿Se debe permitir que la GUI configure el `quick-paste hotkey` o sólo
  mostrarlo? Hoy se propone mostrar; abrir el picker de hotkeys queda
  fuera de este cambio.
- ¿Conviene que la sweep de retención tenga un jitter para evitar
  ráfagas en arranques simultáneos de varios procesos ClipVault? Sí por
  defecto, pero se confirmará con la decisión final sobre
  `clipvault-cli`.
