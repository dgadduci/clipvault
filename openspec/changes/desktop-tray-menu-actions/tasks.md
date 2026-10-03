# Tareas: acciones del menú de bandeja

## 1. Relevamiento y contrato

- [x] 1.1 Revisar el menú Tauri, el mapeo de `TrayAction` y su despacho.
- [x] 1.2 Revisar los flujos existentes de confirmación de limpieza,
  Configuración general y Acerca de.
- [x] 1.3 Confirmar el predicado de limpieza: no favorito y sin membresía de
  colección de usuario, excluida la membresía obligatoria en `Historial`.

## 2. Despacho de acciones de bandeja

- [x] 2.1 Hacer idempotente `Open ClipVault`: restaurar, mostrar y enfocar la
  ventana principal existente, o recrearla desde la configuración canónica de
  Tauri si falta, sin crear duplicados.
- [x] 2.2 Quitar la entrada y el despacho de `Favorites` solo del menú nativo;
  conservar favoritos en el resto de la aplicación.
- [x] 2.3 Conectar `Clear History` con la confirmación existente y con la
  operación de limpiar historial no organizado; cancelar no muta datos.
- [x] 2.4 Conectar `Settings` con el modal existente de Configuración general,
  mostrando y enfocando primero la ventana principal.
- [x] 2.5 Agregar `About` al menú localizado y abrir el modal existente Acerca
  de, sin duplicar la ventana o el modal.
- [x] 2.6 Mantener el callback nativo como adaptador delgado y entregar las
  acciones a los flujos existentes del frontend y Rust.
- [x] 2.7 Actualizar el runtime Tauri a 2.12.1 para incluir la corrección
  upstream de decoraciones y controles de título en Wayland.

## 3. Especificaciones y localización

- [x] 3.1 Sincronizar el contrato de bandeja en `desktop-platform-integration`
  y ampliar el punto de acceso único al modal Acerca de en
  `clipboard-history-cards`.
- [x] 3.2 Agregar y traducir la etiqueta `About` en `en`, `es`, `pt`, `de` y
  `fr`; comprobar paridad de claves.

## 4. Verificación

- [x] 4.1 Cubrir ventana existente oculta/minimizada y activaciones repetidas
  sin ventanas duplicadas.
- [x] 4.2 Cubrir ausencia de `Favorites`, despacho de `Clear History`,
  cancelación y conservación de favoritos/colecciones.
- [x] 4.3 Cubrir la apertura de Configuración general y del modal Acerca de
  desde la bandeja, incluida la reutilización del modal About.
- [x] 4.4 Ejecutar checks afectados, regresiones frontend relevantes y
  `openspec validate desktop-tray-menu-actions --strict`.
- [x] 4.5 Revisar el diff, `git diff --check`, los cinco catálogos y confirmar
  que no se añadieron dependencias, migraciones, secretos ni archivos
  generados.
- [ ] 4.6 Verificar manualmente el menú de bandeja en las plataformas cubiertas
  cuando no pueda reproducirse automáticamente.
- [ ] 4.7 Verificar en Linux Wayland el ciclo cerrar/ocultar → reabrir desde la
  bandeja y probar minimizar, maximizar y cerrar sin resize o reinicio previo.
