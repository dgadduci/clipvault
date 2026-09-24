# Propuesta: transferencia manual de capturas de texto entre pares locales

## Problema

El historial de ClipVault es local a cada equipo. Cuando una persona trabaja
con varios equipos de confianza en la misma red, debe copiar el texto por un
medio externo para reutilizar una captura entre ellos. El roadmap
descarta la sincronización automática entre dispositivos; esa restricción se
mantiene. Lo que se propone es una importación local, explícita y verificable.

## Objetivo

Permitir que N instalaciones de ClipVault se descubran dentro de la misma red
local, se vinculen de a pares una sola vez con aprobación mutua y un código
corto coincidente, y naveguen manualmente las capturas de texto transferibles
de cada par activo. Al importar una captura, ClipVault crea o reutiliza una
entrada local y la agrega a una colección vinculada al par remoto.

## Alcance aprobado

- Cada instalación posee una identidad criptográfica local estable y un nombre
  visible editable. El nombre no es una credencial ni la fuente de confianza.
- `Compartir en red local` empieza desactivado. Al habilitarlo, la aplicación
  pide los permisos de sistema que correspondan, anuncia su servicio y busca
  pares continuamente en futuras ejecuciones hasta que se desactive.
- El descubrimiento usa DNS-SD/mDNS sólo dentro del segmento local. La lista
  conserva pares vistos en sesiones previas y distingue `Activo`, `No
  disponible` y `No verificado`.
- Cada instalación puede vincularse con N pares de forma independiente. Un
  vínculo, su revocación, bloqueo, disponibilidad o colección no altera el
  acceso ni las importaciones de los demás pares.
- El primer vínculo exige una solicitud y aceptación explícita en ambos
  equipos. Ambos muestran el mismo código corto; sólo después de confirmarlo
  se guardan las claves públicas de confianza. El vínculo habilita acceso
  mutuo y los accesos posteriores se autentican silenciosamente.
- Sólo se exponen entradas textuales sin imágenes ni payload enriquecido. El
  historial remoto muestra título cuando existe, tipo, fecha y un preview de
  texto escapado y acotado. La navegación es por páginas recientes; no hay
  búsqueda remota en esta entrega.
- El texto completo viaja sólo tras una acción explícita `Importar`. El
  importador preserva un título remoto válido en entradas nuevas, asigna la
  entrada a una colección asociada internamente al `peer_id` y evita duplicar
  el mismo snapshot importado. Una entrada local idéntica se reutiliza sin
  sobrescribir su título local.
- Desvincular o bloquear corta acceso futuro, pero no borra colecciones,
  capturas ni registros de importación ya locales.

## Fuera de alcance

- Sincronización automática, polling de contenido, push, mirror, conflictos,
  borrado remoto, edición remota, presencia fuera de la LAN, NAT traversal,
  relay, nube o cuentas.
- Imágenes, rich text/HTML/RTF, assets, tags, colecciones, favoritos,
  aplicaciones de origen, hashes de contenido o secretos del equipo remoto.
- Escaneo de subredes/puertos, broadcast propio, entrada manual de IP, QR,
  invitaciones por enlace, emparejamiento automático o aceptación unilateral.
- Exponer el clipboard del sistema, pegar sintéticamente al importar o crear
  una nueva captura desde el monitor de clipboard.
- Windows y soporte de redes que bloquean multicast en esta primera entrega.

## Criterios de aceptación

Con N instalaciones con la compartición habilitada en el mismo segmento LAN,
cada una puede descubrir y vincular de forma independiente los pares elegidos
tras aprobar el código corto correspondiente en ambas pantallas. Todo par
vinculado y activo permite navegar páginas de texto remoto con previews
seguros. Importar una entrada crea o reutiliza una entrada local, conserva un
título remoto válido si corresponde, la asocia a la colección de ese par y
permanece disponible después de reiniciar, desvincular o bloquear. Una red sin
multicast, un par no confiable, un protocolo incompatible o un contenido fuera
de los límites falla de forma tipada, sin filtrar texto en logs, eventos ni
descubrimiento.

## Entrega incremental

Este documento no autoriza una implementación única. Su alcance se descompone
en cinco cambios OpenSpec dependientes: identidad local, descubrimiento,
vínculo mutuo, navegación de historial remoto e importación. Cada cambio debe
validarse, probarse y quedar en un commit funcional antes de iniciar el
siguiente; MiniMax sólo recibirá uno de ellos a la vez.
