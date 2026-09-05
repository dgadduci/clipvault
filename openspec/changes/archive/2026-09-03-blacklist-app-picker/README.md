# blacklist-app-picker

Este cambio mejora la configuración de aplicaciones ignoradas de ClipVault.
La ventana de Privacidad dejará de pedir al usuario que conozca y escriba
identificadores como com.apple.Terminal. En macOS ofrecerá un selector nativo
que comienza en /Applications, valida el bundle seleccionado y obtiene su
identificador, nombre e icono para construir la fila visual de la blacklist.

La selección y extracción pertenecen a clipvault-platform, la persistencia y
las reglas a clipvault-core/clipvault-db, y la UI Svelte sólo orquesta comandos
y muestra DTOs. No se cambia la lógica del PrivacyGate ni la detección de la
aplicación activa durante la captura.

El cambio queda activo hasta completar la revisión manual. No debe archivarse
automáticamente.
