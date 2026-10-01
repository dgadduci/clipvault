# Diseño: collection-text-documents-and-notes

## Modelo de texto manual

Un documento manual será una entrada de texto del modelo de historial
existente, no una entidad paralela. El flujo de creación llamará a un servicio
del core y al repositorio actual; la GUI no accederá directamente a SQLite.
Se reutilizarán la clasificación determinística, el hash y las reglas de
deduplicación existentes.

La creación recibirá el identificador de la colección activa. El core aceptará
Historial o una colección local de usuario sin vínculo de importación y
rechazará una colección obsoleta o vinculada a un par. En una transacción,
guardará o resolverá la entrada según la deduplicación existente, conservará
su pertenencia a Historial y agregará la pertenencia a la colección local
seleccionada cuando no sea Historial. Si el contenido ya existe, se reutiliza
la entrada existente y se garantiza su asociación a la colección solicitada;
no se crea una fila duplicada.

El documento conserva literalmente los saltos de línea. La UI ofrecerá una
acción de creación en el panel de la colección activa y abrirá un modal con un
textarea nativo. La edición posterior reutilizará el flujo de edición textual
existente. La creación manual no lee ni escribe el portapapeles y es una
acción local explícita, independiente de la pausa de captura automática o de
la aplicación fuente ignorada.

## Persistencia de notas

Cada captura y cada colección tendrá como máximo una nota opcional. Se
persistirán en tablas locales separadas, con una clave foránea al registro
propietario y una marca de modificación propia. Una migración SQLite agregará
ambas tablas. Al borrar el propietario se elimina su nota en la misma
transacción; borrar una colección no conserva una nota huérfana aunque se
preserven sus capturas.

Las notas admiten texto plano y saltos de línea. Se guardan sin transformar el
contenido. Una nota vacía equivale a quitar la nota y oculta su icono. La nota
no cambia el contenido, hash, tipo, captura/actualización, título, fuente,
favorito, tags, colecciones ni asset de una entrada; tampoco cambia nombre,
color o membresías de una colección. La nota conserva su propio timestamp de
edición.

El core expondrá operaciones tipadas para leer, guardar y quitar notas de
capturas y colecciones. Los comandos Tauri serán adaptadores delgados. Las
proyecciones de lista/card sólo llevarán un booleano `has_note`; el cuerpo se
cargará al abrir el modal. Los eventos de actualización serán metadata-only y
no incluirán el texto de la nota.

## Interacción de notas

Un modal compartido recibirá un destino tipado (`entry_id` o
`collection_id`), un borrador de texto y callbacks tipados. El mismo componente
servirá para notas de capturas y de colecciones, con textarea, Guardar,
Cancelar, foco inicial, retorno de foco, Escape, cierre por backdrop y estados
de error/busy coherentes con el shell Modal existente.

- En una card, el icono de nota se muestra cuando existe una nota y al
  activarlo abre el modal. Cuando no existe, el menú de la card ofrece
  `Agregar nota`; si existe, ofrece `Editar nota`.
- En cada fila de colección habrá una acción accesible de nota. Con nota, el
  icono identifica su presencia y abre el modal; sin nota, la acción permite
  crearla. La acción no cambia la selección ni inicia drag-and-drop.
- Guardar una nota vacía elimina la asociación y oculta el icono de presencia.
  Cancelar, Escape o backdrop descartan el borrador sin escribirlo.
- Los iconos tendrán nombres accesibles claros. El control de nota es
  interactivo y queda excluido del controlador singleton de drag-and-drop.

La card mantiene su tamaño fijo y sus controles actuales de pin, menú, título,
tags y colecciones. La acción de nota se integra en el área de controles sin
convertirse en una zona de arrastre ni activar paste.

## Privacidad y límites de actualización

El cuerpo de una nota es metadata privada local: no entra en texto capturado,
clipboard, búsqueda, logs, diagnósticos, tooltips de lista, eventos de
refresh, payload de drag ni transferencias de capturas entre pares. Sólo la
respuesta del comando de lectura/guardado al modal contiene el cuerpo. Los
errores no deben incluirlo.

Actualizar una nota emite el refresh metadata-only necesario para actualizar
el indicador `has_note`, sin invalidar assets de imagen ni modificar el
payload. La nota de una colección vinculada a un par pertenece a la copia
local de ClipVault y no se envía al par.

## Verificación prevista

- Pruebas de core y SQLite para creación manual, membresías, rechazo de
  colecciones importadas, deduplicación, persistencia, CRUD de notas y
  borrado en cascada.
- Pruebas frontend para textarea multilinea, ciclo modal, creación/edición,
  iconos y retorno de foco.
- Regresiones de cards, sidebar, selección de colección y drag-and-drop; el
  payload de drag conserva sólo el identificador opaco de la captura.
- Validar `cargo fmt`, checks/tests Rust afectados, `npm run check`, build y
  tests frontend afectados, más validación OpenSpec.
- Prueba manual de creación y edición en una colección local, en Historial,
  en capturas de texto e imagen y en notas de colecciones en macOS y Linux.
