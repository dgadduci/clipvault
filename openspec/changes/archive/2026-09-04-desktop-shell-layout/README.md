# desktop-shell-layout

Este cambio reorganiza la interfaz principal de ClipVault sin alterar la
lógica de captura, historial, búsqueda, tags, colecciones, imágenes, rich text
ni pegado.

La pantalla principal queda enfocada en buscar y revisar las cards. Los
diagnósticos y configuraciones pasan a modales accesibles abiertos desde una
barra superior compacta.

## Estado

- Cambio preparado por Codex como arquitectura OpenSpec.
- MiniMax debe implementarlo con `/opsx-apply desktop-shell-layout`.
- No archivar al finalizar la implementación.
- La verificación manual pendiente de
  `platform-permission-guidance` permanece fuera de este cambio.

## Distribución acordada

- **Development**: diagnósticos Frontend ↔ Tauri ↔ Rust ↔ SQLite,
  Capabilities, Active application y controles de Quick paste que sigan siendo
  funcionales.
- **Privacidad**: blacklist y controles de privacidad existentes.
- **Retención de historial**: selector de período, Preview retention y Apply
  retention now.
- **Atajo de pegado rápido**: visualización del atajo configurado y su estado.
- **Basurero**: Clear non-favorite history, conservando la confirmación y los
  favoritos.
