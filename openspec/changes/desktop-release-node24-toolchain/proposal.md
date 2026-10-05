# Propuesta: preparar ClipVault 0.0.19 con Node.js 24 LTS

## Why

El candidato `0.0.19` debe compilar con una toolchain de frontend que cumpla
los requisitos de las dependencias bloqueadas y siga recibiendo actualizaciones
de seguridad. También debe partir del `main` que el usuario probó; reutilizar el
borrador `0.0.18` publicaría código anterior.

## Problema

El repositorio fija Node.js 20 y npm 10 en `.nvmrc`, `package.json` y la
documentación. Esa línea de Node terminó su soporte el 2026-03-24, mientras que
`svelte-awesome-color-picker@4.1.3`, bloqueado en `package-lock.json`, exige
Node.js `>=24`. Por eso `npm ci` en Ubuntu con Node 20 emite `EBADENGINE` aunque
el comando termine correctamente.

El código validado en `main` contiene cambios posteriores al commit de la
etiqueta `v0.0.18`. Esa versión sigue como borrador y la última release
publicada es `v0.0.17`. Publicar el borrador `v0.0.18` distribuiría el código
anterior a los cambios probados. El siguiente candidato debe ser `v0.0.19` y
debe compilarse desde `main` con las versiones canónicas alineadas.

## What Changes

- Actualizar la toolchain frontend a Node.js 24 LTS y npm 11, que cumplen el
  requisito de la dependencia bloqueada y usan una línea de Node con soporte.
- Alinear `.nvmrc`, `engines`, `package-lock.json`, el workflow de release y la
  documentación con esa toolchain.
- Actualizar la especificación canónica de reproducibilidad para que los
  builds locales y CI usen la misma versión.
- Corregir la ubicación e invocación de las pruebas compiladas para que Node
  24 descubra y ejecute la suite, en vez de aceptar una ejecución vacía.
- Alinear las aserciones estáticas de la suite con las claves de traducción y
  la estructura actual de los componentes, manteniendo las comprobaciones de
  comportamiento. Corregir la etiqueta de captura futura, cuyo comentario y
  prueba actuales no coinciden con el resultado de `Intl`.
- Preparar el candidato funcional `0.0.19` y alinear Cargo, Tauri, frontend,
  lockfiles y `projects.md`.
- Validar la instalación npm, los checks frontend y los builds locales de
  producción en Ubuntu y macOS antes de crear la etiqueta.

La implementación de este cambio no crea etiquetas ni publica releases. El
workflow existente genera un borrador cuando se empuja una etiqueta SemVer; el
borrador se revisa antes de que el usuario solicite su publicación.

## Out of Scope

- Publicar o reemplazar el borrador `v0.0.18`.
- Cambiar el comportamiento del updater, sus claves, el endpoint o los permisos
  de firma.
- Hacer upgrades mayores de Vite o del plugin Svelte como parte del cambio, ni
  ejecutar `npm audit fix --force`.
- Ignorar vulnerabilidades altas de dependencias que entren en el bundle de
  producción; cualquier advisory restante debe quedar clasificado por alcance.
- Crear instaladores desde una computadora local y distribuirlos fuera del
  workflow firmado de GitHub Actions.

## Expected Outcome

Una instalación limpia de frontend con Node.js 24 LTS y npm 11 no muestra
`EBADENGINE`; `npm run check`, `npm run build` y `npm test` pasan con la suite
compilada realmente ejecutada y las expectativas alineadas con la interfaz
localizada actual; las versiones canónicas indican `0.0.19`; y
los builds locales de producción completan en Ubuntu y macOS. Tras integrar y
validar ese commit, la etiqueta `v0.0.19` produce un borrador nuevo con
instaladores, firmas y `latest.json` para revisión.
