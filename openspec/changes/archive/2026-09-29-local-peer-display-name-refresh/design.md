# Diseño: propagar nombres sin tratarlos como identidad

## Decisión

El campo `display_name` seguirá siendo metadata pública y editable. La
identidad del peer continuará anclada en `peer_id` y en la huella de su clave
pública. Un nombre válido distinto no será motivo de conflicto si la identidad
y el protocolo siguen siendo compatibles.

## Publicación del nombre local

Al guardar un nombre válido mientras el uso compartido está activo, el
runtime actualizará el anuncio DNS-SD/mDNS existente con el mismo peer_id,
huella y capacidades, reemplazando solo el nombre. El browse loop y el
listener seguirán activos. El mismo valor actualizado deberá llegar a los
nuevos mensajes de pairing que incluyan el nombre local. Si el uso compartido
está desactivado, el valor persistido se publicará al iniciar la siguiente
sesión de uso compartido.

## Observaciones recibidas

Para un peer_id ya conocido, una observación podrá actualizar `display_name`
cuando incluya un nombre válido, coincidan la huella corta y la huella
completa conocida cuando esté disponible, coincida la versión mayor y se
permita la transición de capacidad existente. La transacción conservará el
trust state, certificado TLS fijado, paired_at, protocolo emparejado,
cursor_secret, first_seen_at e imports/colecciones asociados; actualizará
únicamente la metadata dinámica permitida, incluyendo nombre, capacidad,
capabilities y timestamps de observación.

Una huella distinta, una versión incompatible o una transición de capacidad
no permitida seguirá produciendo conflicto sin sobrescribir el registro
conocido. Los nombres inválidos no actualizarán el nombre persistido.

La misma regla aplica a peers confiables y no confiables. Un peer que estuvo
ausente adopta el nombre vigente cuando DNS-SD vuelva a resolver su anuncio.
Las tarjetas o colecciones que proyectan el nombre desde `known_peers`
mostrarán el último valor persistido sin reemparejar ni alterar su contenido.

## Verificación

- Pruebas de repositorio para actualización de nombre conservando trust,
  fingerprints, certificado fijado, fecha de primer avistamiento y metadata de
  pairing.
- Pruebas de conflicto para huellas distintas, versión incompatible, nombre
  inválido y transición de capacidad no permitida.
- Pruebas del runtime/adaptador para reanunciar el nombre en ejecución sin
  detener browse ni listener, y para persistir una observación renombrada.
- Prueba manual entre equipos ya emparejados y descubiertos, incluyendo un
  equipo que se reconecta después del cambio de nombre.
