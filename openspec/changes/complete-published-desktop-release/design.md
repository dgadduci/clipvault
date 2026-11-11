# Diseño: completar un release desktop publicado con Linux

## Flujo

1. Una ejecución normal iniciada por un tag crea el mismo borrador y construye
   macOS Apple Silicon, macOS Intel y Linux.
2. Una ejecución manual recibe un tag estable. CI carga el historial, resuelve
   ese tag, valida las versiones canónicas y comprueba que su commit pertenece
   a `main`.
3. Antes de compilar, GitHub API confirma que el release para ese tag existe,
   está publicado y no es preliminar. La matriz manual contiene solo Linux
   x86_64.
4. Tauri Action carga AppImage, DEB y RPM en el release existente sin volver a
   crear un borrador. La acción obtiene el `latest.json` ya publicado, conserva
   sus plataformas y añade los artefactos Linux firmados.
5. La descripción del release se actualiza para enumerar las plataformas y
   paquetes disponibles.

## Decisiones

- Se reutiliza el workflow existente mediante `workflow_dispatch`, no se
  introduce un workflow de empaquetado paralelo.
- El selector manual recibe un tag explícito; la matriz se limita a Linux para
  evitar recompilar o reemplazar activos macOS ya publicados.
- La ejecución manual exige un release estable preexistente. La ejecución por
  tag mantiene `releaseDraft: true`; la ejecución manual usa
  `releaseDraft: false` para cumplir el estado del release existente.
- Se mantiene `uploadUpdaterJson` y `uploadUpdaterSignatures`: la acción
  fusiona las entradas del nuevo target con el manifiesto existente.
- La subida de artefactos usa los secretos y variables de firma ya requeridos
  por el job de validación.

## Riesgos y mitigaciones

- Una etiqueta que no corresponda al release publicado falla antes de compilar
  por validación de SemVer, versiones, ascendencia y estado del release.
- El action actualiza la descripción del release; se configura un texto que
  describe macOS y Linux en vez de las notas de borrador anteriores.
- El job Linux sigue dependiendo de los paquetes apt y del runner Ubuntu
  declarados por el workflow; sus errores se mostrarán en la ejecución manual.

## Verificación

- Validar el workflow y el diff antes de publicarlo.
- Ejecutar manualmente el workflow para `v0.0.22`.
- Verificar en el release los paquetes Linux y confirmar que `latest.json`
  contiene las entradas macOS y Linux.
