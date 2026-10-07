# Diseño: programa de descubrimiento de ClipVault en GitHub

## Enfoque

El trabajo se divide por el momento en que cada pieza se vuelve estable:

1. **Confianza y ayuda.** Alinear los archivos de licencia con
   `Cargo.toml`, establecer cómo reportar problemas y explicar cómo contribuir.
2. **Instalación y primeros pasos.** Validar los artefactos publicados y
   redactar recorridos simples para cada configuración probada.
3. **Página de entrada.** Crear README, imagen social y texto de About cuando
   existan destinos de instalación, ayuda y contribución a los que enlazar.

Los cambios implementadores son OpenSpec independientes, pero se revisan como
un único recorrido desde la página del proyecto hasta la aplicación instalada.

## Fuentes de verdad

- `Cargo.toml` define ahora `GPL-3.0-only`, conforme a la decisión del
  mantenedor; el `LICENSE` de raíz debe contener el texto estándar de esa
  licencia.
- `docs/releases.md` y el workflow de releases describen macOS Apple Silicon e
  Intel, y Linux x86_64 `.AppImage` y `.deb`.
- `project.md` describe los límites de plataforma, pero su roadmap no es una
  fuente para afirmar que una función ya existe.
- Las versiones publicadas y las pruebas manuales aprobadas definen qué
  configuración aparece como comprobada.

Antes de publicar una afirmación, comparar la guía con el instalador y la
versión que la persona realmente puede descargar. Si una prueba o canal no
está confirmado, indicarlo o dejarlo fuera.

## Recorrido público

El README será la entrada principal y enlazará a las guías de instalación, los
releases, la privacidad, el soporte y las contribuciones. Las guías priorizarán
el flujo gráfico y añadirán una alternativa de terminal solo cuando sea útil.
No pedirán instalar Rust, Node.js, Python, SQLite o herramientas de desarrollo
para ejecutar la aplicación.

La matriz de plataformas documentará al menos macOS, Ubuntu GNOME y Arch KDE
Plasma. Para Linux identificará la sesión X11 o Wayland por separado. El
recorrido de Arch no presentará un paquete AUR, `.deb` o instalador nativo si
no existe como distribución oficial; usará el artefacto realmente publicado y
explicará sus límites.

## Idiomas y contenido visual

El contenido público de estos cambios tendrá versiones equivalentes en inglés
y español, con navegación entre idiomas. Las capturas utilizarán una base de
datos de demostración con entradas inventadas. La imagen social será un asset
de repositorio aparte, de 1280×640 px, con el logo actual y una descripción
breve; no incluirá capturas de clipboard real.

La descripción de privacidad debe explicar la operación local sin afirmar que
la aplicación nunca se conecta a Internet: `docs/releases.md` documenta las
comprobaciones del actualizador contra GitHub y los datos técnicos que envía.

## Operaciones fuera del repositorio

La edición del texto About, topics, website y social preview en la página de
GitHub se documentará como un paso manual posterior. Estos specs preparan los
valores y assets, pero no dependen de cambios en la configuración remota.
