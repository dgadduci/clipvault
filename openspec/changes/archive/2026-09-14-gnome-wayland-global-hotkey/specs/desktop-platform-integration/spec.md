# Delta: integración de plataforma desktop

## ADDED Requirements

### Requirement: GNOME Wayland debe reenviar una solicitud de Quick Paste local

Cuando la integración GNOME de ClipVault esté habilitada y conectada, la
extensión SHALL registrar `Ctrl+Shift+V` mediante la API de Mutter y enviar un
mensaje de activación local al socket Unix de ClipVault. El mensaje SHALL
contener únicamente la versión de protocolo y el tipo `quick_paste`.

#### Scenario: atajo desde una superficie Wayland nativa

- **WHEN** la persona presiona `Ctrl+Shift+V` desde GNOME Terminal, Files,
  Chrome u otra superficie Wayland nativa en una sesión GNOME Wayland
- **THEN** la extensión SHALL comunicar una solicitud local de Quick Paste al
  proceso ClipVault
- **AND** no SHALL incluir contenido del portapapeles, datos de ventana,
  rutas, hashes ni imágenes en el mensaje

#### Scenario: ciclo de vida de la extensión

- **WHEN** la integración GNOME se deshabilita o se recarga
- **THEN** la extensión SHALL desconectar la señal del acelerador y liberar el
  grab registrado
- **AND** no SHALL conservar una solicitud de Quick Paste para una conexión
  posterior

### Requirement: el listener validará eventos de activación tras el handshake

El listener de la integración GNOME SHALL aceptar el tipo `quick_paste` solo
después de `hello`. El listener SHALL comunicarlo a través de un callback de
evento sin modificar el snapshot de foco.

#### Scenario: evento válido posterior al handshake

- **WHEN** un peer conectado envía `hello` seguido de
  `{ "v": 1, "kind": "quick_paste" }`
- **THEN** el listener SHALL invocar una vez el callback de Quick Paste

#### Scenario: evento antes del handshake

- **WHEN** un peer envía `quick_paste` antes de `hello`
- **THEN** el listener SHALL ignorar el mensaje
- **AND** no SHALL invocar el callback

### Requirement: el backend X11 no se usará para un hotkey Wayland

En una sesión Linux Wayland, el bootstrap SHALL no inicializar el backend
global X11 para el atajo de Quick Paste. En Linux X11 SHALL conservar el
preflight y el registro global existentes.

#### Scenario: sesión Wayland

- **WHEN** ClipVault arranca en Linux Wayland
- **THEN** el manager de hotkeys del backend X11 SHALL ser no-op
- **AND** la integración GNOME consentida seguirá pudiendo solicitar Quick
  Paste por el canal local
