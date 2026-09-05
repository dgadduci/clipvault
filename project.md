# ClipVault

## Resumen

ClipVault es un workspace local para el portapapeles, snippets y utilidades de productividad en macOS y Linux. Convierte el historial del portapapeles en una herramienta rápida, privada y reutilizable, con búsqueda, favoritos, organización y transformaciones locales.

La aplicación debe funcionar offline, sin cuenta, sin nube y sin servicios externos obligatorios.

## Visión y principios

- **Local first:** toda la información permanece en el equipo del usuario.
- **Privacidad por defecto:** no hay telemetría obligatoria ni envío de contenido a Internet.
- **Rápido y discreto:** bajo consumo de CPU y memoria cuando está inactiva.
- **Keyboard first:** las tareas frecuentes se realizan con atajos y teclado.
- **Simple en superficie, potente por debajo:** la búsqueda rápida es mínima; las funciones avanzadas viven en la ventana principal.
- **Multiplataforma real:** macOS y Linux son plataformas de primera clase.

## Stack

- **Core y backend:** Rust.
- **Desktop shell:** Tauri 2.
- **Frontend:** Svelte + TypeScript, salvo que exista una razón documentada para usar React.
- **Persistencia:** SQLite embebido con migraciones, índices y transacciones.
- **Logs:** `tracing`, únicamente locales y sin contenido sensible.

La aplicación final debe ser autocontenida: el usuario no debería tener que instalar Rust, Node.js, Python, SQLite, PostgreSQL o Docker para ejecutarla.

## Arquitectura

La lógica de negocio debe poder ejecutarse sin GUI y ser compartida por la aplicación Tauri y la CLI futura.

```text
OS Clipboard
    ↓
Platform Adapter
    ↓
Normalizer → Type Detector → Security Filter
    ↓
Duplicate Detection → Rules Engine
    ↓
SQLite / Search Index
    ↓
Tauri UI or CLI
```

Estructura objetivo:

```text
clipvault/
├── crates/
│   ├── clipvault-core/
│   ├── clipvault-db/
│   ├── clipvault-platform/
│   ├── clipvault-search/
│   ├── clipvault-cli/
│   └── clipvault-rules/
├── app/
│   └── tauri/
│       ├── src-tauri/
│       └── frontend/
├── docs/
├── tests/
└── openspec/
```

Datos del usuario:

```text
~/.clipvault/clipvault.db
~/.clipvault/assets/
```

La GUI y los comandos Tauri deben ser adaptadores delgados. El frontend no debe acceder directamente a SQLite ni contener lógica de negocio.

## Funcionalidad objetivo

### MVP v0.1

1. Aplicación Tauri funcional.
2. Core Rust desacoplado de la GUI.
3. SQLite con migraciones.
4. Captura de texto del portapapeles.
5. Historial con prevención de duplicados mediante hash.
6. Búsqueda de texto con fuzzy search.
7. Ventana rápida y hotkey global.
8. Selección y pegado del elemento elegido.
9. Favoritos.
10. Eliminación y expiración básica.
11. Blacklist de aplicaciones.
12. System tray / menu bar.
13. Soporte inicial para macOS y Linux.
14. Configuración mínima.

La experiencia diaria objetivo es:

```text
atajo → escribir → Enter → pegar
```

### v0.2

- Imágenes y otros formatos de clipboard.
- Detección determinística de tipos.
- Tags y colecciones.
- Snippets con variables.
- Transformaciones locales de texto.
- Paste Stack.

### v0.3

- CLI compartida con el mismo core y SQLite.
- Reglas automáticas.
- Detección de secretos.
- Import/export local.
- Comparación diff.
- Acciones contextuales por tipo.
- Estadísticas locales.

## Tipos y acciones futuras

La detección debe ser determinística, usando parsing, expresiones regulares y heurísticas; no se utilizarán embeddings ni IA.

Tipos previstos: texto, URL, código, imagen, ruta, HTML, JSON, email, teléfono, comando shell, SQL, IPv4, IPv6, UUID, color HEX y JWT.

Acciones previstas incluyen formatear JSON/SQL, codificar URLs o Base64, abrir URLs, generar QR, abrir rutas en Finder/File Manager y guardar snippets.

## Privacidad y seguridad

- No cloud, no cuentas, no login, no LLM, no OpenAI, no Ollama, no embeddings y no servidor remoto obligatorio.
- Sin telemetría obligatoria.
- Lista configurable de aplicaciones ignoradas, con soporte para gestores de contraseñas.
- Detección conservadora de posibles secretos antes de almacenar contenido.
- Políticas configurables: no almacenar, almacenar temporalmente o almacenar normalmente.
- Favoritos no expiran; el historial debe admitir políticas de 7, 30, 90 días o forever.
- El contenido sensible puede tener una expiración más corta.
- Nunca registrar en logs el contenido sensible completo, contraseñas, tokens o claves privadas.

## Plataformas y empaquetado

V1 soporta macOS y Linux. Linux debe contemplar diferencias entre X11 y Wayland mediante adaptadores, sin asumir que ambos entornos funcionan igual.

Windows puede considerarse en el futuro, pero no forma parte del alcance inicial.

Artefactos objetivo:

- macOS: `.app` y `.dmg`.
- Linux: `.AppImage` y `.deb`; `.rpm` queda para una etapa posterior.

## Requisitos no funcionales

- Inicio rápido y respuesta inmediata en búsqueda y pegado.
- CPU mínima en estado idle.
- Meta aproximada de menos de 100 MB de RAM, sin convertirla en una restricción rígida del MVP.
- Un error al leer un formato extraño del clipboard no debe cerrar la aplicación.
- SQLite debe usar migraciones, índices apropiados y transacciones; WAL se evaluará cuando corresponda.
- Tests unitarios, de integración, parsers, reglas, almacenamiento y transformaciones.
- Los componentes de plataforma deben poder sustituirse por mocks en los tests del core.

## Fuera de alcance inicial

- Sincronización automática entre dispositivos.
- Servicios cloud o servidor remoto.
- PostgreSQL y Docker como requisitos.
- Búsqueda semántica o integración con LLMs.
- Windows.
- Funciones avanzadas antes de que el MVP de captura, búsqueda y pegado sea estable.

## Decisiones pendientes

- APIs nativas concretas para clipboard y hotkeys en macOS, X11 y Wayland.
- Estrategia de indexación de búsqueda para el MVP.
- Forma final del instalador y pipeline de releases.
- Contrato de comandos Tauri y diseño de la CLI.
- Política exacta de detección y retención de secretos.

## OpenSpec

OpenSpec es la fuente de verdad para cambios de comportamiento y decisiones de implementación. Cada cambio sustancial debe tener propuesta, diseño, especificaciones y tareas antes de implementarse.
