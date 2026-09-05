# history-card-layout

Este cambio rediseña la presentación del historial reciente de ClipVault.
Las capturas de texto dejan de mostrarse como filas verticales con tipografía
grande y pasan a una galería horizontal de cards cuadradas, compactas y
reconocibles.

Cada card muestra el tipo de contenido, el nombre e icono de la aplicación
fuente, un título editable y las acciones existentes de pin/unpin y eliminación.
La metadata de aplicación se obtiene detrás de adaptadores de plataforma y se
persiste localmente con referencias de assets controladas.

El cambio no implementa imágenes del portapapeles ni otros formatos binarios.
Eso pertenece al cambio posterior clipboard-rich-content. Tampoco reemplaza la
lógica de favoritos, eliminación, búsqueda, captura o quick-paste.

El cambio debe permanecer activo hasta completar la revisión manual y no debe
archivarse automáticamente.
