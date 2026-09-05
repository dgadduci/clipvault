# Proposal: desktop-shell-layout

## Problema

El desktop principal expone demasiadas secciones técnicas y de configuración
al mismo tiempo: diagnóstico de la arquitectura, capacidades, aplicación
activa, quick paste, gestión de historial y privacidad. Esto hace que la
superficie cotidiana de ClipVault sea larga, poco jerárquica y difícil de
entender.

Además, la acción para limpiar elementos no favoritos aparece como un botón de
gestión de historial dentro del contenido principal, aunque es una acción
global que debe estar disponible de forma compacta y reconocible.

## Resultado esperado

El desktop principal mostrará una barra superior compacta con búsqueda,
acciones de configuración y un basurero. Las cards de historial continuarán
siendo el foco visual principal.

Las secciones técnicas y de configuración se abrirán en modales independientes:

1. **Development** para diagnóstico y controles de desarrollo.
2. **Privacidad** para blacklist y privacidad.
3. **Retención de historial** para período, preview y aplicación de retención.
4. **Atajo de pegado rápido** para mostrar el atajo y su estado.

La sección `History Management` desaparecerá del desktop. `Clear non-favorite
history` se accederá mediante el icono de basurero en la esquina superior
derecha y conservará la confirmación existente.

## Alcance

- Reorganización visual de `App.svelte` y componentes Svelte relacionados.
- Coordinador de modales con un solo modal abierto por vez.
- Extracción o composición de las secciones existentes dentro de modales.
- Barra superior y acción de limpieza global.
- Sistema visual local de tipografía, tamaños, espaciado, botones y modales.
- Estados de carga, error, vacío y busy existentes conservados.
- Tests frontend para apertura, cierre, foco, routing y no duplicación.

## Fuera de alcance

- No cambiar comandos Tauri, contratos de core, SQLite o migraciones salvo que
  sea imprescindible para mover una vista.
- No cambiar el comportamiento de captura, búsqueda, quick-paste, paste,
  imágenes, rich text, tags, colecciones, blacklist o retención.
- No agregar configuración nueva del atajo: el modal muestra el valor y estado
  que ya expone el backend.
- No agregar librerías de UI, iconos o fuentes externas.
- No agregar red, telemetría, embeddings ni dependencias innecesarias.
- No eliminar el Delete individual de las cards.

## Criterios de aceptación

- El desktop ya no muestra las secciones técnicas/configurables trasladadas.
- Development, Privacidad, Retención y Atajo de pegado rápido se abren desde la
  barra superior.
- `History Management` desaparece del contenido principal.
- El basurero abre la confirmación existente para limpiar sólo elementos no
  favoritos.
- Los favoritos nunca se eliminan por la acción del basurero.
- Los modales son accesibles, cerrables con Escape y no se duplican al montar o
  desmontar la vista.
- El rail de cards, la búsqueda y los flujos existentes siguen funcionando.
- La interfaz mantiene el tema local y mejora jerarquía, tipografía y tamaños.
