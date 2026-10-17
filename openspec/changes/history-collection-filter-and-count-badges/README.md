# history-collection-filter-and-count-badges

Cambio propuesto para filtrar las capturas de `Historial` por una colección y
mostrar el total de capturas de cada colección como badge en su fila.

El selector conserva los filtros existentes por aplicación y tag, y se
actualiza ante cambios del ciclo de vida de las colecciones. Los contadores
reflejan las membresías persistidas y se refrescan cuando cambian las capturas
o sus asociaciones.

Estado: implementación terminada. Los checks de compilación y OpenSpec pasan;
las pruebas automatizadas enfocadas quedan pendientes.
