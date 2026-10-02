# Diseño: generar iconos válidos para los bundles desktop

## Decisión

Versionar un único PNG cuadrado de ClipVault como fuente visual y ejecutar
`cargo tauri icon` en cada job de release después de instalar Tauri CLI y
antes de empaquetar. Tauri genera `icon.icns` para macOS y PNG de tamaños
comunes para Linux. El overlay temporal `tauri.release.conf.json` declara esos
archivos generados como iconos del bundle. La configuración base de
desarrollo conserva el PNG que ya existe, por lo que no referencia archivos
generados ausentes al compilar localmente.

El PNG parte del logo provisto por el usuario y conserva el portapapeles y la
katana sobre un fondo azul oscuro. Se suavizó el ruido de la imagen fuente para
mantener el contorno legible en tamaños pequeños. El workflow genera los
formatos de plataforma a partir de este único archivo.

## Alternativas descartadas

- Conservar el PNG actual: tiene 1×1 píxel y no define un tipo de icono
  compatible con macOS.
- Añadir a mano sólo `icon.icns`: duplicaría la fuente visual y permitiría que
  los iconos macOS y Linux diverjan.
- Usar el icono predeterminado de Tauri: identifica a Tauri, no a ClipVault.

## Verificación

El workflow debe generar los archivos esperados desde el PNG en el workspace
efímero del runner, validar el cambio con OpenSpec y terminar el build de
Apple Silicon, Intel y Linux. Git no debe acumular PNG/ICNS derivados ni
alterar datos locales de ClipVault.
