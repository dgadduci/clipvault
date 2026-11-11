# Propuesta: completar un release desktop publicado con Linux

## Problema

El release `v0.0.22` ya está publicado con los instaladores de macOS, mientras
que el job Linux del workflow original quedó cancelado. El workflow solo
acepta tags y siempre intenta crear un borrador, por lo que no permite añadir
los paquetes Linux a ese release existente.

## Qué cambia

- Añadir una ejecución manual del workflow que recibe un tag estable ya
  publicado y construye únicamente Linux x86_64.
- Validar que el tag coincide con todas las versiones de la aplicación, apunta
  a un commit alcanzable desde `main` y corresponde a un release publicado no
  preliminar.
- Adjuntar AppImage, DEB y RPM al release existente; actualizar sus notas para
  reflejar las plataformas disponibles.
- Conservar las entradas macOS existentes en `latest.json` al añadir la
  plataforma Linux.
- Mantener el flujo por tag para releases nuevos y su revisión como borrador.
- Extraer las validaciones del workflow a scripts Python locales que usan la
  biblioteca estándar.
- Mantener los scripts del workflow disponibles desde un checkout de su commit
  mientras la compilación usa el código del tag publicado.

## Fuera de alcance

- Recompilar o reemplazar los instaladores macOS de `v0.0.22`.
- Cambiar la versión, el tag, la firma del updater o los canales de
  distribución.
- Publicar versiones futuras automáticamente.

## Impacto

Se modifica el workflow de releases y los requisitos de distribución desktop
y paquetes RPM. No hay cambios en la aplicación.
