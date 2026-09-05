# card-title-editing-regression

Este cambio corrige la regresión por la que el título de una card ya no se
puede editar con doble click y restaura la opción `Editar título` del menú de
la card.

La solución debe reutilizar el editor inline, la validación y el comando de
títulos existentes. También debe arbitrar correctamente entre doble click y
drag-and-drop: un gesto que no supera el umbral debe llegar al título, y un
gesto que sí lo supera debe iniciar el drag protegido actual.

Estado: propuesto, pendiente de implementación.

No archivar ni sincronizar este cambio hasta completar la implementación y la
verificación manual. No modificar ni marcar la verificación pendiente de
`platform-permission-guidance`.
