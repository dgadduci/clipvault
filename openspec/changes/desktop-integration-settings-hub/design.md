# Diseño: configuración accesible de integraciones de escritorio

## Experiencia

La persona abre **Configuración** desde la ventana principal o el menú del
icono de ClipVault y encuentra una opción **Integraciones de escritorio**.
Esta opción abre una vista de configuración normal y accesible, sin exigir que
la persona conozca el menú Development.

La opción se presenta en Linux cuando la sesión detectada corresponde a una
integración GNOME/KDE compatible. En macOS y en Linux X11 no se presenta como
configuración necesaria. Un entorno Wayland sin integración compatible no
recibe una opción que prometa activar algo que ClipVault no puede ofrecer.

## Contenido de la vista

- Mostrar la integración que corresponde al escritorio detectado: GNOME
  Shell en GNOME Wayland o KWin en KDE Plasma Wayland.
- Consultar el estado al abrir la vista y ofrecer una acción localizada para
  actualizarlo.
- Expresar los estados como frases breves: disponible para configurar,
  configurada y conectada, requiere una acción del usuario, desactivada o no
  compatible.
- Mostrar solo las acciones válidas para el estado actual y reutilizar los
  comandos y servicios existentes para configurar, reintentar, desactivar o
  desinstalar.
- Mantener en Development el diagnóstico detallado. La vista de usuario no
  muestra nombres de backend, versión de protocolo, identificadores de
  aplicación, rutas, variables de entorno ni errores técnicos sin traducir.

## Compatibilidad con las preferencias existentes

La pantalla nueva consume los estados tipados existentes de GNOME y KDE; no
crea una segunda fuente de estado ni modifica los valores de consentimiento.
Los estados `declined` y `disabled` se conservan. En una sesión compatible,
la persona puede cambiar su decisión desde Integraciones de escritorio sin
volver a buscar Development.

Las acciones de atajos de teclado continúan en su modal existente. Esta vista
puede explicar que ciertos atajos globales de Wayland dependen de la
integración, pero no duplica su editor ni cambia sus bindings.

## Plataforma y límites

- GNOME Wayland: explicar que la integración local puede aportar el origen de
  la aplicación y los atajos globales declarados por GNOME; no prometer
  pegado sintético en Wayland.
- KDE Plasma Wayland: explicar las capacidades reales de la integración KWin;
  confirmar el estado mediante el adapter existente.
- Linux X11 y macOS: no ofrecer una instalación de integración Wayland.
- Otro compositor Wayland: mantener el comportamiento y las guías de
  capacidad existentes; no sugerir permisos genéricos ni pasos de GNOME/KDE.

## Idiomas y accesibilidad

Todo texto visible o accesible pertenece a los catálogos `en`, `es`, `pt`,
`de` y `fr`. Las claves y los placeholders deben coincidir. La vista conserva
foco visible, orden de teclado, etiquetas accesibles y retorno de foco de los
modales según los componentes existentes.
