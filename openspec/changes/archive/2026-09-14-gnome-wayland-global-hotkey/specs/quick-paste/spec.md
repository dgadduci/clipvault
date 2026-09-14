# Delta: Quick Paste

## ADDED Requirements

### Requirement: una activación GNOME abrirá el flujo Quick Paste existente

El adaptador Tauri SHALL transformar una solicitud `quick_paste` válida de la
integración GNOME en la emisión de `clipvault://quick-search`. El frontend
SHALL reutilizar el flujo Quick Paste ya existente.

#### Scenario: solicitud local válida

- **WHEN** el listener de GNOME comunica una solicitud de Quick Paste válida
- **THEN** Tauri SHALL emitir `clipvault://quick-search` sin payload de
  contenido
- **AND** la aplicación SHALL usar el mismo modal y flujo de pegado que para
  el atajo global existente
