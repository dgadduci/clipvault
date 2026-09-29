# Tareas: reenviar atribución de aplicación por procedencia

## 1. Consulta de procedencia

- [x] 1.1 Añadir consulta por lote que seleccione, por entrada, la procedencia
  más reciente con nombre o icono; hacer el desempate determinista y no
  devolver peer_id.
- [x] 1.2 Añadir pruebas para orden, empates, procedencias sin metadata y
  aislamiento entre entradas.

## 2. Historial remoto

- [x] 2.1 Resolver nombre efectivo para páginas de historial de texto con
  metadata local prioritaria y procedencia importada como fallback.
- [x] 2.2 Aplicar la misma resolución a páginas de historial de imagen.
- [x] 2.3 Cubrir regresiones de nombre propagado y ausencia de icono/ref en
  filas remotas.

## 3. Importación explícita

- [x] 3.1 Reenviar nombre e icono validado en la importación de texto usando la
  procedencia cuando la fila no tenga metadata local.
- [x] 3.2 Reenviar nombre e icono validado en la importación de imagen con la
  misma regla.
- [x] 3.3 Probar precedencia de metadata local, reenvío de metadata importada,
  fallback desconocido y que no se sobrescriba `clipboard_entries.source_app*`.

## 4. Verificación

- [x] 4.1 Ejecutar tests relevantes de db/core, formato y checks/build afectados;
  validar OpenSpec estrictamente y ejecutar `git diff --check`.
- [x] 4.2 Revisar el diff y confirmar que no expone peer_id, rutas o
  identificadores de aplicación y no modifica cambios ajenos.
- [x] 4.3 Prueba manual multi-hop: B captura y comparte con A; C explora A y ve
  el nombre de origen; después importa y confirma nombre e icono originales.
