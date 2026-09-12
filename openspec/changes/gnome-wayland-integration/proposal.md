# Propuesta: integración opcional con GNOME Wayland

## Problema

En GNOME Wayland, las aplicaciones nativas como Chrome, Firefox y Terminal pueden publicar un `app_id` hacia GNOME Shell, pero un cliente Wayland normal no dispone de una API pública y portable para consultar qué aplicación tiene el foco. Los protocolos públicos de toplevel disponibles no resuelven este caso en Mutter.

El cambio `linux-native-wayland-app-detection` debe conservar una degradación honesta cuando el compositor no ofrece información. Para cubrir específicamente Ubuntu GNOME Wayland se necesita un componente que se ejecute dentro de GNOME Shell y comunique a ClipVault únicamente el identificador de la aplicación enfocada.

## Objetivo

Agregar una integración GNOME opcional que:

1. detecte GNOME + Wayland al iniciar ClipVault;
2. compruebe si la integración está instalada, habilitada y conectada;
3. ofrezca al usuario instalarla y activarla desde ClipVault;
4. instale la extensión incluida en el paquete de ClipVault, sin descargar código desde Internet;
5. conserve la decisión del usuario de forma persistente;
6. comunique sólo un identificador de aplicación estable (`app_id`/desktop id);
7. alimente la misma caché, PrivacyGate, metadata provider y pipeline de captura existentes;
8. funcione como complemento del adapter Wayland público, no como reemplazo de X11/XWayland.

## Experiencia de usuario acordada

En una sesión GNOME Wayland, si la integración no está activa, ClipVault mostrará una explicación clara y dos acciones:

- **Activar integración GNOME**
- **Ahora no**

La activación requiere consentimiento explícito. No se deben copiar archivos, habilitar extensiones ni modificar preferencias del usuario silenciosamente.

Después de aceptar:

- ClipVault instala la extensión desde sus recursos locales;
- solicita o realiza la activación usando el mecanismo soportado por GNOME;
- informa si es necesario reiniciar GNOME Shell, cerrar sesión o realizar otra acción;
- mantiene el estado en futuros inicios;
- no vuelve a preguntar en cada arranque mientras la integración siga habilitada.

La pantalla de configuración debe permitir comprobar estado, deshabilitar y desinstalar la integración. Deshabilitarla nunca debe impedir que ClipVault capture y pegue contenido.

## Seguridad y privacidad

La extensión tendrá privilegios dentro de GNOME Shell, por lo que su instalación debe ser visible, consentida y reversible. Las extensiones GNOME se ejecutan como código del Shell y deben liberar señales, recursos y conexiones al deshabilitarse. [Modelo de extensiones GNOME](https://wiki.gnome.org/Attic/GnomeShell/Extensions/Writing)

La extensión y ClipVault:

- no leerán el portapapeles;
- no leerán ni enviarán títulos de ventanas;
- no leerán PID, `/proc`, rutas de procesos ni variables de entorno;
- no ejecutarán `xprop`, `wmctrl`, `gdbus`, `org.gnome.Shell.Eval` ni comandos externos;
- no enviarán contenido, snippets, hashes, `asset_ref` ni rutas absolutas;
- intercambiarán sólo estado de conexión, versión y `app_id` normalizado;
- tratarán el `app_id` con el mismo modelo de confianza que `WM_CLASS`: es metadata declarada por la aplicación, no una prueba de identidad del proceso.

## Distribución

La extensión debe formar parte de los recursos del paquete de ClipVault. La primera versión no debe depender de una descarga desde extensions.gnome.org ni de red.

La instalación se realizará para el usuario actual, en el directorio estándar de extensiones de usuario de GNOME, por ejemplo:

`~/.local/share/gnome-shell/extensions/<uuid>/`

La escritura debe ser atómica, no debe sobrescribir una extensión ajena con el mismo UUID y debe dejar una instalación incompleta fuera de servicio. La extensión tendrá un UUID propio y estable de ClipVault.

## Fuera de alcance

- Cambiar GNOME Shell o instalar paquetes del sistema.
- Requerir privilegios de root.
- Instalar una extensión sin consentimiento.
- Resolver aplicaciones nativas en todos los compositores Wayland mediante este mecanismo.
- Hacer que la extensión controle ventanas, mueva ventanas, lea contenido o agregue UI innecesaria al panel GNOME.
- Reemplazar el adapter público Wayland ni el fallback X11/XWayland.
- Prometer detección si la versión de GNOME Shell no es compatible.

## Resultado esperado

En Ubuntu GNOME Wayland, tras aceptar la activación, una captura de Chrome, Firefox o Terminal obtiene `source_app`; el provider Linux existente resuelve nombre e icono y el blacklist puede actuar antes de persistir.

Si la integración no está instalada, fue rechazada, está deshabilitada o es incompatible, ClipVault continúa sin fallar y muestra `Unavailable`/aplicación desconocida según el contrato actual.

## Validación de runtime y límite actual

La validación manual reportada en Ubuntu GNOME Wayland confirmó el transporte:
la sesión `linux_wayland` tiene consentimiento `accepted`, el estado técnico
alcanza `identified` y el diagnóstico publica `window:6`. Esto confirma que la
extensión habilitada completa el handshake local y comunica únicamente un
identificador opaco de la aplicación enfocada.

Ese hito no garantiza que cada identificador pueda resolverse a un icono
renderizable. Actualmente ClipVault conserva la detección de origen de las
capturas (por ejemplo Chrome, Firefox, Terminal y Warp) en X11 y Wayland, pero
en Wayland no muestra el icono de la aplicación. La resolución y presentación
de iconos para identificadores Wayland será un cambio OpenSpec posterior: no
forma parte de esta integración metadata-only ni autoriza a la extensión a
transportar iconos, rutas, títulos o contenido adicional.

La build Linux normal de ClipVault debe incluir la feature del shell que
expone esta integración. `feature_disabled` sólo es admisible en una build
reducida solicitada intencionalmente; no puede ser el resultado de `cargo
tauri dev` ni del artefacto Linux estándar, porque impediría al usuario dar
su consentimiento e instalar el recurso local.

La extensión incluida debe invocar las APIs GI y las propiedades GObject con el
contrato que publica la versión de GNOME Shell declarada como compatible. En particular, una
integración instalada y habilitada que no pueda abrir el socket local no puede
quedar indefinidamente en `activation_pending` por una llamada JavaScript
inválida: debe poder completar el `hello` o informar un error de comunicación
tipado, sin afectar la captura normal.
