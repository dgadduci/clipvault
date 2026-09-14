# Diseño: normalización acotada de propiedades marker

## Decisión

`linux_svg_raster` hará una pasada léxica pequeña sobre el XML antes de
llamar a `usvg::Tree::from_data`. La pasada no es un parser SVG general:

1. elimina atributos XML `marker-start`, `marker-mid` o `marker-end` cuyo
   valor sea exactamente `none`;
2. elimina declaraciones CSS `marker`, `marker-start`, `marker-mid` y
   `marker-end` cuyo valor sea `none` (con `!important` opcional) en
   `style="..."` y `<style>...</style>`;
3. conserva referencias `url(#...)`, las demás propiedades, comentarios,
   cadenas y el contenido no relacionado.

Omitir una propiedad marker con valor `none` es semánticamente equivalente a
no tenerla, porque `none` es el valor inicial de esas propiedades. Se hace
antes del parser para evitar que la dependencia emita el warning; no se
silencian logs globalmente.

## Seguridad y límites

La normalización se ejecuta después del límite de bytes y antes del mismo
`Options` con `ImageHrefResolver` que rechaza recursos externos. No añade
dependencias, no escribe el SVG intermedio y no cambia los límites de tamaño.
Si los bytes no son UTF-8, se entregan sin transformar y `usvg` conserva su
resultado de error.

## Verificación

- Test unitario de cada forma de declaración `none` y de preservación de
  `url(#arrow)`.
- Test de rasterizado de un SVG con los valores problemáticos que verifica la
  salida PNG.
- `cargo fmt --all -- --check` y tests de `clipvault-platform` con
  `linux-svg-raster`.
- Validación OpenSpec estricta y `git diff --check`.
