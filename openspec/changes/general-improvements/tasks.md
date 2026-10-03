# Tareas: mejoras generales de previews y colecciones

## 1. Relevamiento y contrato

- [x] 1.1 Revisar la carga de páginas remotas y los contratos de importación
  de texto e imagen, incluidos sus resultados deduplicados.
- [x] 1.2 Revisar cómo la proyección local identifica colecciones vinculadas
  a pares y cómo el sidebar recibe drops.
- [x] 1.3 Revisar los servicios actuales de historial, membresías, borrado de
  colecciones, preview de conteos y limpieza de assets.
- [x] 1.4 Revisar el registro de atajos, el modal existente, sus defaults,
  contextos y validación de conflictos.
- [x] 1.5 Identificar la barra superior, los flujos de confirmación y los
  cinco catálogos localizados afectados.

## 2. Indicador y resultado de previews remotos

- [x] 2.1 Mostrar un spinner centrado sobre `RemoteHistoryRail` durante las
  cargas activas, sin alterar el rail, bloquear cards ni aceptar respuestas
  obsoletas de otro peer.
- [x] 2.2 Resolver el nombre de la colección desde datos locales para los
  resultados duplicados de importaciones de texto e imagen.
- [x] 2.3 Limitar el resultado visible a la card con ellipsis y conservar el
  mensaje completo como nombre accesible.
- [x] 2.4 Descartar a los tres segundos los mensajes de importación exitosa y
  duplicada; limpiar o reiniciar correctamente los temporizadores.
- [x] 2.5 Añadir claves y placeholders de carga, resultado, truncamiento
  accesible y temporización a `en`, `es`, `pt`, `de` y `fr`.
- [x] 2.6 Cubrir carga concurrente de texto/imagen, cambio de peer con
  respuesta tardía, ambas formas de duplicado y expiración de estados.

## 3. Drops en colecciones vinculadas a pares

- [x] 3.1 Excluir `is_peer_bound` de los destinos válidos tanto en el
  feedback visual del sidebar como en el handler central.
- [x] 3.2 Reconocer el drop rechazado y presentar un mensaje localizado sin
  enviar una mutación ni alterar membresías.
- [x] 3.3 Conservar el controlador singleton, fallback de mouse, pointer
  capture/liberación, ghost, selección de texto, `touch-action`, cancelación,
  exclusión de controles, destinos scrolleables y payload de ID opaco.
- [x] 3.4 Ejecutar las regresiones frontend requeridas de drag-and-drop y
  cubrir un drop rechazado en colección de par y un drop válido en colección
  local.

## 4. Vaciado contextual de colecciones

- [x] 4.1 Mostrar en la barra superior una acción contextual para cada
  colección de usuario y conservar intacta la acción de Historial.
- [x] 4.2 Implementar un preview metadata-only del alcance completo de la
  colección y una operación transaccional para vaciarla sin borrar su
  definición ni binding.
- [x] 4.3 Implementar la opción de conservar capturas en Historial quitando
  sólo la membresía seleccionada, y verificar membresías restantes y
  procedencia.
- [x] 4.4 Implementar la opción de borrar capturas globalmente conservando la
  colección y el binding del peer; integrar gestión de historial,
  invalidación y limpieza segura de assets compartidos.
- [x] 4.5 Cubrir conteos obsoletos, colección vacía, errores, cancelación y
  atomicidad sin modificar el estado al cancelar o fallar.
- [x] 4.6 Crear un diálogo accesible con nombre, conteos, opciones explícitas,
  estados de carga/error y retorno del foco.
- [x] 4.7 Añadir todos los textos nuevos a `en`, `es`, `pt`, `de` y `fr`, con
  claves y placeholders en paridad.
- [x] 4.8 Cubrir colecciones locales y de pares, capturas favoritas, entradas
  en varias colecciones e importaciones deduplicadas.

## 5. Atajo para abrir el listado

- [x] 5.1 Añadir `open_keyboard_shortcuts` al registro común con default
  `⌘⇧K`/`Ctrl+Shift+K`, almacenamiento local y validación de conflictos.
- [x] 5.2 Añadir la fila al modal existente, conectar el matcher de la ventana
  principal y aplicar cambios aceptados sin recargar ventanas.
- [x] 5.3 Añadir la etiqueta de acción y su contexto a los cinco catálogos y
  conservar las demás entradas y puntos de acceso.
- [x] 5.4 Cubrir el binding predeterminado, apertura, edición, persistencia,
  conflicto y guardas de foco/modal.

## 6. Verificación

- [x] 6.1 Ejecutar tests de core/DB, regresiones frontend de cards y
  drag-and-drop, checks y build de las capas afectadas.
- [x] 6.2 Validar OpenSpec en modo estricto y revisar `git diff --check`, el
  diff acotado, los catálogos y la ausencia de archivos generados o secretos.

## 7. Aprobación manual

- [x] 7.1 El usuario aprobó la prueba manual de `general-improvements` antes
  de sincronizar las specs y archivar el cambio.
