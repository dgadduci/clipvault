# quick-paste-actions

Segunda etapa de Quick Paste. Parte del layout compacto de
`quick-paste-compact-ui` y agrega pin, favoritos y menú de acciones.

Decisión funcional vigente:

- `Enter` y `Shift+Enter` escriben la representación elegida en el
  portapapeles del sistema y cierran Quick Paste, pero no envían
  `Cmd/Ctrl+V` sintético.
- El usuario puede pegar luego con `Cmd/Ctrl+V`.
- Las acciones explícitas del menú `...` conservan el flujo directo de
  pegado automático existente hasta que se defina lo contrario.

Estado: preparado para implementación; no archivar automáticamente.
