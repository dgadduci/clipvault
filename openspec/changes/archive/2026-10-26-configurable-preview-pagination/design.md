# Design: configurable preview pagination

## Configuración compartida

Agregar `preview_page_size` al agregado tipado de Settings, con default 20 y
validación de 1 a 50. `SettingsService` lo lee y persiste como una fila más de
`app_settings`; la tabla ya es key/value y no necesita migración. La
Configuración general expone un control numérico accesible y guarda el valor
mediante el comando de Settings existente. Al guardarlo, el desktop actualiza
su tamaño de página y reinicia la vista activa a la primera página.

El límite superior de 50 evita que el ajuste local solicite más filas de las
que aceptan las páginas actuales de texto e imagen del protocolo entre
equipos. El frontend y el core validan el rango; los valores persistidos
faltantes o inválidos resuelven al default.

## Historial y colecciones locales

Agregar `offset` a la consulta Tauri de capturas filtradas. El repositorio
aplica `LIMIT` y `OFFSET` al query ordenado por `created_at DESC, id DESC`,
después de los filtros de colección, tags, aplicación, tipo e historial sin
asignar. El frontend solicita `tamaño + 1` filas; la fila adicional indica si
existe una página siguiente y no se muestra. Esto evita cargar todas las
capturas en el webview.

La búsqueda mantiene su ranking actual y pagina los resultados puntuados con
el mismo offset después de ordenar. El historial y todas las colecciones
comparten controles Anterior/Siguiente y un indicador de página. Cambiar de
colección, filtro, búsqueda o tamaño de página vuelve a la primera página;
una respuesta antigua no puede reemplazar la vista de otra página o scope.
La paginación usa las consultas existentes y no altera las reglas de
visibilidad de subcolecciones. El rail de tarjetas usa una sola fila horizontal
con columnas de tamaño fijo y scroll horizontal; la paginación queda fuera del
rail y no cambia su orientación.

## Previews de importación entre equipos

Pasar el tamaño configurado como `limit` a los endpoints paginados existentes
de texto e imagen. La combinación de streams conserva el máximo combinado de
la página configurada y guarda el excedente en los buffers existentes para
que `Siguiente` no omita capturas. La solicitud y el merge conservan el tope
protocolario de 50. Al cambiar de tamaño se invalida la página/cache activa y
se vuelve a solicitar la primera página.

## Compatibilidad y seguridad

- No se agregan dependencias ni se cambia el almacenamiento de capturas.
- La respuesta Tauri sigue limitada a los registros de la página solicitada.
- No se agregan textos de producto fuera de los cinco catálogos de idioma.
- Los controles de paginación quedan fuera de las cards y no modifican el
  controlador de drag and drop ni su payload de id opaco.

## Verificación

- Probar queries por offset con filtros y búsqueda, incluyendo el indicador
  de página siguiente.
- Probar default, validación y persistencia del tamaño de página.
- Probar el merge remoto con tamaños menores al máximo y con excedentes en
  buffers.
- Ejecutar frontend checks/build, regresiones protegidas de drag and drop,
  tests backend afectados, validación estricta de OpenSpec y `git diff
  --check`.
