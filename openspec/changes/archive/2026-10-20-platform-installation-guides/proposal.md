# Propuesta: guías simples de instalación y primeros pasos

## Problema

ClipVault publica artefactos para macOS y Linux, pero una persona nueva no
dispone de una guía que le diga qué archivo elegir, cómo abrirlo y qué esperar
en su sistema y escritorio. Ubuntu GNOME, Arch KDE Plasma, X11 y Wayland no
deben tratarse como si fueran el mismo entorno.

## Qué cambia

- Crear un índice de instalación con versiones completas en inglés y español.
- Documentar macOS, Ubuntu GNOME y Arch KDE Plasma usando únicamente los
  formatos oficiales publicados y las pruebas disponibles. La tabla de
  descargas describirá la familia de distribuciones adecuada para cada
  paquete Linux, diferenciando ese alcance de las configuraciones probadas.
- Priorizar la instalación gráfica y ofrecer una alternativa de terminal
  únicamente cuando simplifique la recuperación de un error común.
- Explicar los pasos de primera ejecución, permisos o integración opcional que
  estén presentes en la versión publicada.
- Mantener una matriz explícita de configuraciones probadas, pendientes y no
  ofrecidas como objetivo actual.

## Fuera de alcance

- Crear paquetes nuevos, repositorios AUR, Flatpak, Snap o RPM.
- Declarar que todas las distribuciones Linux o todos sus escritorios están
  probados o soportados porque una configuración concreta haya pasado pruebas
  o porque exista un formato de paquete apropiado.
- Pedir que el usuario final compile la aplicación o instale toolchains.
- Inventar pasos de permisos o habilitar integraciones mediante comandos que
  no estén verificados.

## Resultado esperado

Una persona puede elegir una guía por idioma y configuración, descargar el
artefacto correcto del release oficial y completar su primer inicio con pasos
concisos y comprobados.
