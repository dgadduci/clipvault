# Diseño: resolver y reenviar la atribución efectiva de origen

## Decisión

Las rutas que sirven historial e importaciones resuelven una sola presentación
efectiva por entrada. Usan primero `clipboard_entries.source_app_name` y
`source_app_icon_ref` de la captura local. Si ambos campos están vacíos,
consultan la procedencia de importación más reciente de esa misma entrada que
contenga nombre o referencia de icono. La consulta devuelve sólo esos campos;
no devuelve ni serializa el peer_id que originó la importación.

El orden de procedencias es determinista: `imported_at DESC`, seguido por
`remote_entry_id ASC` y `peer_id ASC` para resolver empates. Una procedencia sin
nombre ni icono no oculta una procedencia anterior que sí tenga presentación.
El registro local de origen tiene prioridad para evitar atribuir una captura
local al equipo desde el que también se importó contenido idéntico.

## Superficies de transporte

- Las páginas de historial de texto e imagen resuelven en lote el nombre
  efectivo y lo incluyen sólo si el peer autorizado anuncia
  `source_app_presentation`. No incluyen bytes, rutas ni referencias de icono.
- Las respuestas de importación explícita de texto e imagen usan la misma
  selección. Envían el nombre validado y leen el PNG desde el almacén local
  sólo mediante una referencia segura; los límites actuales de tamaño,
  dimensiones y decodificación siguen vigentes.
- El receptor valida la respuesta con las reglas existentes y guarda nombre e
  icono únicamente en la procedencia del par que está importando. Se preserva
  el comportamiento actual de deduplicación y rollback de iconos.
- Si no existe metadata propia ni procedencia válida, el flujo continúa y el
  receptor muestra el fallback de origen desconocido.

## Persistencia y privacidad

No se añade almacenamiento ni una migración: `remote_imports` ya guarda la
atribución por entrada importada y los iconos están en
`assets/application-icons/`. La consulta de reenvío se limita al id local de la
entrada y proyecta nombre/referencia, sin exponer identificadores de peers.
Nunca se transmite el peer_id de origen, el identificador bruto de aplicación,
la ruta local o la referencia local del icono. La capacidad aditiva existente
continúa controlando la metadata en la red.

Las consultas de páginas deben ser por lote para no hacer una consulta por
fila. Las importaciones resuelven una sola entrada por solicitud.

## Verificación

- Prueba de repositorio: selección de la procedencia válida más reciente,
  desempate determinista y aislamiento por entrada.
- Pruebas de host: nombre propagado en páginas de texto e imagen; nombre e
  icono propagados en importaciones explícitas de texto e imagen.
- Prueba de precedencia local y fallback cuando no existe atribución importada.
- Mantener las pruebas de validación de nombre/PNG, límites y persistencia por
  procedencia existentes.
- Validar el cambio OpenSpec, los crates Rust afectados y revisar el diff sin
  tocar otros cambios pendientes del workspace.
