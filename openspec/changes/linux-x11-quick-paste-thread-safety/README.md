# linux-x11-quick-paste-thread-safety

Corrección de la apertura de ClipPaste (la ventana Quick Paste de ClipVault)
cuando el usuario pulsa `Ctrl+Shift+V` en Linux X11. El síntoma actual es el
aborto del proceso por la aserción de XCB:

```text
[xcb] Unknown sequence number while processing queue
[xcb] Most likely this is a multi-threaded client and XInitThreads has not been called
```

El cambio está preparado para implementación por MiniMax. No modifica el
código, no archiva cambios y no debe mezclar la corrección con refactors de
frontend, cards, assets o drag and drop.
