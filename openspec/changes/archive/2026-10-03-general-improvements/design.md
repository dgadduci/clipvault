# Diseño: general-improvements

## Contexto

`RemoteHistoryRail` controla la selección del equipo, la paginación de texto e
imágenes, el estado `loading` y la protección contra respuestas obsoletas.
`RemotePreviewCard` realiza la importación explícita y mantiene el resultado
visible de cada card. `App.svelte` y `OrganizationSidebar` son la ruta actual
de los drops hacia colecciones. `DesktopToolbar` ya ofrece la limpieza de
Historial, mientras que Rust y SQLite tienen operaciones confirmadas para
borrar una colección, pero no para vaciarla y conservar su definición. El
registro común de atajos ya permite configurar bindings locales y el modal
existente lista las acciones del usuario.

## Decisiones

### Spinner del rail remoto

El indicador pertenece a `RemoteHistoryRail`, que ya es dueño del estado de
carga y del token de generación de cada página. Se posiciona como overlay en
el centro geométrico del rail horizontal, sin cambiar el tamaño del rail ni
desplazar las cards. Se muestra mientras quede pendiente alguna de las
consultas de previews del equipo activo y desaparece al resolverse la carga,
incluidos los casos vacíos o con error. Una respuesta tardía de otro equipo no
puede alterar este estado. El indicador anuncia la carga mediante atributos y
texto accesibles localizados, pero no bloquea la interacción con las cards.

### Resultado de importación duplicada

Las respuestas de importación ya identifican localmente la entrada y la
colección vinculada mediante `collection_id`. El nombre se resuelve desde el
estado o la proyección local de colecciones; si el estado en memoria todavía
no contiene esa colección, la capa local obtiene el nombre persistido antes de
mostrar el resultado. El nombre no se recibe del contenido remoto ni se
construye a partir del título de la captura. La frase visible se limita al
ancho de la card con ellipsis; la etiqueta accesible conserva el mensaje
completo. No se expone contenido de captura, identificadores de peer ni datos
adicionales de procedencia.

### Resultados transitorios

Sólo los resultados de importación exitosa y duplicada se descartan tres
segundos después de aparecer. Cada resultado nuevo reemplaza el anterior y
reinicia su temporizador. El temporizador se cancela al destruir la card. Los
errores siguen el comportamiento actual para no ocultar automáticamente
información que puede requerir una acción del usuario.

### Colecciones vinculadas a equipos como destinos de drop

La elegibilidad del destino se valida tanto en la presentación de la zona de
drop como en el handler central, usando `is_peer_bound` además del tipo de
colección. El drop sobre una colección de par no ejecuta el comando de
asociación ni cambia datos; aun así, el gesto de soltar sobre esa fila se
reconoce para mostrar una explicación localizada. Las colecciones locales
siguen usando el flujo actual. Se conserva intacto el controlador de pointer,
su fallback de mouse, la captura/liberación del pointer y el payload opaco de
la card.

### Vaciar una colección desde la barra superior

La barra usa el mismo botón contextual de papelera: en Historial conserva la
limpieza de capturas no organizadas; dentro de una colección de usuario vacía
esa colección. La confirmación muestra únicamente metadatos locales, como la
colección y el total de capturas/favoritas, y ofrece cancelar, conservar las
capturas en Historial o borrarlas también del historial. El recuento comprende
la colección completa y no se limita por búsqueda, tags ni filtros visibles.

Vaciar y conservar elimina sólo las membresías de la colección seleccionada;
cada captura permanece en `Historial`, en otras colecciones y con su
procedencia local. Vaciar y borrar elimina las capturas seleccionadas de todo
el historial local, incluidas sus otras membresías, favoritos y procedencia
de importación. En ambas opciones se conserva la definición y, para una
colección de par, su binding y confianza del peer. Una importación posterior
puede volver a usar ese binding. La elección y sus cambios se aplican en una
transacción; los conteos obsoletos exigen presentar el preview actualizado y
volver a confirmar. La recolección de assets pasa por la gestión de historial
existente y respeta referencias compartidas.

### Atajo para abrir el listado

Agregar `open_keyboard_shortcuts` al registro común de acciones como atajo
local de la ventana principal, configurable desde el modal que abre. Su
default será `⌘⇧K` en macOS y `Ctrl+Shift+K` en Linux; usa una tecla y
modificadores ya admitidos por el registro. El matcher abre el modal existente
cuando la ventana principal está en su estado normal, sin crear otra ventana
ni convertirlo en un hotkey global. La fila lista la combinación activa y su
contexto, y permite cambiarla por el flujo de configuración existente. La
acción no interviene si ya hay un modal abierto o si el foco está en un campo
de edición.

### Localización y accesibilidad

Cada etiqueta, descripción, mensaje de rechazo, estado de carga, tooltip,
nombre de acción y control nuevo usa claves de traducción en `en`, `es`, `pt`,
`de` y `fr`, con los mismos placeholders en todos los catálogos. Los avisos
de estado no interrumpen el foco; los diálogos conservan navegación por
teclado, cancelación y retorno del foco al control que los abrió.

## Riesgos y mitigaciones

- **El overlay tapa previews parciales** → no captura eventos del pointer y
  permanece limitado al rail.
- **La colección recién creada aún no está en el estado frontend** → resolver
  su nombre mediante la proyección local antes de mostrar el duplicado.
- **Una colección cambia entre preview y confirmación** → comparar los
  conteos en la operación y exigir reconfirmación con el preview actualizado.
- **Borrar capturas puede afectar otras colecciones y favoritos** → la opción
  se distingue explícitamente en la confirmación y sigue el borrado global
  existente; la opción de conservar afecta sólo la membresía seleccionada.
- **Un drop rechazado puede filtrar una mutación** → validar el binding en la
  UI y nuevamente en el handler central antes de cualquier escritura.
- **El nuevo atajo puede chocar con otro binding local** → incluirlo en el
  registro y aplicar las reglas existentes de validación de conflictos.

## Migración

No se requieren migraciones de SQLite ni dependencias nuevas. La operación de
vaciado se agrega a los servicios existentes y no borra ni modifica datos del
equipo remoto. El nuevo binding usa el almacenamiento local de atajos
existente y un default cuando no hay una preferencia guardada.
