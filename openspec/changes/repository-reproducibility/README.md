# Repository reproducibility

Este cambio endurece el flujo de desarrollo multiplataforma de ClipVault.
Ubuntu ya fue probado manualmente con resultado 10/10 sobre la rama
`fix/linux-x11-compatibility`; este cambio documenta y automatiza la base para
que las siguientes modificaciones se implementen en una rama aislada, se
validen en macOS y Ubuntu y lleguen a `main` con el mismo contrato.

El cambio no modifica la funcionalidad del portapapeles, la UI ni los adapters
de plataforma. No archiva `linux-x11-compatibility`, no borra datos de
`~/.clipvault` y no sustituye las pruebas manuales de hotkeys, captura, foco,
tray o pegado.
