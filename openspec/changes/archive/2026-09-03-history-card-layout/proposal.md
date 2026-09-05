## Why

El historial actual presenta las capturas como una lista vertical, con
tipografía demasiado grande y sin una jerarquía visual que permita reconocer
rápidamente el tipo de contenido o la aplicación de origen. Esto dificulta
explorar varias capturas y no aprovecha la metadata que ClipVault ya conoce.

La interfaz debe acercarse al patrón visual de una galería de clipboard:
cards compactas, scroll horizontal, preview truncado, iconos, título y acciones
discretas. Las imágenes adjuntas a la solicitud se usan como referencia visual:
la primera representa el estado actual y la segunda el objetivo deseado.

## What Changes

- Convertir el listado principal de capturas recientes en una rail horizontal.
- Renderizar cada captura como una card cuadrada de tamaño fijo y bordes redondeados.
- Reducir la tipografía del preview y truncar contenido largo de forma segura.
- Mostrar arriba el icono y label del tipo, y el icono y nombre de la aplicación fuente.
- Generar un set local de iconos SVG para los tipos detectados.
- Obtener y cachear el icono de la aplicación fuente al capturar, si está disponible.
- Agregar título opcional editable; por defecto se muestra el label del tipo.
- Mover las acciones visuales a una fila inferior: pin/unpin y menú de puntos suspensivos.
- Mantener las confirmaciones y semántica existentes de eliminación y favoritos.

## Capabilities

### New Capability

- clipboard-history-cards: presentación visual y metadata de las cards del historial.

### Modified Capability

- clipboard-text-history: una captura permitida puede enriquecer su metadata
  con nombre e icono de la aplicación fuente sin modificar el payload de texto.

## Non-Goals

- No capturar, almacenar ni pegar imágenes del portapapeles.
- No implementar HTML, archivos, audio u otros payloads binarios.
- No cambiar el ranking ni el contrato de búsqueda.
- No cambiar el flujo de quick-paste ni convertir una card en una acción de pegado nueva.
- No rediseñar la configuración de privacidad o el selector de blacklist.
- No cambiar las reglas de PrivacyGate, deduplicación, retención o confirmación.
- No agregar red, telemetría, una librería de iconos externa ni un framework nuevo.
