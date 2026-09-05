# Proposal: clipboard-rich-text

## Problema

ClipVault actualmente lee y persiste la vista plana de una captura textual.
Cuando el clipboard contiene HTML o RTF, se pierde la fuente, color, peso,
cursiva, subrayado, listas, alineación y demás atributos que el sistema ofrece.
Además, la card sólo puede mostrar un preview monoespaciado y el menú no
permite elegir entre pegar la representación enriquecida o la plana.

## Resultado esperado

Una captura textual conserva el texto plano para búsqueda y compatibilidad, y
también conserva las representaciones enriquecidas disponibles. La card
renderiza una vista segura y recortada del formato. Desde su menú el usuario
puede seleccionar `Paste de texto enriquecido` o `Paste de texto plano`.

## Alcance

1. Extender el contrato neutral del clipboard sin acoplar el core a Tauri,
   Svelte, AppKit, X11 o Wayland.
2. Detectar de manera determinística las representaciones textuales y aplicar
   prioridad `RichText > Text > Image > unsupported`.
3. Persistir las representaciones originales en assets locales con referencias
   relativas y una preview sanitizada separada.
4. Extender `EntryRecord`, SQLite, deduplicación, borrado y retención para
   mantener la coherencia del ciclo de vida de los assets.
5. Agregar modos de pegado y capacidades tipadas de lectura/escritura rich.
6. Mostrar el preview enriquecido en la `HistoryCard` existente y agregar las
   dos acciones del menú.

## Decisiones tomadas

- El hash de texto plano (`content_hash`) se conserva para compatibilidad y
  búsqueda. Las filas enriquecidas tendrán además un `rich_text_hash` de la
  representación canónica; la deduplicación compara ambos cuando existe el
  segundo, para no descartar silenciosamente dos estilos distintos con el
  mismo texto plano.
- Los bytes originales no se colocan en `EntryRecord`, eventos Tauri ni logs.
  `EntryRecord` sólo devuelve referencias relativas y metadatos.
- La preview se sanitiza en Rust antes de almacenarse. La allow-list debe
  eliminar scripts, handlers de eventos, iframes, formularios, recursos
  remotos, URLs peligrosas y objetos embebidos.
- Las cards sin representación rich mantienen el preview plano y muestran la
  acción enriquecida deshabilitada con nombre accesible. La acción plana sigue
  disponible.
- Si un paste rich no puede escribirse pero el texto plano sí, el resultado es
  éxito con fallback explícito; si tampoco puede escribirse plano, se conserva
  el flujo tipado de capacidades y guidance actual.

## No objetivos

No se implementan nuevos tipos de archivo, imágenes embebidas dentro de HTML o
RTF, editor, undo/redo, sincronización, red, telemetría, favoritos, borrado,
retención ni reglas de seguridad nuevas.

## Criterios de aceptación

- Una captura desde una fuente que expone rich text muestra formato en la card
  y conserva el contenido tras reiniciar.
- El menú puede pegar la misma captura como rich o como plain.
- Una fuente o sesión sin soporte rich continúa capturando y pegando texto
  plano.
- Las referencias de assets son locales, relativas, validadas y limpiables.
- La captura blacklisted no crea fila, preview ni asset.
- Los tests de Rust, frontend, build y validación OpenSpec pasan; la prueba
  visual real en macOS queda documentada en `tasks.md`.
