# Diseño: guía contextual de configuración

## Principio de aparición

La tarjeta es una sugerencia opcional, no un asistente. Se renderiza después
de que la ventana principal esté lista y la detección local confirme un
entorno aplicable. Su consulta no retrasa la ventana ni bloquea búsqueda,
captura, historial o ajustes.

La tarjeta se ofrece una vez por tipo de integración mientras la persona no
haya tomado una decisión. **Ahora no** guarda una preferencia local de
ocultamiento separada del consentimiento GNOME/KDE. La persona no vuelve a
verla al iniciar, pero puede abrir Integraciones de escritorio desde
Configuración o desde el menú de la bandeja.

## Matriz de comportamiento

| Entorno detectado | Ayuda inicial | Configuración posterior |
| --- | --- | --- |
| GNOME Wayland compatible, sin decisión previa | Tarjeta GNOME con acción Configurar y Ahora no | Integraciones de escritorio |
| KDE Plasma Wayland compatible, sin decisión previa | Tarjeta KDE con acción Configurar y Ahora no | Integraciones de escritorio |
| macOS sin permiso de Accesibilidad | Sin tarjeta de primera ejecución; la guía aparece cuando una acción de pegado lo requiere | Guía contextual de plataforma y destino conocido de Ajustes del Sistema |
| Linux X11 | Sin instalación ni tarjeta de integración | Guía de capacidad solo ante un fallo accionable |
| Otro compositor Wayland o sesión no reconocida | Sin CTA de integración inexistente | Explicación honesta al intentar una función no disponible |

## Estado de presentación

- Mostrar la tarjeta solo cuando el entorno sea compatible, el consentimiento
  siga `unknown` y no exista una preferencia local de ocultamiento para esa
  integración.
- No mostrarla si está conectada, si la persona aceptó y completó la
  configuración, si rechazó, o si la deshabilitó explícitamente.
- La tarjeta dirige al panel correcto y permite que ese panel consulte el
  estado actualizado. No instala, activa, otorga permisos ni acepta el
  consentimiento desde la propia tarjeta.
- Si la detección falla, cerrar ClipVault o cambiar de escritorio no debe
  marcar el aviso como leído.
- El estado de ocultamiento se almacena localmente por identificador estable
  de integración (GNOME o KDE), sin datos de uso ni registro remoto.
- Se reutiliza `app_settings` con las claves booleanas
  `contextual_desktop_setup_guidance_dismissed_gnome` y
  `contextual_desktop_setup_guidance_dismissed_kde`. La ausencia equivale a
  `false`; no se requiere una migración de SQLite.

## No duplicar guías por distribución

La interfaz usa la sesión y la integración detectadas, no nombres de
distribuciones, para decidir qué mostrar. Ubuntu GNOME Wayland y otras
distribuciones con GNOME compatible reciben el mismo recorrido GNOME. KDE
Plasma Wayland usa el recorrido KWin. El texto debe nombrar el escritorio y
las acciones que la persona verá, sin asumir que todas las distribuciones
instalan las mismas aplicaciones auxiliares.

## Idiomas y accesibilidad

Tarjeta, explicación corta, labels, botones y textos accesibles se traducen
en `en`, `es`, `pt`, `de` y `fr`. La tarjeta no roba foco, puede recorrerse
por teclado, tiene contraste y nombres accesibles, y puede descartarse sin
abrir otra pantalla. Tras configurar, la persona puede cerrar el panel y
continuar en la ventana principal.
