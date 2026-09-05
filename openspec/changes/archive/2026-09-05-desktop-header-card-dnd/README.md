# desktop-header-card-dnd

Cambio OpenSpec para eliminar la línea de título del desktop, agregar
drag-and-drop de cards hacia colecciones, reducir el espacio vertical vacío,
posicionar la ventana principal centrada arriba al iniciar y reemplazar el
icono de estrella de favoritos por un pin.

Estado: implementado y auditado. La auditoría de regresión está
documentada en `regression-audit.md`.

El drag-and-drop general en macOS fue confirmado previamente. La prueba
manual específica del inicio desde el título debe ejecutarse después de
reconstruir el binario; la verificación manual independiente de
`platform-permission-guidance` no forma parte de este cambio.

No archivar este cambio al finalizar sin una instrucción explícita.
