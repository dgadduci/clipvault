# Propuesta: selector Linux ordenado con iconos de aplicación

## Problema

En la prueba manual de Privacidad sobre Linux, tanto en X11 como en Wayland,
el selector de aplicaciones para agregar una aplicación a la blacklist aparece
en un orden que no corresponde al nombre que ve la persona usuaria. Además,
las filas no muestran el icono de la aplicación y terminan mostrando la inicial
como fallback.

El problema tiene dos causas concretas:

1. `LinuxApplicationCatalog::list()` ordena por `identifier`, aunque la fila
   se presenta con `display_name`.
2. `PrivacyModal.svelte` intenta resolver referencias `application-icons/...`
   mediante `ignoredAppIconCommand`, cuyo contrato valida el namespace
   `ignored-apps/...`; la resolución falla y se muestra la inicial.

## Objetivo

Hacer que el selector visual Linux de Privacidad:

- entregue las candidatas ordenadas alfabéticamente por nombre visible,
  usando el identificador como fallback y desempate determinista;
- muestre el icono local de la aplicación mediante el bridge existente para
  referencias `application-icons/...`;
- conserve un fallback seguro a la inicial cuando el icono no exista o no
  pueda resolverse;
- funcione con candidatas provenientes de X11, XWayland y Wayland.

## Alcance

- Orden canónico en el catálogo Linux y sus pruebas.
- Resolución del icono en el selector de Privacidad y ciclo de vida de sus
  object URLs.
- Cobertura de integración suficiente para confirmar que las referencias de
  candidatas usan el comando de iconos de aplicaciones existente.
- Pruebas manuales en X11 y Wayland.

## Fuera de alcance

- Cambiar la identidad, estrategia de matching o persistencia de la blacklist.
- Cambiar la captura, las cards, Quick Paste, drag and drop o el layout general.
- Añadir dependencias, llamadas de red, APIs nuevas de GNOME Shell o un nuevo
  protocolo IPC.
- Mover, copiar, migrar o limpiar assets existentes.

## Criterios de aceptación

- Una lista con nombres visibles `alpha`, `Beta` y `Zed` aparece en ese orden,
  independientemente del orden de descubrimiento o del identificador.
- Una candidata con un `icon_ref` local válido bajo `application-icons/`
  muestra una imagen PNG en el selector.
- La resolución de esos iconos usa `sourceAppIconCommand` y no el comando
  restringido al namespace `ignored-apps/`.
- Un icono ausente, inválido o que falla conserva la fila utilizable y muestra
  la inicial, sin una imagen rota.
- Las aplicaciones agregadas siguen sin producir capturas cuando están en la
  blacklist, igual que antes.
- El comportamiento queda cubierto para los caminos de descubrimiento de X11
  y Wayland sin introducir lógica específica de plataforma en el core.
