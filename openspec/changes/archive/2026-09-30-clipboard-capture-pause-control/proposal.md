# Proposal: clipboard-capture-pause-control

## Contexto

ClipVault captura cambios locales del portapapeles en segundo plano. Hoy no
existe un control único para detener y restaurar esa captura desde el teclado,
Configuración general y el icono del sistema. La captura continúa mientras la
aplicación permanece abierta en segundo plano.

## Objetivo

Permitir que el usuario pause y reanude la captura local del portapapeles en
cualquier momento. El estado debe persistir entre reinicios y poder cambiarse
desde un atajo global (`Cmd+Option+Shift+B` en macOS,
`Ctrl+Alt+Shift+B` en Linux), desde un control de Configuración general y desde
el menú del icono tray/menu bar.

## Alcance

- Persistir un único estado local de captura, habilitado de forma predeterminada
  para conservar el comportamiento actual.
- Detener el almacenamiento de nuevas capturas locales de texto e imágenes
  mientras la función está pausada, sin borrar ni alterar el historial previo.
- No registrar posteriormente el contenido que quedó en el portapapeles durante
  la pausa sólo por haber reanudado la captura.
- Mostrar y cambiar el estado desde Configuración general y el menú del icono
  del sistema; ambas superficies deben reflejar el mismo estado persistido.
- Registrar el atajo global solicitado y mostrar su etiqueta en ambas
  superficies.
- En sesiones Wayland GNOME y KDE, registrar el atajo en la integración nativa
  existente del escritorio; indicar en Configuración general cuando esa
  integración deba habilitarse para disponer del atajo.

## Fuera de alcance

- Borrar, expirar, editar o modificar capturas ya almacenadas.
- Cambiar los atajos existentes de búsqueda rápida o pegado.
- Añadir configuración para personalizar la combinación de teclas.
- Pausar la recepción o importación de capturas recibidas de equipos
  vinculados; este control afecta sólo la captura local del portapapeles.
- Añadir telemetría, servicios de red, nube o dependencias nuevas.

## Criterio de aceptación

El usuario puede pausar y reanudar la captura desde el atajo global, el control
de Configuración general o el menú tray/menu bar. Cada cambio se refleja en las
tres superficies, sobrevive al reinicio y deja intactas las capturas existentes.
Al reanudar, el valor actual del portapapeles no se añade por sí solo al
historial; se capturan los cambios locales posteriores.
