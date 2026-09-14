# Delta: Quick Paste

## ADDED Requirements

### Requirement: una activación X11 de fallback abrirá el flujo existente

Cuando el fallback X11 reconozca una binding registrada durante un grab activo
ajeno, SHALL reutilizar exactamente la activación Quick Paste existente. El
backend SHALL publicar solamente la señal de activación; Tauri y el frontend
conservarán la secuencia vigente de captura de target, show, focus y apertura.

#### Scenario: activación por evento raw X11

- **WHEN** el fallback X11 reconoce `Ctrl+Shift+V` durante un grab activo
  ajeno
- **THEN** SHALL invocar el callback registrado una vez
- **AND** Tauri SHALL emitir `clipvault://quick-search` con payload vacío
- **AND** no SHALL transportar contenido de clipboard, hashes, rutas, assets,
  datos de ventana ni títulos

#### Scenario: activación normal X11

- **WHEN** el grab pasivo del manager X11 entrega `Ctrl+Shift+V` sin grab ajeno
- **THEN** SHALL abrir el mismo flujo Quick Paste
- **AND** SHALL no abrir una segunda instancia por el observador raw
