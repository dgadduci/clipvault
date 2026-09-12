# Prompt para MiniMax

Implementa únicamente el cambio OpenSpec
`openspec/changes/linux-app-icon-package-variants/` en el workspace de
ClipVault. Codex ya definió la arquitectura; no inventes un alcance distinto,
no implementes otros cambios y no hagas commit, push ni archive.

Contexto manual: X11 y Wayland reconocen la fuente y la mayoría de los iconos,
pero Firefox y xterm no muestran icono. En el host de diagnóstico Firefox Snap
usa `firefox_firefox.desktop` con `Icon=/snap/firefox/current/default256.png`;
xterm usa `debian-xterm.desktop`, no tiene WM_CLASS declarada, tiene
`Exec=xterm` e `Icon=mini.xterm`. El problema es la divergencia entre identidad
activa, archivo `.desktop` e icono empaquetado.

Lee antes de modificar: `project.md`, `AGENTS.md`, todos los artefactos de
`linux-wayland-desktop-file-icons`, y esta carpeta completa. Respeta el
contrato local-first y metadata-only.

Implementa las tareas pendientes en `tasks.md`:

1. En `crates/clipvault-platform/src/runtime/linux_app_metadata.rs`, agrega un
   alias interno `exec_basename` derivado de un primer token seguro de
   `Exec=`. No ejecutes comandos, no uses shell, no aceptes códigos de campo,
   prefijos, substrings ni similitud. Mantén el orden
   `StartupWMClass` → `X-GNOME-WMClass` → filename stem → `exec_basename`.
   Los IDs con `.desktop` siguen siendo estrictos.
2. Agrega `MatchStrategy::ExecBasename` y el string `exec_basename` sin
   exponer el comando o la ruta. `source_app` no se transforma.
3. Amplía únicamente el allowlist local del provider para raíces de payload de
   paquetes detectadas explícitamente, incluyendo Snap (`/snap` y la raíz
   local de Snap cuando exista) y la variante Flatpak disponible. No permitas
   `/`, `/tmp` ni el home completo. Canonicaliza, exige archivo regular y
   rechaza symlinks que escapen.
4. Reutiliza la resolución PNG/SVG, validación, rasterización, escritura
   atómica y namespace `application-icons/`. Conserva
   `IconFailureKind::OutOfRoots`, fallback, assets previos y el bridge actual.
5. Agrega tests representativos para Firefox Snap, xterm, rutas fuera de
   allowlist, symlink escapado, alias parcial y precedencia existente. Usa
   filesystem temporal/memory-backed; nunca escribas `~/.clipvault` ni borres
   o renombres assets persistidos.
6. Si el cambio toca bootstrap, metadata, cards, colecciones o listeners,
   ejecuta además las regresiones frontend de drag-and-drop. Conserva
   `pointerDragAndDrop.ts`, `data-testid="history-card"`, `data-entry-id`,
   `draggable="false"`, pointer capture, cancelaciones y payload opaco.

No agregues campos al socket GNOME, no transportes rutas/iconos/bytes, no uses
títulos/PID/`/proc`/D-Bus/red, no cambies SQLite, PrivacyGate, búsqueda,
Quick Paste ni el DTO de cards. Si la implementación revela una decisión
arquitectónica distinta —por ejemplo, una raíz de paquetes que no puede
expresarse con el adaptador existente— detente y solicita actualizar
OpenSpec antes de continuar.

Verifica proporcionalmente:

```text
cargo fmt --all -- --check
cargo test -p clipvault-platform --all-targets
cargo clippy -p clipvault-platform --all-targets -- -D warnings
cd app/tauri/frontend && npm run check && npm run build && npm test
cd ../.. && openspec validate linux-app-icon-package-variants --strict --type change
```

Marca cada tarea inmediatamente después de verificarla, informa cualquier
fallo de entorno por separado y entrega un resumen de archivos, tests y
pendientes. No hagas commit ni push.
