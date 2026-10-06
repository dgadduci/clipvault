# Tareas

## 1. Confirmar los mecanismos por escritorio

- [ ] 1.1 Revisar el estado de Git y los artefactos finales de GNOME y KDE antes
  de modificar sus servicios.
- [ ] 1.2 Confirmar por versión qué activación, recarga o apertura de ajustes
  admite cada escritorio mediante interfaces públicas.
- [ ] 1.3 Conservar los límites de privacidad, instalación local, reversibilidad
  y consentimiento de los cambios existentes.

## 2. Reducir acciones sin ocultar pasos reales

- [ ] 2.1 Coordinar consentimiento e instalación GNOME bajo una acción explícita
  en la capa de servicio; mantener el comando Tauri delgado.
- [ ] 2.2 Reutilizar la activación KDE actual como un flujo único y hacer que
  su resultado dependa del estado confirmado.
- [ ] 2.3 Mostrar progreso y errores recuperables sin perder la decisión
  persistida cuando falla la instalación.
- [ ] 2.4 Abrir la gestión de extensiones GNOME mediante un destino seguro
  cuando exista; mantener pasos manuales concisos como fallback.
- [ ] 2.5 Consultar y presentar de nuevo el estado después de iniciar sesión o
  reabrir ClipVault.
- [ ] 2.6 Mantener disponibles desactivar, retirar y cambiar de decisión.

## 3. Localización y validación

- [ ] 3.1 Añadir todos los textos a `en`, `es`, `pt`, `de` y `fr`, con
  placeholders en paridad.
- [ ] 3.2 Añadir pruebas de secuencia, consentimiento, idempotencia, error y
  estado confirmado para GNOME y KDE.
- [ ] 3.3 Ejecutar pruebas/checks relevantes de Rust y frontend, verificación de
  catálogos y `openspec validate`.
- [ ] 3.4 Revisar el diff en busca de APIs privadas, operaciones privilegiadas,
  acceso de red, datos sensibles o estados de éxito prematuros.
