# Propuesta: generar iconos válidos para los bundles desktop

## Problema

Al compilar la app con Rust `1.90.0`, la compilación nativa de macOS termina,
pero Tauri no puede crear el bundle porque no encuentra un icono compatible:
`Failed to create app icon: No matching IconType`. El repositorio sólo tiene
`icons/icon.png`, un PNG transparente de 1×1, y la configuración no declara
un icono ICNS para el bundle de macOS.

## Qué cambia

- Añadir un PNG fuente cuadrado de ClipVault, derivado del logo provisto por
  el usuario (portapapeles y katana).
- Generar, con el comando `tauri icon` ya provisto por Tauri CLI, los formatos
  de icono que necesita cada target durante el workflow de release.
- Declarar en el overlay temporal de release los iconos macOS y PNG generados
  para Linux, sin cambiar la configuración de los builds locales.
- No versionar los iconos generados; el PNG fuente es el único arte versionado y el
  workflow los recrea antes de cada build.

## Fuera de alcance

- Cambios en la interfaz o el comportamiento de la aplicación.
- Añadir dependencias o cambiar la clave/firma de Tauri Updater.
- Publicar el release borrador.

## Resultado esperado

Los bundles de macOS contienen un `icon.icns` válido y los bundles de Linux
usan PNG cuadrados generados desde la misma fuente. Los tres targets del
workflow pueden terminar el empaquetado.
