# Proposal: configurable-preview-pagination

## Why

El historial y las colecciones locales muestran una cantidad fija de capturas
sin controles para avanzar por el resto. Los previews de importaciones entre
equipos ya usan páginas, pero su cantidad no se puede configurar junto con la
de las colecciones.

## What Changes

- Paginar todas las colecciones locales y el Historial, también cuando hay
  filtros o una búsqueda activos.
- Agregar a Configuración general un tamaño de página compartido por las
  colecciones locales y los previews de importación entre equipos.
- Usar 20 previews por página por defecto. Permitir valores de 1 a 50 para
  mantener el límite de página que ya soportan las importaciones entre
  equipos.
- Persistir el ajuste en `app_settings`, sin una migración de SQLite.

## Fuera de alcance

- Cambiar el protocolo o el límite máximo del transporte entre equipos.
- Paginar el ordenamiento de archivos del modal de importación local.
- Cambiar el contenido, orden cronológico o semántica de los filtros.
