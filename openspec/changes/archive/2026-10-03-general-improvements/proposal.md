# Propuesta: mejoras generales de previews y colecciones

## Problema

Al explorar capturas de otro equipo, la carga de previews no tiene un indicador
visible centrado sobre la lista horizontal. Los resultados de importación
duplicada no aclaran en qué colección local quedó la captura, y los mensajes
de importación exitosa o duplicada permanecen en la card indefinidamente.
Además, las colecciones vinculadas a equipos aparecen como destinos posibles
de drag-and-drop aunque no deben aceptar capturas. La barra superior permite
vaciar capturas sin organizar desde Historial, pero no ofrece una acción
equivalente para vaciar la colección seleccionada. El modal existente de
atajos de teclado tampoco tiene un acceso directo desde la ventana principal.

## Qué cambia

- Mostrar un spinner flotante centrado sobre el rail mientras se cargan
  previews del equipo activo.
- Informar en una importación duplicada el nombre de la colección local
  vinculada al equipo y truncar el texto visible para que quepa en la card.
- Desaparecer a los tres segundos los estados de importación exitosa y de
  captura duplicada.
- Rechazar drops sobre colecciones vinculadas a equipos y explicar el motivo
  con un mensaje localizado.
- Agregar a la barra superior una acción para vaciar cualquier colección de
  usuario. La confirmación permitirá conservar las capturas en Historial o
  borrarlas también del historial local.
- Agregar un atajo configurable para abrir el modal existente de atajos de
  teclado, incluyendo su binding y contexto en la lista que muestra ese modal.

## Capacidades

### Nueva

- `peer-preview-import-feedback`: carga de previews y resultados de
  importación entre equipos.

### Modificadas

- `desktop-header-card-dnd`: rechazo informado de drops sobre colecciones de
  equipos.
- `tags-and-collections`: acción de vaciado contextual en la barra superior,
  con elección explícita del alcance de borrado.
- `keyboard-shortcuts`: atajo local configurable para abrir el listado
  existente de atajos.

## Fuera de alcance

- Cambiar el transporte entre equipos, el contenido de las capturas o el
  comportamiento de importación y deduplicación.
- Cambiar el payload, el controlador singleton o los mecanismos de
  cancelación y fallback del drag-and-drop.
- Cambiar la acción actual de Historial para limpiar capturas no organizadas.
- Eliminar la definición o el vínculo de una colección cuando se la vacía.
- Cambiar la personalización multiplataforma de atajos globales o sumar una
  acción global para el modal.
- Sincronizar eliminaciones con el equipo remoto, agregar red, dependencias o
  migraciones de base de datos.

## Resultado esperado

El usuario distingue cuándo se están cargando previews, entiende dónde está
guardada una captura duplicada y recibe mensajes breves. Las colecciones de
pares no aceptan drops. Desde cualquier colección de usuario se pueden quitar
todas sus asociaciones o borrar también del historial local las capturas que
contiene, sin eliminar la colección ni su vínculo de par. El listado de
atajos se puede abrir desde la ventana principal mediante una combinación
visible y configurable en el propio listado.
