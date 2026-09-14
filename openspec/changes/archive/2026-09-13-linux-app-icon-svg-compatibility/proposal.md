# Propuesta: compatibilidad SVG para iconos del catálogo Linux

## Problema

Al resolver iconos SVG instalados por el tema del sistema, `usvg 0.45`
registra avisos para `marker-start`, `marker-mid` y `marker-end` cuando el
SVG usa el valor estándar `none`. El parser trata esas propiedades como
referencias a nodos y no acepta el keyword, aunque el icono sea válido. Los
avisos ensucian la consola y pueden hacer parecer que falló el selector.

El escaneo de archivos `.desktop` ya existe en
`LinuxApplicationMetadataProvider` y cubre las raíces XDG, nombres
localizados, precedencia y resolución segura de iconos. Sustituirlo por un
recorrido exclusivo de `/usr/share/applications` perdería aplicaciones del
usuario y la asociación de identificadores que necesitan X11 y Wayland.

## Objetivo

Eliminar los avisos producidos por los valores SVG/CSS válidos `none` sin
alterar el dibujo, el catálogo, la identidad de la aplicación ni la política
que bloquea recursos externos.

## Alcance

- Normalizar, sólo durante el rasterizado, las declaraciones
  `marker-start`, `marker-mid`, `marker-end` y `marker` cuyo valor sea `none`.
- Cubrir atributos XML, atributos `style` y bloques `<style>` sin modificar
  otras propiedades ni texto SVG.
- Agregar regresiones unitarias para la normalización y el rasterizado de un
  SVG con las tres propiedades.
- Mantener el proveedor existente como única fuente del catálogo `.desktop`.

## Fuera de alcance

- No reemplazar el proveedor XDG por un escaneo de `/usr/share/applications`.
- No ejecutar archivos `.desktop`, leer procesos, usar red o ampliar la
  allowlist de iconos.
- No cambiar X11, Wayland, GNOME, PrivacyGate ni el contrato del picker.
- No persistir el SVG normalizado; sólo se guarda el PNG resultante.

## Criterio de éxito

Un icono SVG válido con `marker-start/mid/end="none"`, `style="marker:
none"` o un bloque CSS equivalente se rasteriza sin los avisos de `usvg`,
continúa generando un PNG y conserva el comportamiento de bloqueo de
referencias externas.
