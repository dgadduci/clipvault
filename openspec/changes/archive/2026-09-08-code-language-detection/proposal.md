# Proposal: code-language-detection

## Contexto

ClipVault ya clasifica algunos contenidos estructurados (`json`, `html`,
`sql`, `shell_command`) y tiene `ContentType::Code`, pero este último sólo se
activa actualmente con bloques cercados o shebangs reconocibles. Un fragmento
plano como `const value = 1`, una función Python o una estructura Rust se
guarda como `text`, aunque sea código fuente.

La detección y el resaltado son problemas relacionados pero distintos:

- la detección decide qué representa la captura;
- el resaltado sólo modifica su presentación visual.

## Objetivo

- Reconocer de forma conservadora lenguajes de programación frecuentes.
- Persistir el lenguaje como metadata opcional y estable.
- Mantener `ContentType::Code` como categoría común.
- Resaltar código localmente en frontend usando `highlight.js`.
- Reutilizar la misma detección, metadata y presentación en Desktop, cards,
  ClipboardPreview y Quick Paste.
- Mantener el texto original intacto y preservar los tipos existentes.

## Lenguajes iniciales

La primera versión debe cubrir estos identificadores canónicos:

`javascript`, `typescript`, `java`, `c`, `cpp`, `csharp`, `python`, `rust`,
`go`, `kotlin`, `swift`, `php`, `ruby`, `bash` y `shell`.

Los alias de la biblioteca (`js`, `ts`, `py`, `rs`, `c++`, etc.) se normalizan
a estos valores antes de persistirlos. No se deben guardar nombres arbitrarios
devueltos por una biblioteca.

## Alcance

Incluye:

- metadata nullable `code_language`;
- migración SQLite aditiva y reversible;
- detector local conservador;
- comando local metadata-only para persistir una clasificación aceptada;
- etiquetas de lenguaje en las superficies existentes;
- resaltado seguro en la preview compartida;
- tests de precedencia, ambigüedad, persistencia, UI y privacidad.

No incluye:

- ejecución, validación o compilación del código;
- análisis semántico, AST, autocompletado o linting;
- embeddings, LLM, red o telemetría;
- cambio de `content` o conversión a HTML persistido;
- un `ContentType` distinto para cada lenguaje;
- migración de imágenes, rich text, tags, colecciones o favoritos.

## Resultado esperado

Una captura con evidencia suficiente se representa como:

```text
content_type = "code"
code_language = "python"
```

Una captura ambigua conserva:

```text
content_type = "text"
code_language = null
```

La captura original permanece sin modificaciones.
