# Diseño: toolchain Node.js 24 LTS y candidato 0.0.19

## Decisiones

- Usar Node.js 24 LTS y npm 11 para el frontend. Node.js 20 llegó al fin de
  soporte el 2026-03-24 y la versión bloqueada de
  `svelte-awesome-color-picker` declara `node >=24`.
- Mantener una sola fuente de selección: `.nvmrc` contiene `24`; el workflow
  `desktop-release.yml` ya lee ese archivo mediante `actions/setup-node`.
- Declarar en `package.json` un rango compatible con el baseline (`node >=24
  <25`, `npm >=11 <12`) y actualizar los metadatos raíz de
  `package-lock.json`. No cambiar las versiones de dependencias para ocultar
  el requisito de Node.
- Mantener la prueba basada en `tsc --noCheck` y el runner integrado
  `node --test`, sin añadir un runner TypeScript ni flags experimentales. Poner
  las pruebas transpiladas en el directorio ignorado `test-build/`, fuera de
  `node_modules`: el runner de Node excluye esa ruta y allí la suite quedaba
  omitida o reportada como inexistente. Invocar los archivos compilados con
  un glob compatible con Node 24 y confirmar que la ejecución informa pruebas
  ejecutadas.
- Al ejecutar por primera vez los 102 archivos, 21 fallan con aserciones que
  comparan literales antiguos, firmas estáticas o conteos anteriores con la
  implementación vigente; las fuentes y fixtures sí cargan. Actualizar esas
  aserciones para verificar las claves/interpolaciones localizadas, los
  handlers y delegaciones actuales, y las acciones vigentes. No debilitar
  pruebas de comportamiento ni cambiar funcionalidades sólo para hacerlas
  pasar.
- `formatElapsedTime` documenta y prueba que una marca futura se presenta como
  “Recién capturado”, pero hoy devuelve el literal equivalente a “Ahora” de
  `Intl`. Mantener el contrato documentado usando una clave `time.just_captured`
  en los cinco catálogos; conservar `Intl.RelativeTimeFormat` para los demás
  intervalos y ajustar las pruebas de formato a esos resultados localizados.
- Actualizar `docs/development.md` y el requisito
  `repository-reproducibility` para describir Node 24 LTS / npm 11. El runtime
  Node sólo es una herramienta de desarrollo/build; no se incluye en los
  instaladores de ClipVault.
- Antes de etiquetar, repetir `npm audit` con el lockfile final. La revisión
  actual reporta `devalue@5.9.2` como alta en el árbol que npm clasifica como
  producción y ofrece una corrección compatible. Actualizar sólo esa
  dependencia dentro del rango ya declarado por Svelte, sin `--force`, y
  confirmar que la vulnerabilidad desaparece del audit de producción. El
  código de `devalue` que Svelte importa está en sus módulos `internal/server`;
  confirmar además que el build cliente de ClipVault no lo incorpora. Los
  advisories de Vite y del plugin Svelte se registran por separado como
  dependencias de build/desarrollo; no hacer un upgrade mayor de esas
  herramientas dentro de este cambio.

## Versión de producto

El borrador `v0.0.18` se creó desde el commit `7657f48`, anterior a los cambios
presentes en `main` (`7fa72e8`). La release publicada más reciente es
`v0.0.17`. El siguiente build con los cambios probados usa `0.0.19` en
`Cargo.toml`, `Cargo.lock`, `tauri.conf.json`, `package.json`,
`package-lock.json` y `projects.md`.

No mover ni reemplazar el tag `v0.0.18` ni modificar su borrador. La nueva
etiqueta `v0.0.19` sólo se crea después de integrar el cambio y completar las
validaciones locales. El workflow debe crear un borrador independiente para
`v0.0.19`.

## Verificación local

En ambas máquinas, desde `app/tauri/frontend`:

```sh
node --version
npm --version
npm ci
npm run check
npm run build
npm test
```

Las versiones deben informar Node `v24.x` y npm `11.x`; `npm ci` no debe
mostrar `EBADENGINE`. Luego, desde `app/tauri`:

```sh
cargo tauri build --bundles deb,appimage
```

en Ubuntu, y:

```sh
cargo tauri build --bundles app,dmg
```

en macOS. Probar “Acerca de” en el paquete de producción: la acción de
actualizar debe estar habilitada en producción. El updater no se consulta en
`cargo tauri dev`.

## Borrador GitHub y publicación

Después de integrar los cambios y pasar la prueba manual en ambos sistemas, se
empuja `v0.0.19`. El workflow valida la versión y que la etiqueta pertenezca a
`main`, construye macOS Apple Silicon/Intel y Linux x86_64, firma los artefactos
y crea un borrador. Antes de publicar se revisan el resultado de todos los
jobs, los instaladores y sus `.sig`, y que `latest.json` asocie cada sistema,
arquitectura e instalador con el artefacto correcto. La publicación queda como
un paso explícito separado.
