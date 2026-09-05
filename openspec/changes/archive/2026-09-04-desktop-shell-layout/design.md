# Design: desktop-shell-layout

## 1. Límites de arquitectura

```text
Main desktop
    ├── top toolbar: search + modal triggers + clear-history icon
    └── recent cards rail
            ↓ existing callbacks/commands
       existing core and Tauri contracts

Modal coordinator
    ├── Development
    ├── Privacy
    ├── Retention
    └── Quick-paste shortcut
```

El cambio es principalmente de presentación. La lógica de negocio permanece en
los servicios existentes y los comandos Tauri no deben duplicarse por mover una
sección de lugar.

## 2. Estado de modales

`App.svelte` debe tener un único estado discriminado, por ejemplo:

```text
null | development | privacy | retention | quick_paste_shortcut
```

El coordinador debe garantizar:

- como máximo un modal abierto;
- no montar dos copias del mismo contenido;
- cerrar el modal activo antes de abrir otro;
- conservar el estado de cada sección durante una operación en curso;
- no volver a registrar listeners por cada apertura;
- limpiar listeners al destruir la ventana.

Se puede crear un componente modal compartido si reduce duplicación. No debe
introducir una dependencia externa.

## 3. Barra superior

La barra superior debe contener, en un orden legible y responsive:

- búsqueda existente;
- botón `Development`;
- botón `Privacidad`;
- botón `Retención`;
- botón `Atajo de pegado rápido`;
- icono de basurero alineado a la derecha.

Los botones deben tener texto visible o tooltip más nombre accesible. El
basurero no debe depender de un texto largo para comunicar su función: debe
tener `aria-label` y `title` equivalentes a `Limpiar historial no favorito`.

La barra no debe desplazar ni deformar el rail horizontal de cards. En anchos
pequeños debe poder envolver controles o usar un layout responsive sin cortar
el basurero.

## 4. Modal Development

Debe contener las vistas existentes de diagnóstico:

- `Frontend ↔ Tauri ↔ Rust ↔ SQLite`;
- `Capabilities`;
- `Active application`;
- estado/diagnóstico de quick paste y sus errores.

Los controles `Tick capture` y `Paste latest` se deben auditar durante la
implementación. Si continúan ejecutando comandos reales y son útiles para
diagnóstico/manual testing, deben vivir dentro de Development. Si alguno es un
stub muerto o no cumple ninguna función, debe eliminarse en vez de trasladarse.

El modal no debe convertirse en una nueva ventana Tauri ni alterar la ventana
transient de quick-paste.

## 5. Modal Privacidad

Debe contener el contenido actual de privacidad:

- blacklist de aplicaciones;
- selector nativo y fallback manual existentes;
- mensajes de error y capacidades relacionadas;
- cualquier diagnóstico de privacidad que ya pertenezca al panel actual.

La configuración de retención no debe duplicarse aquí. El valor de retención
debe vivir en el modal Retención.

La guía de permisos existente debe continuar usando sus comandos, estados y
mensajes actuales. Mover el panel no debe marcar ni alterar la verificación
manual de `platform-permission-guidance`.

## 6. Modal Retención

Debe contener:

- selector de 7 días, 30 días, 90 días o forever;
- `Preview retention`;
- `Apply retention now`;
- mensajes de éxito, error, carga y confirmación que ya existan.

La acción destructiva `Clear non-favorite history` no se duplica aquí: se
ejecuta desde el basurero superior usando el flujo de confirmación existente.

El modal no debe aplicar una retención al abrirse. Sólo debe mutar cuando el
usuario confirma o activa la acción correspondiente.

## 7. Modal Atajo de pegado rápido

Debe mostrar el atajo efectivo según plataforma:

- macOS: `Cmd + Shift + V`;
- Linux: `Ctrl + Shift + V`.

También debe mostrar el estado disponible del listener/capacidad usando el
contrato actual. En esta primera versión es informativo/read-only: no se
agrega edición del atajo ni otro registro de hotkey.

La ventana transient de quick-paste continúa siendo la superficie de uso del
atajo. Este modal no debe abrirla, reemplazarla ni registrar un segundo
listener.

## 8. Basurero y limpieza

El icono superior debe reutilizar `clearHistoryCommand` y la confirmación
existente:

1. activar el icono abre la confirmación;
2. cancelar no muta nada;
3. confirmar envía `confirm: true` por el comando existente;
4. sólo se eliminan elementos no favoritos;
5. los favoritos y sus assets permanecen;
6. el rail se refresca después del éxito;
7. errores y estados busy se muestran sin romper el desktop.

No reemplazar la acción `Delete` individual del menú de cada card.

## 9. Tipografía y sistema visual

Mantener el tema oscuro existente y definir tokens CSS locales para evitar
valores inconsistentes:

- texto general: 14–15 px;
- texto secundario: 12–13 px;
- título principal: 24–28 px;
- títulos de sección/modal: 16–18 px;
- botones compactos: 12–14 px;
- espaciado y padding moderados;
- bordes, radios y colores coherentes con las cards actuales.

La rail conserva sus cards cuadradas y su scroll horizontal. El objetivo es
compactar la shell, no volver a cambiar el layout de las cards.

No cargar fuentes, iconos ni estilos desde Internet. Reutilizar SVG y estilos
locales ya existentes.

## 10. Accesibilidad y foco

Cada modal debe:

- usar `role="dialog"` y `aria-modal="true"`;
- tener un título asociado mediante `aria-labelledby`;
- enfocar el primer control útil al abrirse;
- devolver el foco al botón que lo abrió al cerrarse;
- cerrarse con Escape;
- cerrarse al hacer clic fuera cuando no haya una operación que lo impida;
- mantener navegación por Tab dentro del modal mientras está abierto;
- exponer nombres accesibles para iconos y botones;
- conservar visible el foco de teclado.

El fondo no debe quedar interactuable mientras un modal está abierto. La
interacción no debe depender exclusivamente del color o del icono.

## 11. Privacidad y contratos existentes

Este cambio no debe añadir payloads a eventos existentes ni transportar
contenido del clipboard. Los comandos y eventos deben conservar sus contratos
metadata-only. No se deben registrar contenidos, snippets, hashes, paths,
HTML, RTF ni bytes de imágenes.

Los errores se muestran mediante los mensajes y tipos ya existentes. Si se
requiere un nuevo estado de UI, debe ser local al frontend y no crear una nueva
capa de negocio.

## 12. Compatibilidad

Debe conservarse el comportamiento de:

- captura y actualización del historial;
- búsqueda local;
- cards de texto, rich text e imagen;
- paste plano, rich y de imagen;
- tags y colecciones;
- blacklist y platform-permission-guidance;
- retención y favoritos;
- ventana transient de quick-paste;
- listeners idempotentes.
