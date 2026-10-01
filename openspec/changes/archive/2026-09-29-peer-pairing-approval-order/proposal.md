# Propuesta: aprobación de vínculo independiente del orden

## Why

El modal deshabilita **Aceptar** tanto cuando esta instancia ya aprobó como
cuando recibió la aprobación remota. Si un equipo acepta primero, el otro
puede recibir esa aprobación antes de que su usuario acepte. Su botón queda
deshabilitado. Además, el core sólo promueve la relación cuando llega una
aprobación remota; si esa aprobación ya llegó antes de la aprobación local,
el estado puede quedar esperando aunque el segundo usuario consiga aceptar.

Además, `approve_local` conserva `local_approved` si falla la señal al
transporte. El modal prioriza el estado de sesión y no interpreta de forma
consistente una respuesta tipada `Failed`, de modo que un error puede verse
como una espera indefinida.

Completar el vínculo cuando ambos usuarios aprueban el mismo SAS en cualquier
orden, y presentar los fallos locales de aprobación como errores recuperables
sin marcar el vínculo como confiable.

## What Changes

- Mantener **Aceptar** disponible mientras la instancia local no haya aprobado,
  aunque ya haya llegado la aprobación remota.
- Promover la relación cuando la segunda aprobación se registra, sin depender
  de que llegue otra observación remota.
- Si el transporte rechaza la aprobación local, dejarla sin registrar y
  permitir reintentar.
- Mostrar respuestas tipadas de fallo en el modal en lugar de indicar que se
  espera al otro equipo.
- Añadir regresiones de UI y core para orden inverso, fallo y reintento.

## Fuera de alcance

- Cambiar el protocolo, el SAS, la autenticación mTLS, discovery o persistencia
  de identidad.
- Autoaprobar una solicitud, aceptar códigos distintos o cambiar la política
  de confianza mutua.
