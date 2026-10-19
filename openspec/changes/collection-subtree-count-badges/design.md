# Design: collection-subtree-count-badges

## Conteo de la colección principal

- El badge de una colección de nivel superior suma dos partes: sus capturas
  directas que no aparecen en ninguna subcolección, y los conteos directos de
  cada subcolección.
- Si una captura pertenece a la principal y a una hija, se cuenta por la hija
  y no se vuelve a sumar desde la membresía directa de la principal. Si
  pertenece a varias hijas, contribuye una vez al conteo de cada hija.
- El badge de una subcolección sigue contando únicamente sus membresías
  directas. `Historial` conserva su conteo de todas las capturas.
- La consulta agregada se mantiene en SQLite y devuelve solo conteos; no carga
  contenido, IDs ni datos sensibles de las capturas al snapshot.

## Actualización y eliminaciones

Los conteos no se almacenan como contadores mutables. Cada snapshot los
calcula desde las membresías persistidas. Al vaciar una colección o eliminar
capturas/subcolecciones, la consulta siguiente refleja automáticamente el
estado resultante. Se reutilizan los eventos existentes de actualización de
organización e historial para solicitar ese snapshot; no se añade otra ruta
de sincronización.

## Verificación

- Cubrir en el repositorio una captura directa, capturas solo en subcolecciones
  y una captura compartida por varias hijas; validar la suma del principal y
  los conteos directos de las hijas.
- Validar que retirar membresías mediante vaciado o eliminación cambia el
  conteo de la siguiente lectura sin reiniciar ni recalcular estado en la UI.
- Ejecutar regresiones frontend de colecciones y drag and drop, checks y build
  afectados, pruebas Rust enfocadas, validación estricta de OpenSpec y revisión
  del diff.
