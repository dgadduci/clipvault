## Contexto

privacy-settings ya persiste identificadores ignorados y PrivacyGate los
compara antes de persistir un payload. La UI actual conoce sólo strings y los
muestra como texto. Este cambio añade metadata de presentación sin cambiar el
identificador que usa la regla de privacidad.

    Native application picker
             |
    Platform adapter -> SelectedApplication metadata
             |
    Core settings service -> normalized ignored-app record
             |
    SQLite + PrivacyGate
             |
    Tauri DTO -> Svelte icon/name row

## Decisiones

### Identificador canónico

La selección genera un SelectedApplication con identifier, display_name, icono
o referencia local segura y, sólo durante la operación, el path que el adapter
necesite. El identificador normalizado es el único dato que participa en el
matching. La UI no debe derivarlo ni volver a escribirlo.

No persistir el path absoluto del bundle salvo justificación explícita.
Preferir una referencia de asset local estable para el icono.

### Selector macOS

Usar un selector nativo inicializado en /Applications (se puede aceptar
~/Applications si queda documentado). Sólo aceptar directorios .app cuya
metadata permita obtener un identificador no vacío. No usar open, osascript,
shell ni URLs para resolver la aplicación. Respetar las restricciones de AppKit
y evitar deadlocks con el event loop de Tauri. Cancelar no es un error y no
modifica la blacklist.

### Icono

El icono debe poder renderizarse después de reiniciar sin leer una ruta
arbitraria. La implementación puede guardar un PNG pequeño en un asset local
controlado por ClipVault y persistir una referencia relativa, o usar otra
representación local equivalente validada por el backend. Debe limitar tamaño y
formato. Si falla la extracción, la aplicación se agrega con icono genérico.
El nombre debe tener fallback al nombre del bundle y, sólo para accesibilidad o
diagnóstico, al identificador.

### Persistencia compatible

Evolucionar ignored_apps con una migración aditiva y reversible que agregue
metadata opcional, por ejemplo display_name e icon_ref. Los registros antiguos
sólo con id siguen siendo válidos y reciben fallback visual.

Seleccionar dos veces la misma aplicación es idempotente: no crea otra fila y
puede completar metadata faltante. El servicio de matching continúa consumiendo
únicamente ids normalizados.

### Linux

La primera implementación funcional es macOS. En Linux no inventar un app id si
no existe una correspondencia segura entre el elemento .desktop seleccionado y
el identificador que usa X11/Wayland. Devolver unsupported_session o
backend_unavailable y mantener usable la UI. Wayland no debe recibir un id
fabricado.

### API y UX

El shell expone un comando delgado, por ejemplo
clipvault_ignored_app_pick_and_add, que delega en el adapter y el servicio de
settings. Tauri no contiene parsing de Info.plist, extracción de iconos ni
lógica de matching. La UI muestra botón, carga, cancelación silenciosa, error,
éxito y filas con icono, nombre y Eliminar; el id no es la presentación
principal.

## Flujo de datos

    click Seleccionar aplicación
      -> picker (/Applications)
      -> validar .app
      -> leer id/nombre/icono
      -> core add/update ignored app
      -> SQLite commit
      -> DTO de lista
      -> render icon + name

Un fallo anterior al commit no cambia la lista. Un fallo de icono no impide
guardar una aplicación válida.

## Seguridad y privacidad

- No ejecutar la aplicación seleccionada.
- No aceptar rutas fuera de ubicaciones permitidas sin una decisión explícita.
- No seguir symlinks fuera de una raíz permitida sin validación segura.
- No invocar shell ni URLs construidas desde la selección.
- No escribir contenido, hash o snippet del clipboard en logs.
- No mezclar metadata de aplicación con eventos de captura.

## Verificación manual

En macOS:

1. Abrir Privacidad y pulsar Seleccionar aplicación.
2. Confirmar que el selector inicia en /Applications.
3. Seleccionar Terminal.app y comprobar que la lista muestra su icono y Terminal.
4. Reiniciar ClipVault y confirmar que icono y nombre se conservan.
5. Seleccionar Terminal otra vez y confirmar que no hay duplicado.
6. Copiar un marcador desde Terminal y confirmar que la blacklist lo descarta.
7. Cancelar el selector y confirmar que la lista no cambia.
8. Intentar seleccionar un archivo no-app y confirmar un error seguro sin mutación.

Linux debe mostrar la capacidad no disponible de forma específica hasta que
exista un mapeo seguro.
