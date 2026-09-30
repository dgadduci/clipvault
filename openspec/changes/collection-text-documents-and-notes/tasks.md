# Tareas de implementación: collection-text-documents-and-notes

## 1. Relevamiento y contrato

- [x] 1.1 Leer `project.md`, `AGENTS.md`, estas propuesta/diseño/specs y las
  specs vigentes de texto, historial, cards, colecciones e importaciones.
- [x] 1.2 Revisar los servicios de creación/edición de texto, la migración de
  SQLite, las proyecciones de cards/colecciones y los modales existentes.
- [x] 1.3 Confirmar que el contrato de creación respeta deduplicación y la
  membresía obligatoria en Historial, y que los targets importados se
  rechazan desde el core.

## 2. Texto manual en colecciones

- [x] 2.1 Agregar al core y persistencia la creación atómica de una entrada de
  texto para Historial o una colección local elegible, reutilizando las reglas
  actuales de tipo, hash y deduplicación.
- [x] 2.2 Exponer el comando Tauri delgado y el bridge tipado para la creación;
  devolver resultado seguro sin incluir contenido en eventos o logs.
- [x] 2.3 Agregar una acción de nueva captura de texto al panel de Historial y
  colecciones locales, con modal textarea multilinea y cancelación sin
  persistir.
- [x] 2.4 Reutilizar el editor textual existente para editar el texto manual,
  conservar asociaciones y aplicar sus errores y reglas de duplicados.
- [x] 2.5 Rechazar desde el core el alta manual en una colección vinculada a
  importaciones, incluso si el frontend envía un ID obsoleto o manipulado.

## 3. Notas locales de capturas y colecciones

- [x] 3.1 Agregar una migración SQLite para notas opcionales de capturas y
  colecciones, con integridad referencial, timestamps propios y borrado
  atómico junto con el propietario.
- [x] 3.2 Implementar en el core y repositorio operaciones tipadas para leer,
  guardar y quitar una nota por captura o colección.
- [x] 3.3 Exponer comandos Tauri finos y bridges TypeScript; limitar las
  proyecciones de lista a `has_note` y no filtrar cuerpos por eventos/logs.
- [x] 3.4 Crear un modal de nota multilinea reutilizable para capturas y
  colecciones, incluyendo Guardar, Cancelar, eliminar al guardar vacío,
  busy/error, Escape, backdrop y retorno de foco.
- [x] 3.5 Integrar el icono y la acción Agregar/Editar nota en las cards; cubrir
  texto, texto enriquecido e imagen sin alterar layout, paste ni drag.
- [x] 3.6 Integrar el indicador y la acción de crear/editar nota en cada fila
  de colección, conservando selección, teclado, color, rename, delete y
  drop-zone.
- [x] 3.7 Mantener las notas locales fuera de búsqueda, transferencias entre
  pares, logs, diagnósticos, payloads de drag y eventos con contenido.

## 4. Verificación

- [x] 4.1 Añadir pruebas representativas de core/SQLite para alta manual,
  memberships, deduplicación, target importado, persistencia y cascadas de
  borrado.
- [x] 4.2 Añadir pruebas frontend del modal compartido, saltos de línea,
  agregar/editar/borrar nota, iconos, foco y estados de error.
- [x] 4.3 Ejecutar las regresiones de cards y drag-and-drop exigidas por
  `AGENTS.md`, además de los checks Rust y frontend afectados.
- [x] 4.4 Validar este cambio con OpenSpec, revisar diff y confirmar que no se
  modificaron assets existentes ni se añadieron logs de contenido.
- [ ] 4.5 Probar manualmente en macOS y Linux la creación/edición de texto,
  notas de capturas de texto e imagen y notas de colecciones, incluida la
  persistencia tras reiniciar.
