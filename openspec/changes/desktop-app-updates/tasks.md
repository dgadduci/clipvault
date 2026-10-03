# Tareas: actualizaciones firmadas de la aplicación desktop

## 1. Decisiones y relevamiento

- [x] 1.1 Confirmar el estado actual de versiones, bundling, capability,
  “Acerca de” y workflows de release.
- [x] 1.2 Definir GitHub Releases + Tauri Updater + GitHub Actions para macOS y
  Linux, con instalación confirmada por el usuario y sin servidor propio.
- [x] 1.3 Confirmar los secretos operativos disponibles: clave privada Tauri y
  credenciales de firma/notarización macOS; documentar su configuración segura
  sin incorporar valores al repositorio.

## 2. Integración del updater

- [x] 2.1 Añadir y registrar las dependencias oficiales Tauri updater y process
  en el shell Rust y sus bindings JavaScript.
- [x] 2.2 Activar los bundles requeridos y configurar el endpoint HTTPS,
  artefactos de actualización y clave pública Tauri.
- [x] 2.3 Conceder en las capabilities únicamente los permisos necesarios para
  comprobar, instalar y relanzar la aplicación.
- [x] 2.4 Asegurar que las builds de desarrollo no consulten el endpoint y
  que la comprobación de producción no bloquee el arranque.

## 3. Experiencia de actualización

- [x] 3.1 Implementar un servicio frontend único que gestione comprobación,
  reintento, disponibilidad, descarga, errores y progreso del updater.
- [x] 3.2 Integrar en “Acerca de” el estado de actualización, la versión
  disponible y la acción explícita de instalar y reiniciar.
- [x] 3.3 Añadir las claves y traducciones de todos los estados a `en`, `es`,
  `pt`, `de` y `fr`, manteniendo la paridad de claves y placeholders.
- [x] 3.4 Confirmar que la ausencia de red, rechazo de elevación o fallo de
  instalación conserva la versión actual y permite continuar usando la app.

## 4. Pipeline de releases

- [x] 4.1 Añadir un workflow de GitHub Actions disparado por tags SemVer que
  valide la igualdad de las versiones Cargo, Tauri, frontend y tag.
- [x] 4.2 Construir macOS Apple Silicon/Intel y Linux x86_64 desde los paths y
  comandos admitidos por el repositorio; generar `.dmg`, `.AppImage` y `.deb`.
- [x] 4.3 Firmar artefactos con los secretos Tauri, adjuntar firmas y generar
  `latest.json` mediante `tauri-apps/tauri-action`.
- [x] 4.4 Crear releases en borrador con permisos mínimos de `GITHUB_TOKEN`;
  publicar sólo después de revisar artefactos y manifiesto.
- [x] 4.5 Configurar firma ad hoc de macOS sin identidad Apple y documentar la
  aprobación manual de Gatekeeper para distribución directa.
- [x] 4.6 Documentar la generación y el respaldo seguro de la clave privada,
  además de la instalación manual de la primera versión con updater.

## 5. Verificación y rollout

- [x] 5.1 Validar los manifiestos y enlaces para cada combinación de sistema,
  arquitectura e instalador, incluyendo la actualización `.deb`.
- [ ] 5.2 Completar los escenarios de fallo y recuperación:
  - [x] Actualizar desde un artefacto con firma válida, con confirmación del
    usuario, descarga, instalación y relanzamiento; comprobado en Linux
    AppImage, Linux `.deb` y macOS Apple Silicon.
  - [x] Comprobar el estado sin conexión y reintentar tras restaurar Wi-Fi;
    verificado en Ubuntu con la instalación `.deb` `v0.0.17`.
  - [ ] Rechazar un artefacto con firma inválida o faltante.
  - [ ] Verificar el error y la recuperación ante una interrupción de red
    durante la descarga del artefacto.
  - [ ] Cancelar la autorización elevada de una actualización `.deb` y
    confirmar que la versión instalada sigue funcionando.
  - Nota de alcance (2026-10-02): por decisión del usuario, diferir estas
    pruebas restantes y asumir provisionalmente que funcionan correctamente;
    siguen sin estar verificadas.
- [x] 5.3 Comprobar builds y pruebas relevantes de Tauri/frontend, paridad de
  traducciones, capacidades mínimas y preservación de la base SQLite y assets.
- [ ] 5.4 Completar la verificación manual por formato y arquitectura, y
  registrar los resultados del primer release:
  - [x] Linux AppImage: actualización de `v0.0.16` a `v0.0.17` confirmada;
    la aplicación reinició correctamente y el historial siguió disponible.
  - [x] Linux `.deb`: actualización de `v0.0.16` a `v0.0.17` confirmada
    correctamente en Ubuntu; la instalación inicial mostró un aviso informativo
    del sandbox `_apt`, pero terminó con éxito.
  - [ ] macOS Intel: diferido por falta de un equipo disponible; compatibilidad
    asumida provisionalmente, sin verificación manual.
  - [x] macOS Apple Silicon: actualización de `v0.0.16` a `v0.0.17`
    confirmada sin problemas.
- [x] 5.5 Ejecutar `openspec validate desktop-app-updates --strict` y revisar
  `git diff --check`.
