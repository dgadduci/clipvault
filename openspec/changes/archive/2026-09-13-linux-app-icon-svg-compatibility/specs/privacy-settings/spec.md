# Compatibilidad de iconos SVG del selector Linux

## ADDED Requirements

### Requirement: Los iconos SVG válidos no generan warnings por marker none

El rasterizador Linux SHALL aceptar SVG que expresen el valor estándar
`none` en `marker`, `marker-start`, `marker-mid` o `marker-end` sin emitir los
warnings de compatibilidad de `usvg`. SHALL conservar referencias de marker
`url(#...)`, otras propiedades SVG y la política de no cargar recursos
externos.

#### Scenario: Atributos marker none

- GIVEN un icono SVG con `marker-start="none"`, `marker-mid="none"` y
  `marker-end="none"`
- WHEN el proveedor lo rasteriza
- THEN se genera un PNG válido
- AND no se registra el warning de parseo de esas propiedades

#### Scenario: CSS marker none

- GIVEN un icono SVG con `marker: none` o las propiedades individuales en un
  atributo `style` o en un bloque `<style>`
- WHEN el proveedor lo rasteriza
- THEN se genera un PNG válido sin esos warnings

#### Scenario: Marker referenciado y recursos externos

- GIVEN un SVG con `marker-start="url(#arrow)"` y una referencia externa en
  `<image>`
- WHEN el proveedor lo rasteriza
- THEN conserva el marker local
- AND rechaza la carga externa como antes
