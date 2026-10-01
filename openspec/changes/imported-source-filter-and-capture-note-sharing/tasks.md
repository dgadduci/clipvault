# Tareas de implementación

## 1. Relevamiento y contrato

- [x] 1.1 Leer `project.md`, `AGENTS.md`, las especificaciones activas y las
  capacidades de desktop filtering, capture notes, peer text/image import,
  source-app presentation y settings.
- [x] 1.2 Seguir la consulta de opciones del filtro hasta SQLite y comparar
  sus fuentes de metadata con la resolución de atribución de las cards.
- [x] 1.3 Revisar los contratos de fetch explícito de texto e imagen, la
  negociación de capacidades y el comportamiento de deduplicación actual.
- [x] 1.4 Confirmar compatibilidad con `desktop-toolbar-layout`; reutilizar
  el item existente `Configuración general` sin alterar el layout del toolbar.

## 2. Filtro de aplicaciones importadas

- [x] 2.1 Extender la consulta core/repositorio para agregar atribución de
  `remote_imports` con la misma precedencia y desempate que las cards.
- [x] 2.2 Restringir las opciones a la colección activa, incluyendo Historial,
  colecciones vinculadas a pares y colecciones locales, sin usar el límite
  visible de cards.
- [x] 2.3 Extender el filtro aplicado a cards y búsqueda para que cada opción
  importada coincida con su atribución efectiva y no filtre por peer ID.
- [x] 2.4 Conservar iconos locales o el fallback genérico, el estado
  `Aplicación desconocida`, el orden determinístico y los contratos metadata-only.
- [x] 2.5 Cubrir el combobox y los resultados con pruebas representativas de
  Historial, colección de par y colección local.
- [x] 2.6 Cubrir el cambio Historial → colección: consultar usando el id
  destino aunque haya otra consulta en curso, descartar respuestas obsoletas y
  validar el alcance devuelto.

## 3. Consentimiento local para compartir notas

- [x] 3.1 Agregar una preferencia booleana persistente a `Settings`, con
  valor predeterminado `false` para instalaciones y bases existentes.
- [x] 3.2 Exponer la preferencia mediante `SettingsUpdate` y los comandos
  tipados actuales, preservando actualizaciones parciales y errores.
- [x] 3.3 Agregar una casilla accesible al modal abierto por el item existente
  `Configuración general`; mostrar carga, guardado y error sin perder el
  último valor persistido.
- [x] 3.4 Anunciar una capacidad aditiva de transferencia de notas sin cambiar
  los contratos existentes de pairing, importación ni presentación de apps.
- [x] 3.5 Adjuntar la nota únicamente a fetches explícitos de capturas de
  texto/imagen cuando el emisor optó por compartir y el receptor admite la
  capacidad; no exponerla durante browse, búsqueda o miniaturas.
- [x] 3.6 Guardar la nota recibida separada de la captura, mantener el import
  exitoso si la nota se omite y preservar cualquier nota local ya existente
  cuando se deduplica el contenido.
- [x] 3.7 Cubrir preferencias ausentes, pares antiguos, notas ausentes,
  contenido duplicado, errores de persistencia y límites del transporte.
- [x] 3.8 Cubrir la deserialización de una actualización parcial que sólo
  envía `capture_notes_sharing_enabled`, omitiendo las listas de aplicaciones
  ignoradas sin alterar sus valores persistidos.

## 4. Verificación

- [x] 4.1 Validar que `Aplicación desconocida`, tags, búsqueda y cambio de
  colección conservan sus semánticas anteriores.
- [x] 4.2 Ejecutar regresiones frontend de cards y drag-and-drop, checks de
  core/DB/Tauri/frontend afectados y build correspondiente.
- [x] 4.3 Ejecutar `openspec validate` en modo estricto y revisar `git diff`
  y `git diff --check` sin modificar assets persistidos.
- [ ] 4.4 Probar manualmente en macOS y Linux opciones importadas en Historial
  y colecciones, toggle de exportación apagado/encendido y recepción de notas
  desde pares compatibles.
