# AGENTS.md

## Roles y coordinación

- **Codex es la LLM arquitecta:** analiza problemas, releva el repositorio, define la arquitectura, toma decisiones técnicas y prepara los artefactos OpenSpec.
- **MiniMax es la LLM implementadora:** ejecuta las tareas aprobadas en OpenSpec, modifica el código y verifica la implementación.
- **OpenSpec es la fuente común de verdad:** las decisiones relevantes deben quedar reflejadas en `proposal.md`, `design.md`, `specs/` y `tasks.md`.
- Si la implementación revela una contradicción o una decisión arquitectónica nueva, MiniMax debe pausar y solicitar actualizar OpenSpec antes de continuar.

## Contexto obligatorio

Antes de trabajar, leer:

1. `project.md`.
2. `AGENTS.md`.
3. Los artefactos del cambio OpenSpec activo.
4. La estructura y el estado actual del repositorio.

No asumir que una funcionalidad está aprobada solo porque aparece en el roadmap: el MVP v0.1 tiene prioridad.

## Flujo OpenSpec

- Todo cambio sustancial debe comenzar con una propuesta OpenSpec.
- Codex prepara y mantiene coherentes propuesta, diseño, especificaciones y tareas.
- MiniMax implementa únicamente las tareas pendientes del cambio seleccionado.
- Las tareas se marcan como completadas inmediatamente después de verificarlas.
- Antes de entregar, ejecutar la validación de OpenSpec y revisar el diff.
- No implementar snippets, reglas, transformaciones, CLI, import/export o detección avanzada de secretos si no forman parte del cambio aprobado.

## Reglas arquitectónicas

- La lógica de negocio nunca debe depender directamente de Tauri, Svelte o el navegador.
- La GUI y la CLI deben consumir el mismo core Rust.
- Los comandos Tauri deben ser adaptadores delgados; no deben contener cientos de líneas de lógica.
- El frontend no debe acceder directamente a SQLite ni duplicar reglas de negocio.
- Las integraciones con el sistema operativo deben vivir detrás de traits/adaptadores testeables.
- El core debe poder probarse sin clipboard real, GUI, display gráfico o sesión específica de Linux.
- Mantener separados core, persistencia, búsqueda, plataforma y reglas según la estructura definida en `project.md`.
- No introducir una dependencia arquitectural nueva sin documentar su motivo, impacto y alternativa descartada en OpenSpec.

## Privacidad y seguridad

- ClipVault es local-first y offline-first.
- No agregar llamadas de red, cuentas, telemetría, analytics, cloud, LLMs, embeddings, OpenAI, Ollama, PostgreSQL ni Docker como requisitos.
- No registrar contenido completo del portapapeles, contraseñas, tokens, claves privadas ni otros secretos.
- No guardar credenciales en el código, fixtures, tests, documentación o logs.
- Las detecciones de secretos deben ser conservadoras y determinísticas.
- La lista de aplicaciones ignoradas debe poder ampliarse sin hardcodear una única plataforma.
- Toda exportación o importación debe ser explícitamente local y no debe subir datos a ningún servicio.

## Plataformas

- macOS y Linux son objetivos de primera clase desde el MVP.
- Linux debe distinguir X11 y Wayland; no asumir que una API o comportamiento de uno aplica al otro.
- Mantener la posibilidad de agregar Windows en el futuro sin contaminar el core con condicionales de plataforma innecesarios.
- Las pruebas específicas de plataforma deben aislarse de los tests puros del core.

## Dependencias y código

- Preferir la biblioteca estándar y dependencias pequeñas, maduras y justificadas.
- No agregar frameworks o paquetes por conveniencia sin una necesidad concreta.
- Mantener Rust formateado y sin warnings evitables.
- Usar nombres y APIs claros; evitar abstracciones prematuras.
- Las migraciones de SQLite deben ser explícitas, reversibles cuando sea posible y cubiertas por tests.
- Tratar correctamente duplicados, expiración, favoritos y errores de formatos inesperados.

## Testing y verificación

- Escribir solo los tests necesarios para demostrar el comportamiento nuevo o proteger una regresión concreta.
- La cobertura debe ser proporcional al riesgo y al impacto del cambio; no perseguir cobertura por porcentaje.
- Evitar tests redundantes, duplicados, triviales o que repitan exactamente la implementación.
- Preferir un test representativo por comportamiento y agregar casos límite solo cuando exista un riesgo real.
- Usar unit tests para lógica pura y tests de integración únicamente cuando validen la interacción entre módulos, SQLite o el sistema operativo.
- Mockear adaptadores de clipboard, hotkeys y plataforma solo cuando sea necesario para aislar el core o evitar dependencias no determinísticas.
- Los cambios de frontend o Tauri deben validarse con los checks y builds correspondientes cuando afecten esas capas.
- Las pruebas manuales deben limitarse a flujos que no puedan verificarse de forma confiable automáticamente, como hotkeys, tray/menu bar, captura y pegado.

Antes de entregar:

1. Ejecutar los tests relevantes para el cambio, no necesariamente toda la suite.
2. Revisar errores, warnings y el diff.
3. Confirmar que no se añadieron secretos ni archivos generados innecesarios.
4. Informar qué se verificó, qué no fue necesario probar y qué quedó pendiente.

## Git y alcance

- Mantener commits y cambios acotados al objetivo del cambio OpenSpec.
- No borrar, resetear o sobrescribir trabajo del usuario sin autorización explícita.
- No mezclar refactors amplios con una funcionalidad puntual.
- No modificar archivos fuera del repositorio.
- No crear releases ni publicar instaladores sin una solicitud explícita.

## Criterio de producto

La interacción cotidiana debe sentirse como Raycast, Alfred o Spotlight: abrir, buscar, pegar y desaparecer en uno o dos segundos. La ventana principal queda reservada para organización, configuración y funciones avanzadas.
