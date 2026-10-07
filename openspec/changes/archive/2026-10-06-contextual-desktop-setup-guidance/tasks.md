# Tareas

## 1. Reconciliación y estado

- [x] 1.1 Confirmar que los cambios de Integraciones de escritorio y su flujo
  de activación están integrados y archivados.
- [x] 1.2 Reutilizar las consultas tipadas de GNOME y KDE; no inferir el
  escritorio a partir de cadenas de distribución o texto libre.
- [x] 1.3 Definir una preferencia local de ocultamiento por integración en el
  almacén existente, sin migración innecesaria.

## 2. Guía contextual

- [x] 2.1 Renderizar la tarjeta solo después de que la ventana esté lista y la
  detección confirme GNOME/KDE Wayland compatible.
- [x] 2.2 Hacer que Configurar abra la sección correcta sin conceder permiso,
  instalar ni activar desde la tarjeta.
- [x] 2.3 Guardar Ahora no separado del consentimiento y recordar la decisión
  por integración.
- [x] 2.4 Suprimir la tarjeta para estados rechazado, activo o deshabilitado;
  conservar la entrada permanente en Configuración general y la bandeja.
- [x] 2.5 Mantener en macOS la guía contextual de Accesibilidad al intentar
  pegar; no mostrar onboarding en Linux X11 ni en Wayland no compatible.
- [x] 2.6 Verificar que la consulta de estado no retrasa ni bloquea la ventana.

## 3. Idiomas, accesibilidad y verificación

- [x] 3.1 Añadir y validar los textos de tarjeta en `en`, `es`, `pt`, `de` y
  `fr`.
- [x] 3.2 Añadir regresiones para aparición, supresión, ocultamiento,
  consentimiento separado, entrada por Settings y cada grupo de plataformas.
- [x] 3.3 Ejecutar verificador de catálogos, checks y pruebas frontend
  afectados, validación OpenSpec y `git diff --check`.
- [x] 3.4 Revisar que no se agregaron permisos, comunicaciones de red,
  telemetría, rutas o datos de usuario a la persistencia.
- [x] 3.5 El usuario aprobó las pruebas manuales en Arch KDE Plasma Wayland y
  Ubuntu GNOME Wayland (2026-10-06).
