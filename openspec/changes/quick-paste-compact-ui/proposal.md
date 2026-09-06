# Proposal: quick-paste-compact-ui

## Contexto

Quick Paste ya funciona como ventana transient abierta por
`Cmd/Ctrl+Shift+V`, con búsqueda local, navegación y pegado en la aplicación
previamente activa. Su presentación actual se parece demasiado a una ventana
principal: tiene un encabezado grande, tipografía amplia y filas que no
establecen una geometría compacta y homogénea.

La ventana debe servir para el flujo de alta frecuencia:

```text
abrir → escribir o navegar → seleccionar → pegar → volver al target
```

## Objetivos

- Fijar la ventana en `720 × 520` píxeles lógicos y centrarla al abrir.
- Mantener una lista vertical con scroll y sin scroll horizontal.
- Dar a cada resultado una geometría fija, homogénea y predecible.
- Mostrar cada resultado en dos líneas compactas con título, preview o
  miniatura, tipo, aplicación fuente y tiempo.
- Reducir la tipografía y eliminar la apariencia de segundo desktop.
- Enfocar automáticamente la búsqueda al abrir la ventana.
- Conservar búsqueda, selección, Escape, foco del target, pegado, guidance,
  privacidad y listeners idempotentes existentes.

## Fuera de alcance

- Favoritos, orden especial de favoritos y pin directo; pertenecen a
  `quick-paste-actions`.
- Nuevas modalidades de pegado; pertenecen a `quick-paste-actions`.
- Filtros de colecciones, tags, workspaces o aplicaciones.
- Cambios en SQLite, en el modelo de capturas o en la captura del clipboard.
- Nueva API de foco o implementación paralela del hotkey.
- Red, telemetría, embeddings o dependencias nuevas.
