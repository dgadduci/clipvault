# Propuesta: estabilidad de presencia de pares locales

## Problema

El TTL local del core fue eliminado, pero el adaptador agregó una reconfirmación
periódica con `ServiceDaemon::verify`. En pruebas macOS ↔ Linux, ambos peers
siguen con TLS e historial funcionales, pero tras uno o dos minutos un verify
vence sin respuesta y `mdns-sd` emite `ServiceRemoved`. La UI marca ausente a
un peer que sigue activo.

## Objetivo

Conservar la presencia mientras el browser mDNS no informe una baja real.
Mantener goodbye y expiración natural de caché, sin un verify periódico que
fuerce el vaciado de una caché sana pero silenciosa.

## Alcance

- Retirar el scheduler privado y `ServiceDaemon::verify` periódico.
- Conservar `ServiceResolved` → `Observed`, `ServiceRemoved` → `Removed`, el
  cleanup de endpoint efímero y el cierre ordenado.
- Eliminar el código y tests exclusivos de la máquina de estado de verify.

## Fuera de alcance

Polling TCP/TLS, pairing, historial, UI, persistencia, clipboard, escaneo de
red, IP manual y broadcast propio.

## Criterio de aceptación

Dos peers macOS/Linux anunciados y activos no pasan a No disponible por tiempo
ni por falta de respuesta a un query forzado. Goodbye o expiración que el
browser reporte como `ServiceRemoved` sigue retirando la presencia.
