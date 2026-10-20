## 1. Alinear contratos y preparar el cambio

- [x] 1.1 Alinear el delta pendiente de
  `quick-paste-text-editing` con este flujo: rich abre la advertencia al usar
  menú o atajo e imagen conserva el aviso informativo; verificar que ambos
  cambios OpenSpec no describan resultados contradictorios. `openspec validate
  quick-paste-text-editing --strict --type change` → válido.
- [x] 1.2 Confirmar en los cambios aprobados que editar imágenes, preservar
  formato rich y añadir migraciones o dependencias siguen fuera del alcance;
  verificar la propuesta y los deltas OpenSpec.

## 2. Advertencia y estado del editor

- [x] 2.1 Añadir claves localizadas para título, advertencia, confirmar y
  cancelar en `en`, `es`, `pt`, `de` y `fr`; verificar paridad de claves y
  placeholders con `npm run locales:check` → 821 claves en los cinco catálogos.
- [x] 2.2 Implementar el aviso accesible previo a la edición rich y verificar
  que cancelar solo cierra el aviso, sin comando de persistencia, con la prueba
  dirigida `editableTextCaptures.test.ts`.
- [x] 2.3 Reutilizar el modal de texto plano con el `content` canónico y el
  título resuelto; verificar que Guardar está deshabilitado para texto vacío o
  byte por byte igual al original y que Escape/Cancelar no mutan la entrada;
  prueba dirigida `quickPasteTextEditing.test.ts` y `npm run check`.

## 3. Conectar Historial y Quick Paste

- [x] 3.1 Conectar menú y atajo de cards al mismo flujo de advertencia para
  capturas rich, manteniendo el aviso actual para imágenes; verificar la ruta
  con `editableTextCaptures.test.ts` y `editTextShortcutFix.test.ts`.
- [x] 3.2 Conectar menú y `Cmd/Ctrl+E` de Quick Paste al mismo flujo, resolver
  la fila seleccionada por ID y conservar query, selección y foco al cancelar
  o guardar; suites dirigidas `quickPasteActions.test.ts` y
  `quickPasteTextEditing.test.ts`.
- [x] 3.3 Mantener los contratos de cards y drag-and-drop protegidos al añadir
  la acción; verificar `data-testid`, `data-entry-id`, `draggable="false"`,
  payload ID-only, pointer capture, fallback de mouse y cancelaciones con
  `desktopHeaderCardDnd.test.ts` y `quickPasteLayoutContract.test.ts`.
- [x] 3.4 Preservar el rail de capturas en una fila horizontal de altura fija
  con scroll, y fijar una altura de viewport independiente de la cantidad de
  colecciones; mantener claros los textos de colecciones y equipos vinculados.
  Inspeccionar las reglas CSS fuente y compiladas.

## 4. Convertir de forma atómica en el backend

- [x] 4.1 Añadir una operación tipada explícita de conversión rich-a-plain que
  vuelva a validar en el backend ID, tipo y referencias rich; verificar
  resultado de texto sin cambios, vacío, no rich y duplicado con la suite
  focalizada de `clipvault-db`.
- [x] 4.2 Guardar texto no vacío y modificado en la misma transacción SQLite,
  recalcular hash, tamaño y tipo, limpiar metadata rich y preservar ID,
  timestamps no afectados, título, favorito, origen, tags y colecciones;
  verificar con las pruebas focalizadas de repositorio y core.
- [x] 4.3 Recolectar assets rich antiguos solo después del commit y cuando no
  tengan referencias restantes; verificar que un asset compartido sigue
  disponible con `rich_conversion_keeps_assets_referenced_by_another_entry`;
  assets sin referencias se eliminan después del guardado con
  `rich_conversion_collects_its_assets_after_successful_commit`.
- [x] 4.4 Exponer la operación mediante comando Tauri delgado y bridge tipado,
  emitir el evento metadata-only existente y mantener contenido fuera de logs,
  diagnósticos y eventos; la prueba dirigida de `editableTextCaptures.test.ts`
  verifica bridge/evento metadata-only y `cargo check -p clipvault-app` pasa.

## 5. Verificación y entrega

- [x] 5.1 Ejecutar pruebas frontend dirigidas para warning, edición sin cambio,
  guardado, errores, focus, shortcut y refresco en ambas superficies; ejecutar
  también regresiones de Quick Paste y drag-and-drop afectadas → 7 suites
  dirigidas, todas pasan.
- [x] 5.2 Ejecutar pruebas Rust relevantes para validación, duplicados,
  atomicidad, limpieza de referencias y persistencia tras reinicio; verificar
  suites focalizadas de DB, core y assets; `cargo fmt --all -- --check`,
  `npm run check` y `npm run build` pasan.
- [x] 5.3 Validar todos los catálogos de idiomas, ejecutar
  `openspec validate rich-text-plain-editing-conversion --strict --type change`
  y revisar `git diff --check` y el diff completo.
- [x] 5.4 Prueba manual aprobada por el usuario en Ubuntu Linux Wayland para
  Historial y Quick Paste: cancelar advertencia, confirmar y cancelar editor,
  intento sin cambios, guardar texto modificado, reabrir como texto plano y
  verificar que imagen sigue sin editar. macOS no se probó en esta ronda.
