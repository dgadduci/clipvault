# Proposal: quick-paste-actions

## Contexto

Quick Paste ya permite abrir una ventana compacta, buscar capturas y
seleccionarlas con teclado. La siguiente etapa debe agregar favoritos y
acciones por item, pero también debe conservar el comportamiento que el
usuario considera estable para el teclado: confirmar con `Enter` prepara la
captura en el portapapeles y permite que el usuario decida cuándo ejecutar
`Cmd/Ctrl+V`.

La implementación experimental del pegado sintético introdujo una regresión:
la captura sí quedaba disponible en el portapapeles, pero el evento automático
no llegaba de forma confiable a la aplicación anterior. Por eso el flujo de
teclado se define explícitamente como copy-only y se separa del flujo directo
de las acciones del menú.

## Objetivos

- Mostrar y alternar un pin en todas las filas.
- Priorizar favoritos sin romper el ranking de búsqueda.
- Agregar un menú `...` con acciones válidas por representación.
- Hacer que `Enter` y `Shift+Enter` escriban al portapapeles sin pegado
  sintético.
- Mantener la representación escrita disponible para un `Cmd/Ctrl+V` posterior.
- Evitar nuevas cards, pérdida de imágenes, cambios de tags/colecciones o
  listeners duplicados.

## Fuera de alcance

- Reescribir el layout fijo de `quick-paste-compact-ui`.
- Cambiar el hotkey o crear otra ventana.
- Implementar filtros de colecciones, tags, workspaces o aplicaciones.
- Cambiar el modelo de favoritos existente.
- Red, telemetría, embeddings o dependencias nuevas.
