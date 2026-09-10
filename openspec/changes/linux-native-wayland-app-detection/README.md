# linux-native-wayland-app-detection

Cambio propuesto para identificar el origen de capturas producidas por aplicaciones nativas Wayland en Linux.

Estado: propuesta. La implementación corresponde a MiniMax; Codex mantiene la arquitectura y el contrato OpenSpec.

El cambio debe mantener el comportamiento existente para X11 y XWayland, y debe degradar de forma tipada cuando el compositor no publique un protocolo compatible. No se debe fabricar un identificador a partir del título de la ventana, del proceso o de heurísticas privadas.
