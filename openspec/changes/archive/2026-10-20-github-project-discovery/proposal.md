# Propuesta: mejorar el descubrimiento y la primera experiencia de ClipVault

## Problema

El repositorio no tiene un README en la raíz ni guías públicas de instalación
y primeros pasos. Una persona que llega desde GitHub no puede entender con
rapidez qué hace ClipVault, qué configuraciones se probaron, cómo instalarlo
sin herramientas de desarrollo ni dónde pedir ayuda. Además, no hay un archivo
de licencia en la raíz. El mantenedor eligió GNU GPL versión 3 para el código
del proyecto.

## Objetivo

Preparar una experiencia pública coherente que ayude a una persona nueva a:

1. entender el propósito y el estado real de ClipVault;
2. elegir un instalador oficial para su sistema;
3. completar la primera ejecución con la menor cantidad de pasos posible;
4. distinguir configuraciones probadas de las que aún no se verificaron; y
5. encontrar instrucciones confiables para soporte, reportes y contribuciones.

## Organización del trabajo

Este cambio es el spec paraguas. Los tres cambios OpenSpec independientes que
lo implementan, en orden, son:

1. `repository-trust-and-support`: licencia, seguridad, contribuciones y
   plantillas de reportes.
2. `platform-installation-guides`: instalación y primeros pasos en macOS,
   Ubuntu GNOME y Arch KDE Plasma, con X11 y Wayland documentados por separado
   solo donde exista verificación.
3. `github-readme-project-identity`: README bilingüe, identidad del proyecto,
   imagen social y metadata recomendada para GitHub; se ejecuta al final para
   enlazar las guías y canales ya establecidos.

Cada cambio implementador conserva su propuesta, diseño, requisitos y tareas
propios para poder ejecutarse y validarse por separado.

## Límites

- El README y las guías estarán disponibles en inglés y español.
- Los textos describirán solo funciones, instaladores y pasos comprobados en
  una versión publicada; el roadmap no se presentará como funcionalidad actual.
- La matriz distinguirá soporte declarado, prueba manual aprobada y estado
  pendiente. Probar un sistema no implicará soporte universal para todos sus
  escritorios o sesiones.
- No se publicarán instaladores, no se cambiarán configuraciones del
  repositorio remoto y no se anunciará soporte para Windows como parte de estos
  cambios.
- Las capturas aportadas por el usuario solo se publicarán con su aprobación
  explícita. No se publicarán credenciales ni secretos.

## Resultado esperado

Una persona que descubre ClipVault en GitHub puede comprenderlo, descargar una
versión oficial, seguir una guía en su idioma y encontrar canales adecuados de
ayuda. La información pública no promete configuraciones sin probar.
