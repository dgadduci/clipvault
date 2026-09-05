# clipboard-rich-content

Este cambio agrega soporte para imágenes estáticas del portapapeles como la
primera extensión más allá del texto. La imagen se persiste como un asset PNG
local y la entrada SQLite conserva sólo metadata y una referencia relativa
segura.

El cambio reutiliza `HistoryCard`, `HistoryCardRail`, quick-paste, favoritos,
eliminación, retención, PrivacyGate y el bridge de assets local. No implementa
todavía HTML binario, archivos, audio, OCR ni transformaciones.

## Implementación

MiniMax debe ejecutar:

```text
/opsx-apply clipboard-rich-content
```

El cambio no debe archivarse automáticamente. La prueba manual de imagen en
macOS/Linux debe permanecer pendiente hasta verificarse en una sesión real.
