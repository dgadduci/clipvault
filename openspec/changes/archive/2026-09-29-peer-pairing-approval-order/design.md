# Diseño: aprobación local y remota independiente

## Flujo de aprobación

Las aprobaciones local y remota son dos hechos independientes de una misma
sesión. La aprobación remota actualiza el estado informativo de la sesión, pero
no sustituye la acción local ni debe bloquearla. El botón **Aceptar** depende
de la sesión, de si esta instancia local ya aprobó y de si hay una llamada de
aprobación en curso.

El core marcará internamente la señalización local como pendiente, sin
establecer aún `local_approved`, antes de llamar al transporte. Así, una
observación remota concurrente puede registrar `remote_approved` sin promover
la relación antes de confirmar el envío local. Al tener éxito el transporte,
el core establecerá `local_approved`; si `remote_approved` ya estaba marcado,
promoverá la relación en ese momento. Si el transporte falla, limpiará sólo el
estado pendiente y conservará la aprobación remota para que el usuario pueda
reintentar.

La promoción se extraerá a una ruta compartida invocada cuando se registre la
aprobación remota y cuando termine correctamente la aprobación local. Esto
cubre ambos órdenes y evita depender de una segunda notificación de red. La
ruta reclama la promoción bajo el lock de la sesión para que dos observaciones
concurrentes no persistan dos veces el vínculo ni roten dos veces el secreto
del cursor.

## Presentación del estado

El modal procesará el resultado de `approve_local` igual que procesa el
resultado de inicio. Una variante `Failed` se mostrará como error junto al SAS,
sin ocultarse bajo el estado “Esperando aprobación”. Al revertirse el estado
local fallido, **Aceptar** quedará habilitado para permitir un reintento.

## Verificación

- UI: la llegada de `remote_approved` no deshabilita **Aceptar** cuando
  `local_approved` aún es falso; una aprobación local sí lo deshabilita.
- UI: una respuesta tipada de fallo se muestra como error en la sesión.
- Core: las pruebas cubren ambas secuencias (local→remota y remota→local),
  con promoción inmediata cuando se registra la segunda aprobación.
- Core: un fallo de transporte no deja `local_approved` establecido, conserva
  la aprobación remota y permite aprobar la misma sesión al reintentar.
- Mantener los tests existentes de sesión entrante, aprobación mutua,
  cancelación, timeout y transporte autenticado.
