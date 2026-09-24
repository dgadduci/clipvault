# Diseño: visibilidad y origen de colecciones importadas desde pares

## Fuente de verdad

`peer_collection_bindings(peer_id, collection_id)` es la identidad de la
colección remota. La identidad no depende del nombre de la colección ni del
nombre visible mutable del peer. No se agrega una tabla paralela ni se infiere
el origen por prefijos, colores o coincidencia de nombres.

La consulta de colecciones hará un left join metadata-only con el binding y,
cuando el peer siga conocido, con su nombre visible actual. La proyección
pública incluirá sólo campos de presentación equivalentes a:

```text
is_peer_bound: boolean
peer_display_name: Option<String>
```

El `peer_id`, certificados, fingerprints, endpoints y cualquier secreto no
se serializan al frontend.

## Nombre y marca de origen

Al crear el binding, el nombre inicial se obtiene del nombre visible remoto y
usa la resolución de colisiones existente. Un rename posterior modifica sólo
`collections.name`; nunca modifica `peer_collection_bindings`.

La UI mostrará el nombre elegido por el usuario y una marca accesible de
origen, por ejemplo `Importadas de Equipo X`, construida desde
`peer_display_name`. Si el peer cambia su nombre, la marca puede actualizarse
sin renombrar la colección. Si ya no existe un nombre visible, se mostrará una
etiqueta genérica segura como `Importadas de equipo remoto`.

La marca no debe incluir el `peer_id` crudo ni datos de red. El color y las
acciones normales de la colección se conservan.

## Ciclo de vida

- El primer commit de importación crea la colección, el binding y la
  membership atómicamente, y emite el evento de organización ya existente.
- El shell refresca la proyección de colecciones al recibir ese evento, de modo
  que el sidebar muestra la colección sin una navegación adicional.
- Al reiniciar, la consulta reconstruye la misma proyección desde SQLite y el
  binding persiste.
- Renombrar conserva el binding y el origen visible.
- Eliminar la colección elimina su binding según el contrato existente, pero
  conserva entries, memberships de `Historial` y `remote_imports`. Una futura
  importación crea un binding nuevo y no selecciona una colección sólo por
  nombre.

## Privacidad y límites

La colección no transporta contenido remoto. Los eventos de refresco siguen
siendo metadata-only y no incluyen texto, hashes, assets, certificados,
direcciones ni rutas. La nueva proyección no modifica el payload de drag and
drop ni las acciones de las cards locales.

## Verificación

La lógica de binding y sus invariantes se prueban en core/DB; la proyección y
el refresco se prueban en Tauri/frontend. La matriz manual debe comprobar
Linux X11, Linux Wayland y macOS, además de rename, reinicio, varios peers y
el borrado/recreación de la colección.
