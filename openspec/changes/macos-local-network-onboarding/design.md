# Diseño: onboarding del nombre visible para compartir en red local

## Fuente de estado y condición de acceso

`PeerSharingModal` consultará la configuración local existente antes de
cargar los controles de compartir. El nombre se considera configurado cuando
`Settings.local_peer_display_name`, después de quitar espacios al principio y
al final, no está vacío. No se agrega una clave, migración, comando Tauri ni
dependencia: el valor ya se edita y valida en **Configuración → Privacidad →
Nombre visible**.

La vista no debe solicitar el estado de sharing ni el snapshot de equipos
hasta confirmar que el nombre está configurado. Así, el modal de onboarding no
expone ni carga la vista que debe permanecer oculta.

## Estados del modal

| Estado de configuración | Contenido visible | Acciones de red |
| --- | --- | --- |
| Cargando configuración | Estado de carga accesible | Ninguna |
| Nombre ausente, vacío o sólo espacios | Únicamente explicación y pasos para configurar y guardar el nombre en Configuración → Privacidad; luego volver a Compartir en red local | No consulta toggle/snapshot ni inicia discovery |
| Nombre configurado | Formulario actual completo: toggle, estado, reintento de identidad y lista Equipos | Sólo las acciones actuales del formulario; abrir el modal no activa sharing |
| No se pudo leer configuración | Error seguro y opción de reintentar la lectura | Ninguna |

El mensaje previo a configurar el nombre también explicará el siguiente paso:
volver al modal, activar **Compartir en red local** y aceptar el aviso de
**Red local** de macOS cuando aparezca. Después de ese primer acceso, ClipVault
puede habilitarse en **Configuración del Sistema → Privacidad y seguridad →
Red local**. La instalación por sí sola no debe presentarse como una solicitud
de permiso.

## Integración con las pantallas existentes

- `PrivacyModal` conserva el único formulario de edición del nombre visible.
- `PeerSharingModal` carga primero la configuración; con nombre ausente
  renderiza sólo la guía y omite `peerSharingToggleGetCommand` y
  `peerSnapshotCommand`.
- Con nombre presente se conserva el formulario actual y sus estados typed de
  identidad segura y runtime. Tener nombre no equivale a que la identidad
  segura esté disponible ni a que macOS haya concedido el permiso.
- Al volver a abrir el modal después de guardar el nombre, la vista vuelve a
  consultar Settings y presenta el formulario completo. No se añade un estado
  de onboarding persistido ni se cambia el consentimiento de sharing.

## Documentación pública

Agregar en `docs/install/en.md` y `docs/install/es.md`, dentro de la sección de
primer inicio/configuración de macOS, un bloque de **Compartir en red local**.
Debe indicar la ruta para guardar el nombre visible, el orden de los pasos, la
solicitud del permiso del sistema al activar el sharing por primera vez y la
ubicación de **Red local** en Ajustes del Sistema. Aclarará que el usuario no
debe buscar ClipVault en esa lista inmediatamente después de instalarlo.

## Privacidad, idiomas y verificación

Todo texto nuevo de interfaz se agrega a los cinco catálogos (`en`, `es`,
`pt`, `de`, `fr`) y usa claves de traducción. Los pasos públicos de instalación
se mantienen en inglés y español, los idiomas publicados en `docs/install`.

Los tests frontend comprobarán la separación de estados, la ausencia de
controles y llamadas de sharing mientras falte el nombre, y la preservación
del formulario completo cuando exista. Una prueba manual en macOS con
configuración local limpia confirmará el recorrido hasta el diálogo y la lista
de permisos del sistema; los tests automatizados no intentarán conceder
permisos del sistema.
