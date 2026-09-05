# clipboard-rich-text

Este cambio agrega soporte para texto enriquecido en las capturas que hoy se
guardan como texto plano. La card seguirá usando la misma rail horizontal y el
mismo tamaño fijo ya implementados, pero su preview podrá mostrar formato
enriquecido de forma segura.

## Decisiones de producto

- La card muestra una representación enriquecida segura, recortada al área de
  la card. El recorte nunca modifica el contenido almacenado.
- `Paste de texto enriquecido` intenta conservar las representaciones HTML y
  RTF originales disponibles en el clipboard.
- `Paste de texto plano` escribe únicamente el texto plano canónico.
- Si el destino no soporta escritura enriquecida, el modo enriquecido puede
  degradar a texto plano y debe devolver un resultado tipado indicando el
  fallback.
- Una captura textual con al menos una representación enriquecida y texto
  plano se persiste como `RichText`; una captura sólo de texto continúa siendo
  `Text`.
- El texto plano permanece en la columna `content` para compatibilidad,
  búsqueda, snippets y fallback.

## Supuestos adoptados

- Se preservan HTML y RTF cuando el backend del sistema los expone. No se
  agrega una conversión artificial de un formato al otro.
- La vista de la card usa un HTML sanitizado y acotado generado localmente. No
  se renderiza HTML crudo del clipboard.
- El alcance cubre formato textual: fuente, tamaño, color, peso, cursiva,
  subrayado, tachado, listas, alineación, párrafos, código y saltos de línea.
  Imágenes embebidas, archivos adjuntos, objetos OLE, audio, video y contenido
  remoto siguen fuera de alcance.
- La compatibilidad que no exista en macOS, Linux X11 o Linux Wayland se
  modela como capacidad no disponible; no se inventan permisos ni se rompe la
  captura de texto plano.

## Fuera de alcance

No incluye editor enriquecido, favoritos nuevos, borrado nuevo, retención
nueva, sincronización, red, telemetría, embeddings, conversión automática de
documentos ni cambios al modelo de quick-paste fuera del comportamiento de
pegado existente.

MiniMax debe implementar únicamente las tareas de `tasks.md`. Si una API
nativa o una dependencia necesaria contradice estas decisiones, debe pausar y
actualizar primero `design.md` y las deltas correspondientes.
