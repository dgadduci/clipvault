# Tareas

## 1. Especificación

- [x] 1.1 Revisar el componente compartido y confirmar que Desktop y Quick
  Paste usan `ClipboardPreview.svelte`.
- [x] 1.2 Documentar la causa de doble escape y el límite de seguridad del
  renderizado como nodo de texto.
- [x] 1.3 Definir el contrato de caracteres visibles originales sin decodificar
  texto literal parecido a entidades HTML.

## 2. Implementación

- [x] 2.1 Renderizar el contenido canónico sin resaltado mediante interpolación
  de texto de Svelte y retirar el pre-escape de esa rama.
- [x] 2.2 Actualizar comentarios y pruebas existentes que exigen pre-escapar el
  texto antes de interpolarlo.
- [x] 2.3 Añadir regresiones para comillas, ampersands, `&quot;` literal,
  markup-hostil y equivalencia entre Desktop y Quick Paste.
- [x] 2.4 Confirmar que el branch de código resaltado sigue mostrando
  caracteres originales y mantiene el renderizado seguro existente.

## 3. Verificación

- [x] 3.1 Ejecutar los tests frontend relevantes, `npm test`, `npm run check` y
  `npm run build`.
- [x] 3.2 Validar el cambio OpenSpec en modo estricto y revisar
  `git diff --check`.
- [x] 3.3 Verificar manualmente una captura con comillas en el preview del
  Desktop y Quick Paste cuando haya una sesión interactiva disponible.

La prueba manual de los previews de Desktop y Quick Paste fue aprobada por el
usuario.
