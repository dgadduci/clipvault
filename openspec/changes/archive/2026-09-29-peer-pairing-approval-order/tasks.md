# Tareas: aprobación de vínculo independiente del orden

Codex implementa este cambio por solicitud explícita del usuario. No archivar,
sincronizar las specs canónicas ni commitear automáticamente.

## 1. Regresiones

- [x] 1.1 Añadir prueba frontend que mantenga **Aceptar** activo cuando
  `remote_approved` es verdadero y `local_approved` es falso, y lo desactive
  después de la aprobación local.
- [x] 1.2 Añadir prueba frontend que muestre una respuesta tipada de fallo
  durante una sesión activa.
- [x] 1.3 Añadir prueba core para fallo del transporte: no conservar
  `local_approved`, preservar el estado remoto y permitir reintentar.
- [x] 1.4 Añadir prueba core para aprobación remota primero: la aprobación
  local posterior debe completar la promoción sin otra observación de red.

## 2. Corrección

- [x] 2.1 Hacer que el botón dependa de la aprobación local de esta instancia,
  no de `remote_approved`.
- [x] 2.2 Mantener un estado local pendiente durante el envío; confirmar
  `local_approved` sólo tras éxito y limpiar lo pendiente si falla, sin perder
  una aprobación remota concurrente.
- [x] 2.3 Promover al registrar la segunda aprobación, tanto desde la ruta
  remota como desde la ruta local, usando la huella certificada de la sesión.
- [x] 2.4 Procesar los resultados tipados de `approve_local` y mostrar los
  errores en el diálogo sin presentar una espera indefinida.

## 3. Verificación

- [x] 3.1 Ejecutar los tests relevantes del core y frontend, checks/build
  afectados, `openspec validate peer-pairing-approval-order --strict --type
  change` y `git diff --check`.
- [x] 3.2 Revisar el diff y confirmar que el cambio queda limitado a pairing.
- [x] 3.3 Prueba manual aprobada en Arch, macOS y Ubuntu: los equipos se
  detectan y envían/reciben capturas recíprocamente.
