# Delta: integración de plataforma desktop

## ADDED Requirements

### Requirement: X11 conservará el hotkey durante un grab activo ajeno

En Linux X11, ClipVault SHALL conservar `Ctrl+Shift+V` cuando otro cliente
mantenga temporalmente un grab activo de teclado. Un único adaptador SHALL
mantener el grab pasivo como ruta primaria y SHALL usar eventos raw XInput2
solo cuando el `KeyPress` pasivo correspondiente no llegue durante un grab
activo del teclado.

#### Scenario: pulsación X11 sin grab ajeno

- **GIVEN** una sesión Linux X11 sin un `XGrabKeyboard` activo de otro cliente
- **WHEN** la persona presiona `Ctrl+Shift+V`
- **THEN** el backend pasivo existente SHALL entregar una activación
- **AND** el observador raw SHALL no entregar una segunda activación

#### Scenario: pulsación X11 con grab activo ajeno

- **GIVEN** una sesión Linux X11 donde otro cliente mantiene un
  `XGrabKeyboard` activo
- **AND** XInput2 está disponible
- **WHEN** la persona presiona `Ctrl+Shift+V`
- **THEN** el fallback raw SHALL entregar una sola activación al callback de
  hotkey registrado
- **AND** la pulsación siguiente, tras su release, SHALL poder activar de
  nuevo

#### Scenario: XInput2 no está disponible

- **GIVEN** una sesión Linux X11 sin XInput2 utilizable
- **WHEN** ClipVault inicia
- **THEN** SHALL conservar el backend de grab pasivo
- **AND** SHALL exponer solo un diagnóstico técnico sanitizado de la
  indisponibilidad
- **AND** SHALL continuar vivo sin declarar que el fallback raw está activo

### Requirement: el fallback X11 preservará las fronteras de plataforma

El fallback SHALL vivir en `clipvault-platform`, compilado solo para Linux
X11. No SHALL depender de Tauri, Svelte, navegador, clipboard ni de datos de
ventana o usuario.

#### Scenario: arranque Wayland

- **WHEN** ClipVault arranca en Linux Wayland
- **THEN** SHALL no inicializar el observador X11/XInput2
- **AND** el mecanismo GNOME consentido seguirá siendo la ruta de hotkey
  Wayland
