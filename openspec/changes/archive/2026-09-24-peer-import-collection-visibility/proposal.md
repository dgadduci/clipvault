# Propuesta: visibilidad y origen de colecciones importadas desde pares

## Problema

Una importación remota pertenece correctamente al historial local, pero el
usuario puede no distinguirla de una captura propia. El cambio
`peer-text-import` ya persiste un binding por `peer_id` y crea una colección
secundaria; falta garantizar que esa colección aparezca y que su origen remoto
sea reconocible aunque el usuario edite el nombre visible.

## Objetivo

Hacer que cada peer con importaciones tenga una colección local visible en el
sidebar. El nombre de la colección se inicializa con el nombre visible del
equipo y puede editarse como cualquier otra colección, pero el origen remoto
queda vinculado de forma inmutable al `peer_id` y se muestra mediante una marca
o etiqueta derivada del peer.

## Alcance

- Reutilizar `peer_collection_bindings` como fuente de verdad del origen.
- Exponer en la proyección de colecciones si existe un binding remoto y el
  nombre visible actual del peer, sin mostrar el `peer_id` crudo.
- Garantizar que la colección aparece después del primer import y después de
  reiniciar, sin depender de volver a seleccionar `Historial`.
- Mantener el nombre editable, sin cambiar el binding al renombrar.
- Mostrar una marca accesible de origen remoto en el sidebar y en el estado
  de la colección cuando corresponda.
- Mantener el binding independiente por peer, la resolución de colisiones y
  la recreación posterior si el usuario elimina la colección.

## Fuera de alcance

- Cambiar fetch_text, mTLS, deduplicación, provenance o límites de importación.
- Crear una segunda colección por snapshot o hacer sincronización automática.
- Convertir el `peer_id` en nombre visible, secreto o credencial.
- Impedir que el usuario renombre o elimine la colección.
- Copiar el texto, escribir el clipboard, pegar o alterar assets existentes.
- Cambiar el comportamiento de `Historial`: toda entrada seguirá perteneciendo
  a `Historial` además de su colección de origen.

## Criterio de aceptación

Después de importar desde un peer, la entrada permanece en `Historial` y
aparece también en una colección identificada como de origen remoto. El
usuario puede renombrar esa colección sin perder su asociación con el peer;
el origen se sigue mostrando. Varios peers mantienen colecciones distintas,
los reinicios no duplican colecciones y eliminar una colección no elimina las
entradas importadas ni su provenance.
