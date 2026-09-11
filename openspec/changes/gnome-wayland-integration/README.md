# gnome-wayland-integration

Cambio propuesto para que ClipVault pueda identificar aplicaciones nativas Wayland en sesiones GNOME cuando el compositor no expone un protocolo público de foco.

La integración se distribuye dentro de ClipVault, pero se instala y activa sólo después del consentimiento explícito del usuario. Si el usuario la rechaza, la aplicación continúa funcionando con el estado `Unavailable` existente.

Estado: propuesta. Codex define la arquitectura; MiniMax implementa el cambio.
