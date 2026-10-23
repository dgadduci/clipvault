# Diseño: revisiones Wayland según el transporte activo

## Causa observada

La sesión actual es Ubuntu GNOME sobre Wayland. El binario Tauri habilita
`arboard/wayland-data-control`. En el código de `arboard 3.6.1`, la creación
del cliente primero intenta conectar con `ext-data-control-v1` o
`wlr-data-control-unstable-v1`; si esa conexión no está disponible, cae a su
cliente X11. ClipVault hoy clasifica cualquier sesión Wayland como revisión
desconocida, incluso cuando el backend efectivo de `arboard` es X11.

Después de borrar una fila, `baseline_dedupe_state` no puede rebaselinar con
`UNKNOWN`. El siguiente poll idéntico conserva la revisión y el fingerprint
anterior, así que el watcher lo clasifica correctamente como el contenido que
quedó en el portapapeles, aunque el usuario ya lo haya copiado otra vez. El
mismo camino se usa para texto e imágenes.

## Decisión

`RevisionSource::for_session` resolverá conjuntamente el origen de revisión y
el identificador de transporte:

1. En X11, conservar el monitor XFixes existente.
2. En Wayland con `arboard-wayland-data-control`:
   - si se publica un protocolo data-control compatible, iniciar un monitor
     Wayland metadata-only y clasificar la ruta como nativa;
   - si no se publica ninguno, usar XFixes si está disponible, porque en ese
     caso `arboard` también cae a X11/XWayland;
   - si la conexión o el monitor falla de forma inesperada, devolver
     `UNKNOWN`; no atribuir al fallback un transporte que no se pudo verificar.
3. En Wayland sin la feature data-control, usar XFixes porque `arboard` usa
   X11 directamente.
4. En sesión desconocida, o sin el monitor correspondiente, conservar
   `UNKNOWN`.

El monitor nativo escuchará `selection` en el dispositivo data-control del
portapapeles regular. La primera notificación que Wayland envía al crear el
dispositivo solo establece el estado inicial; las notificaciones posteriores
incrementan un contador local monotónico. El monitor no lee ofertas, MIME types
ni bytes del portapapeles. El protocolo documenta que `selection` anuncia una
nueva selección y que la primera notificación se envía al asociar el
dispositivo.

El event queue se atiende en un hilo dedicado y mantiene un contador atómico
metadata-only. Si el hilo pierde la conexión, la revisión pasa a `UNKNOWN`.
Esto evita usar un valor viejo para autorizar una recaptura.

## Dependencias

`wayland-client`, `wayland-protocols` y `wayland-protocols-wlr` se declararán
como dependencias opcionales directas de `clipvault-platform`, habilitadas por
`arboard-wayland-data-control`. Ya están en el lockfile como dependencias
transitivas de `wl-clipboard-rs`, por lo que esta integración no agrega código
de terceros al build. Se necesitan para suscribirse al mismo protocolo que
`arboard` usa sin exponer la selección a la lógica de negocio.

Alternativas descartadas:

- Usar XFixes siempre en una sesión Wayland: cuando `arboard` realmente está
  en data-control nativo, XWayland puede observar otra selección y su revisión
  no representa el payload leído.
- Usar solo fingerprints: una copia idéntica no cambia el fingerprint y es
  indistinguible de un poll ordinario.
- Limpiar el portapapeles al borrar: cambiaría datos del usuario y no es
  necesario para resolver la señal faltante.

## Semántica común de captura

La revisión no depende del tipo de payload. Tras borrar una fila, el watcher
ignora la selección que permanece en la misma revisión. Una nueva selección
con el mismo texto, rich text o imagen avanza la revisión; la persistencia crea
una fila nueva si la anterior fue eliminada y continúa deduplicando contra una
fila viva.

## Verificación

- Pruebas unitarias del monitor: la notificación inicial no incrementa la
  revisión, las siguientes notificaciones de selección sí, y los eventos de
  selección primaria no alteran la revisión del portapapeles regular.
- Pruebas del selector: data-control disponible elige la revisión Wayland;
  protocolo ausente elige XFixes; fallos no confiables eligen `UNKNOWN`.
- Regresión end-to-end de borrado y recaptura con el mismo texto y una misma
  imagen, incluyendo el poll estable que no debe recrear la fila.
- Tests Rust relevantes, formato, validación estricta de OpenSpec y revisión
  del diff.
- La validación en compositores reales Wayland queda sujeta a la prueba manual
  del usuario, porque los protocolos publicados dependen del compositor.
