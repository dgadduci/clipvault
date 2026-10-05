# Tareas: toolchain Node.js 24 LTS y release 0.0.19

## 1. Relevamiento

- [x] 1.1 Confirmar que los manifiestos y `.nvmrc` declaran Node 20 / npm 10,
  que el lockfile contiene `svelte-awesome-color-picker@4.1.3` con requisito
  Node `>=24`, y que el borrador `v0.0.18` no incluye el `main` actual.
- [x] 1.2 Confirmar en la fuente oficial de Node.js que Node 20 está EOL y
  Node 24 está en LTS para la fecha de este cambio.

## 2. Alinear toolchain frontend

- [x] 2.1 Cambiar `.nvmrc` a Node 24 LTS y actualizar los rangos `engines` de
  Node y npm en `package.json` al baseline Node 24 / npm 11.
- [x] 2.2 Actualizar los metadatos raíz de `package-lock.json` sin modificar
  las versiones bloqueadas de dependencias.
- [x] 2.3 Actualizar `docs/development.md` y la especificación canónica
  `repository-reproducibility` para Node 24 LTS / npm 11.
- [x] 2.4 Confirmar que el workflow de release lee `.nvmrc` y no mantiene otra
  versión de Node hardcodeada.
- [x] 2.5 Mover las pruebas transpiladas fuera de `node_modules`, actualizar
  el glob del runner e ignorar el directorio generado para que Node 24 ejecute
  la suite.
- [x] 2.6 Actualizar las aserciones obsoletas detectadas en 21 archivos para
  comprobar las claves localizadas, estructuras dinámicas y comportamientos
  vigentes sin reducir cobertura.
- [x] 2.7 Hacer que `formatElapsedTime` use el texto localizado
  `time.just_captured` para timestamps futuros; actualizar los cinco catálogos
  y conservar `Intl.RelativeTimeFormat` para los intervalos restantes.

## 3. Preparar versión 0.0.19

- [x] 3.1 Alinear la versión `0.0.19` en Cargo, `Cargo.lock`, Tauri, frontend,
  `package-lock.json` y la tabla/versionado de `projects.md`; documentar el
  bump `0.0.18` → `0.0.19`.
- [x] 3.2 Confirmar que ningún tag existente se mueve y que `v0.0.19` apunta a
  un commit alcanzable desde `main` después de integrar las validaciones. El
  tag anotado y publicado apunta a `336887e`; antes de crearlo no existía el
  tag remoto y `main` contiene ese commit.

## 4. Verificación

- [x] 4.1 Resolver el advisory alto de `devalue` mediante una actualización
  compatible dentro del rango existente, sin `npm audit fix --force`; repetir
  `npm audit --omit=dev` y confirmar que el build cliente no contiene el código
  de serialización del módulo Svelte `internal/server`. El lockfile ahora usa
  `devalue@5.9.4`; la auditoría de producción reportó cero vulnerabilidades y
  la búsqueda en `dist/assets/*.js` no encontró `devalue`, `uneval` ni
  `internal/server`.
- [x] 4.2 En Node 24 LTS / npm 11 ejecutar `npm ci`, `npm run check`,
  `npm run build` y `npm test`; confirmar que no aparece `EBADENGINE` y que el
  runner reporta y aprueba toda la suite compilada. Verificado con Node
  `v24.21.0` / npm `11.19.0`: `npm ci` sin `EBADENGINE`, `check` con cero
  errores (18 warnings existentes), build correcto y `npm test` con 1.569
  pruebas aprobadas en 102 archivos.
- [x] 4.3 Clasificar los cuatro findings restantes como dependencias de
  build/desarrollo: Vite (un aviso alto y dos moderados incluidos en su
  rango afectado `<=6.4.2`), esbuild (moderado, `<=0.24.2`),
  `@sveltejs/vite-plugin-svelte` (moderado, `<=5.0.0`) y
  `@sveltejs/vite-plugin-svelte-inspector` (moderado, `<=3.0.1`). El audit
  propone Vite 8.3.2 y plugin Svelte 7.3.1, saltos mayores fuera del alcance;
  no se actualizan esas dependencias en este cambio.
- [x] 4.4 Validar la consistencia de versiones, lockfiles, formatos y
  `openspec validate desktop-release-node24-toolchain --strict`. Confirmado
  con `cargo metadata --locked --no-deps`, manifests/lockfiles en `0.0.19`,
  `npm run locales:check` y validación OpenSpec estricta.
- [x] 4.5 El usuario confirmó que el build de producción `deb,appimage` y la
  acción de actualización en “Acerca de” pasaron en Ubuntu. En el host Arch de
  desarrollo, `linuxdeploy` no pudo procesar secciones ELF `.relr.dyn`; esa
  limitación no afectó la prueba realizada en Ubuntu.
- [x] 4.6 El usuario confirmó que el build de producción `app,dmg` y la acción
  de actualización en “Acerca de” pasaron en macOS.
- [x] 4.7 Revisar el diff y `git diff --check`; no hay archivos generados ni
  secretos en el cambio. `test-build/` y `dist/` permanecen ignorados.

## 5. Candidato y publicación

- [x] 5.1 Integrar y subir el candidato validado a `main` en el commit
  `f8b0d96` (`build: prepare v0.0.19 release candidate`).
- [x] 5.2 Crear y subir el tag anotado `v0.0.19`; inició el workflow
  `desktop-release.yml` (run `37370613055`) para generar un borrador, sin
  publicar automáticamente.
- [x] 5.3 Revisar el run `37370613055` y los artefactos del borrador
  `v0.0.19`: todos los jobs pasaron; están los `.dmg` y `.app.tar.gz` de
  macOS x64/arm64, `.deb` y AppImage de Linux, cuatro sidecars `.sig` y
  `latest.json`. Las siete entradas del manifiesto usan los assets correctos
  y cada firma coincide con su sidecar; los `.dmg` no son paquetes de updater.
- [ ] 5.4 Publicar el borrador sólo tras la revisión explícita del usuario.
