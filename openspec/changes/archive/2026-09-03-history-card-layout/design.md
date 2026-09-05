## Contexto

La ventana principal actualmente renderiza recent-entries como una lista vertical
en App.svelte. EntryRecord contiene el texto, content_type, source_app, tamaño,
hash, timestamps y estado pinned, pero no contiene título ni metadata visual
persistida de la aplicación fuente.

La solución mantiene la separación existente:

    platform adapter
        -> application metadata and controlled icon asset
    clipvault-core
        -> capture enrichment, title rules and domain decisions
    clipvault-db
        -> nullable title and source-app presentation metadata
    Tauri
        -> thin commands and safe icon bytes bridge
    Svelte
        -> horizontal rail, cards and interaction state

## Decisiones de arquitectura

### 1. Componente visual separado

Extraer el listado reciente a un componente reutilizable, por ejemplo
HistoryCardRail y ClipboardHistoryCard. App.svelte conservará la carga de
entries y las operaciones existentes, pero no debe contener todo el layout de
la card.

La rail usa overflow-x auto, cards con flex-basis fijo y scroll-snap opcional.
Cada card mide 240 por 240 CSS pixels, con flex-shrink 0, y conserva su forma
cuadrada incluso cuando el viewport sea más angosto. El tamaño debe quedar como
un token CSS documentado para poder ajustarlo sin romper el contrato.

El preview usa una tipografía significativamente menor, line-clamp o clipping
determinista y no permite que una captura larga cambie la altura de la card.
El contenido completo sigue siendo el mismo dato de la entrada; esta vista sólo
lo resume.

### 2. Jerarquía de la card

La zona superior usa tres áreas:

    [type icon + label] [title centered] [source app icon + name]

El título visible es el título personalizado cuando existe y, si no, el label
localizado de content_type. El nombre de la aplicación usa source_app_name
cuando existe y tiene fallback a source_app o a una etiqueta genérica.

El tipo se representa con SVG local, determinista y accesible. Deben existir
iconos para los tipos text, url, email, json, jwt, uuid, ipv4, ipv6, hex_color,
html, file_path, shell_command, sql y code, más un fallback de texto. No se
debe añadir una dependencia de iconos sólo para este cambio.

La aplicación fuente usa un PNG local controlado por ClipVault. Si no existe,
falla su carga o la plataforma no ofrece metadata, la card muestra un icono
genérico de aplicación sin romper el historial.

### 3. Título persistente

Agregar un campo nullable title al registro de entrada. Null o cadena vacía
significa que la UI utiliza el label del tipo como título por defecto; no se
debe persistir una copia redundante del label.

El menú de puntos suspensivos ofrece Editar título. La edición debe validar
trim, longitud máxima documentada y permitir restaurar el valor por defecto
dejando title en null. El comando de actualización es específico, idempotente
y no necesita confirmación destructiva.

Los errores de título pueden mostrar códigos o mensajes seguros, pero nunca
deben incluir el contenido completo de la captura ni escribirlo en logs.

### 4. Metadata de la aplicación fuente

Extender EntryRecord con source_app_name y source_app_icon_ref opcionales, o
un DTO equivalente que preserve exactamente la misma semántica. source_app
continúa siendo el identificador usado por privacidad y compatibilidad.

La captura permitida consulta un ApplicationMetadataProvider de plataforma.
El provider devuelve nombre e icon_ref y utiliza un cache o asset store
compartido por identificador normalizado para no renderizar el mismo icono en
cada captura. El asset debe vivir bajo un namespace controlado, por ejemplo
assets/application-icons, separado de assets de la blacklist aunque pueda
reutilizar la misma implementación de validación.

El enriquecimiento es best-effort: si falla la resolución o escritura del
icono, la captura de texto se persiste igualmente con fallback visual. La
decisión del PrivacyGate debe ocurrir antes de persistir contenido o metadata
de una captura ignorada; una captura blacklistada no debe crear un asset nuevo.

macOS puede resolver nombre e icono mediante NSWorkspace o NSBundle respetando
main-thread constraints. La coordinación con la shell debe reutilizar el patrón
main-thread existente. Linux debe devolver metadata cuando exista una
correspondencia segura; en X11 o Wayland sin correspondencia se usa fallback y
no se inventa un app id.

### 5. Persistencia y compatibilidad

Preferir una migración SQLite aditiva y reversible que agregue title,
source_app_name y source_app_icon_ref como columnas nullable a entries. Las
filas anteriores permanecen válidas y se muestran con título derivado del tipo,
nombre de app derivado de source_app y un icono genérico.

La referencia del icono es opaca y relativa. El frontend no recibe ni persiste
paths absolutos. El bridge de Tauri debe validar scope, traversal, symlinks,
formato PNG y tamaño antes de entregar bytes para un Blob URL.

### 6. Menú y acciones

El botón de puntos suspensivos se ubica en la parte inferior izquierda de la
card y tiene aria-haspopup, aria-expanded y un label accesible. El botón
pin/unpin permanece dentro de la card, inmediatamente a la izquierda del botón
de puntos suspensivos. El menú puede abrirse hacia arriba para no ser recortado
por la rail.

El menú muestra como mínimo Editar título y la acción existente Eliminar,
manteniendo la confirmación requerida por clipboard-management. Sólo un menú
puede estar abierto a la vez; Escape, click fuera y cambio de foco lo cierran.
No se debe duplicar ningún handler por remount o hot reload.

### 7. Límites de alcance

La rail se aplica al historial reciente de la ventana principal. El listado de
resultados de búsqueda y la ventana quick-paste mantienen su comportamiento
actual; pueden reutilizar un componente sólo si no cambia su contrato ni su
flujo de teclado. La captura sigue siendo exclusivamente textual hasta
clipboard-rich-content.

## Flujo de captura enriquecida

    clipboard text change
        -> source identifier snapshot
        -> PrivacyGate
        -> best-effort app metadata lookup/cache
        -> SQLite entry + metadata
        -> history-updated
        -> card rail refresh

Un fallo de metadata no debe convertir una captura válida en error. La metadata
de aplicación no debe retrasar indefinidamente el loop ni abrir una ventana.

## Privacidad

- Nunca registrar texto, hash, snippet ni payload de clipboard.
- No extraer ni guardar iconos para capturas descartadas por PrivacyGate.
- No cargar iconos desde URLs remotas o paths arbitrarios.
- No ejecutar la aplicación fuente.
- Mantener el icono y nombre como metadata local, sin telemetría.
- No exponer el identificador de la app como contenido principal de la card.

## Verificación manual

En macOS:

1. Lanzar ClipVault y confirmar que el historial reciente es una rail horizontal.
2. Confirmar que las cards son cuadradas, de tamaño uniforme y con bordes redondeados.
3. Copiar textos largos y comprobar que el preview no rompe la card.
4. Verificar icono y label de tipo en la parte superior.
5. Copiar desde TextEdit, Terminal, Safari o una aplicación disponible y verificar nombre e icono de origen.
6. Confirmar que una segunda captura de la misma aplicación reutiliza el icono y no crea assets duplicados.
7. Abrir el menú de puntos suspensivos desde la esquina inferior izquierda.
8. Editar el título, reiniciar ClipVault y comprobar que persiste.
9. Restaurar el título por defecto y comprobar que vuelve al label del tipo.
10. Verificar pin/unpin junto al menú y eliminación con confirmación.
11. Probar historial vacío, teclado, Escape, click fuera y scroll horizontal.
12. Confirmar que búsqueda y quick-paste siguen funcionando.
13. Confirmar que una captura blacklistada no crea un icono ni una nueva card.
