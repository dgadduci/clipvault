## 1. Contrato de plataforma

- [x] 1.1 Definir PlatformIssueKind, PlatformGuidance y
  PlatformSettingsTarget serializables y sin contenido del clipboard.
- [x] 1.2 Agregar SettingsNavigator y errores/resultados tipados para apertura
  de configuración.
- [x] 1.3 Extender los errores/respuestas de paste para transportar guidance
  sin romper los consumidores existentes.

## 2. Diagnóstico macOS/Linux

- [x] 2.1 Implementar preflight de permiso de Accesibilidad para CGEvent en
  macOS.
- [x] 2.2 Clasificar permiso faltante, backend fallido y sesión no soportada
  como causas distintas.
- [x] 2.3 Implementar SettingsNavigator macOS con deep link validado y fallback
  a Ajustes del Sistema.
- [x] 2.4 Generar instrucciones Linux para X11, Wayland y display desconocido.
- [x] 2.5 Abrir settings Linux únicamente cuando exista un destino de entorno
  conocido y seguro; en los demás casos mostrar pasos manuales.

## 3. Core y Tauri

- [x] 3.1 Propagar PlatformGuidance desde PasteService hasta PasteResponse.
- [x] 3.2 Agregar clipvault_open_platform_settings como comando Tauri delgado.
- [x] 3.3 Permitir refrescar la matriz de capacidades después de volver de
  Ajustes del Sistema.
- [x] 3.4 Mantener la entrada histórica intacta ante cualquier fallo o
  cancelación.

## 4. Frontend

- [x] 4.1 Crear una modal accesible para permisos y limitaciones de plataforma.
- [x] 4.2 Mostrar título, explicación, pasos y estado de reintento.
- [x] 4.3 Mostrar Abrir configuración únicamente cuando corresponda.
- [x] 4.4 Conectar Abrir configuración, Reintentar y Cerrar.
- [x] 4.5 Mostrar fallback manual cuando el deep link no pueda abrirse.
- [x] 4.6 Usar textos específicos para macOS, Linux X11 y Linux Wayland.

## 5. Tests

- [x] 5.1 Tests de clasificación de causas y serialización sin payload.
- [x] 5.2 Tests de preflight macOS con fake de permiso.
- [x] 5.3 Tests de navigator: abierto, fallback y error.
- [x] 5.4 Tests del flujo de paste: guidance visible e historial sin cambios.
- [x] 5.5 Tests de Wayland sin opción falsa de abrir settings.
- [x] 5.6 Tests de X11 sin display/XTest.
- [x] 5.7 Tests de UI/TypeScript para modal y reintento.

## 6. Verificación

- [x] 6.1 Ejecutar cargo fmt --all -- --check.
- [x] 6.2 Ejecutar cargo clippy --workspace --all-targets -- -D warnings.
- [x] 6.3 Ejecutar cargo test --workspace.
- [x] 6.4 Ejecutar npm run check y npm run build en el frontend.
- [x] 6.5 Ejecutar openspec validate platform-permission-guidance --strict.
- [x] 6.6.a Verificación automática: la matriz `synthetic_paste` ahora
  se calcula con `detect_capabilities_runtime`, que invoca
  `CGPreflightPostEventAccess` en macOS; los tests determinísticos
  cubren el caso sin permiso (`false`), con permiso (`true`), el
  refresco tras conceder el permiso, el pegado explícito posterior y
  el historial intacto cuando el pegado falla. La guía serializada
  no contiene texto del portapapeles, secretos, tokens ni claves
  privadas. La función `retryGuidance` del frontend no invoca el
  comando de pegado (test TypeScript en `tests/guidance.test.ts`).
- [ ] 6.6.b Pendiente de verificación manual en una sesión real de
  macOS con y sin Accesibilidad y en Linux X11/Wayland cuando haya
  hosts disponibles. Esta parte no puede automatizarse de forma
  fiable (hotkeys, panel de Ajustes del Sistema, captura y pegado
  reales) y requiere una sesión interactiva. No se ha marcado como
  completada.
