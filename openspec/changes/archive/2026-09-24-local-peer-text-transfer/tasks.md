# Tareas: arquitectura paraguas de pares locales

> No usar este archivo para implementar código. Cada cambio hijo tiene sus
> propias tareas y debe aplicarse de forma aislada.

- [x] 1.1 Acordar alcance N-pares, texto solamente, transferencia manual,
  preservación de títulos y snapshots locales tras revoke/block.
- [x] 1.2 Documentar identidad segura, opt-in, mDNS local, pairing mutuo,
  TLS/pinning, paginación, límites y exclusiones.
- [x] 1.3 Separar el trabajo en cambios secuenciales y dependientes:
  local-peer-identity-foundation, local-peer-discovery,
  local-peer-mutual-pairing, peer-text-history-browser y peer-text-import.
- [x] 2.1 Aplicar exclusivamente local-peer-identity-foundation y verificarlo
  antes de seleccionar local-peer-discovery.
- [x] 2.2 Aplicar exclusivamente local-peer-discovery y verificarlo antes de
  seleccionar local-peer-mutual-pairing.
- [x] 2.3 Aplicar exclusivamente local-peer-mutual-pairing y verificarlo antes
  de seleccionar peer-text-history-browser.
- [x] 2.4 Aplicar exclusivamente peer-text-history-browser y verificarlo antes
  de seleccionar peer-text-import.
- [x] 2.5 Aplicar exclusivamente peer-text-import y completar la matriz manual
  de Wayland, X11 y macOS.
