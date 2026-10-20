# Diseño: README e identidad del proyecto

## README y navegación

El `README.md` de raíz será la página canónica de entrada en inglés y enlazará
al documento completo `README.es.md`. Ambos contendrán el mismo orden y los
mismos enlaces esenciales: propuesta del producto, funciones comprobadas,
captura, plataformas con estado de prueba, instalación, privacidad, soporte y
contribución. Evitar duplicar paso a paso las guías de `docs/install/`.

La sección inicial dirá que ClipVault es un gestor local del historial del
portapapeles para macOS y Linux. Las funciones enumeradas se comprobarán contra
la última versión publicada; no se copiarán listas de CopyQ ni del roadmap de
ClipVault.

El resumen de funciones describirá las capacidades de la release vigente:
captura y edición de texto, organización y notas, búsqueda, Quick Paste y
compartición optativa en red local. La transferencia entre equipos requiere
vincularlos e importar de forma explícita cada elemento elegido; no se
presentará como sincronización automática.

## Capturas

Incluir capturas actuales de la interfaz que el usuario revisó y aprobó
explícitamente para publicación. El README y la imagen social usarán recursos
del proyecto y mantendrán contraste y legibilidad en pantalla pequeña. No
incluir credenciales ni secretos en las imágenes.

## GitHub About y social preview

Proponer una descripción corta, topics y website que reflejen la aplicación y
sus plataformas actuales. Guardar los valores en un archivo documentado de
metadata para revisión. Aplicar cambios puntuales de About y social preview
sólo tras una solicitud explícita del usuario; no automatizar escrituras
futuras a la configuración remota.

Los topics priorizan el producto, sus funciones y plataformas. Mantener
`clipboard-manager`, `clipboard-history`, `local-first`, `macos` y `linux`;
añadir `local-network-sharing` para describir la transferencia optativa entre
equipos. Omitir topics de implementación como `rust`, `tauri` y `svelte`. Los
cambios remotos se realizan sólo tras una solicitud explícita del usuario.

Crear `docs/assets/github-social-preview.png` en 1280×640 px con el logo
vigente, nombre ClipVault y una descripción breve. No debe repetir una lista
larga de funciones ni incluir afirmaciones técnicas absolutas.

## Precisión de privacidad y plataforma

Explicar que el historial se guarda localmente y que ClipVault no requiere una
cuenta o sincronización cloud para su uso normal. Aclarar, enlazando a
`docs/releases.md`, que las comprobaciones de actualizaciones contactan GitHub
y envían solo los datos técnicos allí descritos. Las plataformas se vinculan a
la matriz actual de pruebas; no se deduce soporte universal a partir de “Linux”.

## Mantenimiento

Cada release que cambie capacidades, formatos, links o plataformas debe
disparar una revisión del README y las guías. Los enlaces usarán el destino de
releases actual y no una etiqueta versionada que envejezca. Mantener ambos
idiomas en el mismo cambio y sin traducciones parciales.
