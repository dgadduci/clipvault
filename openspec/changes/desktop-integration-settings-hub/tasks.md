# Tareas

## 1. Reconciliación

- [x] 1.1 Revisar el estado de Git y el diff de los cambios que modifican
  Configuración general, la barra superior y las integraciones GNOME/KDE.
- [x] 1.2 Preservar los contratos activos de atajos y toolbar; trasladar la
  gestión de integraciones fuera de Development y dejar los demás cambios
  activos abiertos y sin archivar.
- [x] 1.3 Confirmar el acceso actual a Configuración desde la ventana y la
  bandeja, y los contratos de estado/acciones de ambos adapters.

## 2. Vista para el usuario

- [x] 2.1 Añadir Integraciones de escritorio a Configuración general solo
  cuando la sesión detectada sea compatible.
- [x] 2.2 Reubicar los controles GNOME/KDE existentes y quitar de Development
  las tarjetas, estados y accesos de gestión de integraciones.
- [x] 2.3 Presentar estados y acciones válidas sin duplicar consentimiento,
  comandos, listeners ni estados técnicos.
- [x] 2.4 Mantener correctos teclado, foco, etiquetas accesibles y retorno al
  cerrar la vista.

## 3. Idiomas y verificación

- [x] 3.1 Actualizar `en`, `es`, `pt`, `de` y `fr`, manteniendo claves y
  placeholders en paridad.
- [x] 3.2 Añadir regresiones dirigidas para GNOME, KDE, plataformas donde no
  aplica, actualización del estado y presentación de errores.
- [x] 3.3 Ejecutar el verificador de catálogos, los checks frontend afectados,
  pruebas relevantes y `openspec validate`.
- [x] 3.4 Revisar el diff y confirmar que no se agregaron dependencias,
  comandos privilegiados, accesos de red ni datos sensibles.
- [x] 3.5 Corregir el acceso desde la bandeja antes de cargar diagnósticos y
  retirar por completo de Development la interfaz de integraciones.
