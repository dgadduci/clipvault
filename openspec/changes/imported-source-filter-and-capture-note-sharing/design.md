# Diseño: filtros de origen importado y envío opcional de notas

## Atribución importada en el filtro

La consulta actual agrega `source_app`, nombre e icono desde
`clipboard_entries`. Las importaciones conservan la atribución de origen en
las filas de procedencia del par, para no sobrescribir la metadata propia de
una captura local. La consulta de opciones debe combinar ambas fuentes usando
la misma resolución de atribución efectiva que se utiliza al presentar la
card.

- En la colección vinculada a un par se usa sólo la atribución de la
  procedencia correspondiente a ese par.
- En Historial y en colecciones locales, la metadata propia de la captura
  tiene prioridad; cuando no existe, se usa la procedencia válida más reciente
  con desempate determinístico.
- El alcance se aplica a todas las capturas asociadas a la colección, no sólo
  a las cards cargadas en el rail. Las capturas de texto e imagen siguen
  participando.
- Cada opción muestra el nombre validado y el icono guardado localmente. Si
  falta el icono, conserva la opción con el fallback local existente.
- El valor interno de una opción será opaco y estable para la consulta; no
  expondrá identificadores de pares, rutas remotas, hashes de capturas ni
  contenido. Al seleccionar una opción, el predicado debe coincidir con la
  misma atribución efectiva usada para construirla.
- `Aplicación desconocida` se refiere a capturas sin atribución efectiva. Las
  opciones continúan combinándose con colección, tags y búsqueda según las
  reglas existentes.

No cambia la posición ni el comportamiento accesible del combobox. El cambio
de toolbar activo conserva su responsabilidad de layout; esta propuesta sólo
extiende el origen de los metadatos y la correspondencia de la opción con sus
resultados.

Al cambiar de colección, la consulta de opciones recibe explícitamente el id
destino del evento de selección. La resolución de Historial no depende del
valor reactivo que Svelte todavía puede estar actualizando. Cada consulta
incrementa un token, por lo que una consulta en curso nunca suprime la nueva;
las respuestas antiguas se descartan y la respuesta vigente también debe
declarar el mismo alcance solicitado antes de actualizar el combobox.

## Preferencia local de exportación

La preferencia será un booleano del agregado `Settings`, persistido por la
infraestructura actual de `app_settings`. Si no existe valor guardado, la
opción queda desactivada para conservar el comportamiento privado de las
notas. El modal existente de Configuración general la cargará y actualizará
mediante los comandos tipados actuales de settings. El menu ya contiene el
item `Configuración general` y seguirá abriendo ese modal.

La preferencia sólo controla lo que este equipo envía. No impide que el usuario
importe una nota que otro dueño decidió compartir. El receptor guarda una nota
recibida como su nota local separada del contenido de la captura. Al importar
contenido duplicado, si ya existe una nota local, ésta se conserva sin
confirmación ni sobrescritura; si no existe, se puede guardar la nota recibida.

## Contrato de transferencia entre pares

La capacidad de compartir notas se anuncia mediante un token aditivo en el
TXT separado `caps_extra_v3`. La separación conserva la compatibilidad con
clientes actuales que validan estrictamente los tokens de `caps_extra_v2`;
no sustituye las capacidades de pairing, importación de imágenes o
presentación de aplicaciones existentes. El host incluye la nota
sólo en la respuesta al fetch explícito de una captura de texto o imagen, y
sólo cuando su preferencia local está habilitada y el cliente solicitante
anuncia esa capacidad.

La navegación remota, filas de historial, resultados de búsqueda, respuestas
de miniaturas y diagnósticos nunca contienen el cuerpo de una nota. Una
preferencia desactivada, una nota ausente o un cliente sin capacidad compatible
omiten la nota sin impedir la importación de la captura. El cuerpo se valida y
queda sujeto a los límites de tamaño ya aplicados al transporte; los errores y
logs no incluyen su contenido.

Los pares antiguos siguen importando capturas con el contrato anterior. Los
pares nuevos que reciben una captura sin nota no crean ni borran notas locales.
Las notas de colecciones permanecen siempre locales.

## Verificación prevista

- Pruebas de repositorio/core para opciones importadas en Historial, colección
  vinculada a un par y colección local; atribución, filtro y alcance deben
  coincidir con la presentación de cards.
- Pruebas de settings para default desactivado, persistencia, actualización
  parcial y error de escritura.
- Pruebas de transferencia de texto e imagen para opt-in del emisor, capacidad
  compatible, omisión en navegación/miniaturas, compatibilidad legacy,
  deduplicación y preservación de notas locales existentes.
- Pruebas frontend de visibilidad/navegación de opciones y de la casilla de
  Configuración general, incluidos estados de carga y error.
- Regresión de transición Historial → colección con una consulta anterior en
  curso: las opciones corresponden al destino y una respuesta obsoleta no las
  reemplaza.
- Checks y regresiones frontend, incluidas las de drag-and-drop exigidas por
  `AGENTS.md`, validación OpenSpec y prueba manual en macOS y Linux.
