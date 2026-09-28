# Tareas de implementación

MiniMax implementa las tareas pendientes. Codex mantiene el diagnóstico y los
contratos OpenSpec; no se modifican datos del usuario ni assets persistidos.

## 1. Reproducción y causa raíz

- [x] 1.1 Confirmar el estado limpio, revisar los cambios activos relevantes y
  registrar el baseline sin modificar `~/.clipvault`.
- [x] 1.2 Revisar el grafo de features del binario Linux realmente ejecutado,
  incluyendo `arboard`, `wayland-data-control`, `wl-clipboard-rs`,
  `linux-x11` y el comando de desarrollo/empaquetado.
- [x] 1.3 Reproducir en Arch Linux KDE Plasma Wayland con al menos una
  aplicación nativa Wayland; registrar versiones de distro/compositor,
  backend compilado, backend seleccionado y resultado metadata-only.
- [x] 1.4 Determinar si falla la selección/inicialización del backend, la
  lectura, la señal de cambio/revisión, el PrivacyGate o la persistencia.
  Registrar la causa raíz comprobada en este cambio antes de cerrar la tarea.
- [x] 1.5 Verificar la hipótesis de lectura: si la feature Wayland de `arboard`
  está deshabilitada, probarla habilitada en una build Linux de diagnóstico y
  confirmar si KDE entrega texto y los formatos ya soportados.
- [x] 1.6 Verificar la hipótesis del contador: confirmar si XFixes observa la
  misma copia nativa Wayland y si una revisión conocida puede hacer que
  `CaptureWatcher` ignore un payload distinto. Actualizar el diseño si la
  evidencia contradice la hipótesis.

## 2. Adapter Linux

- [x] 2.1 Implementar selección del transporte Linux coherente con la sesión:
  native Wayland cuando esté disponible, X11 en X11 y fallback XWayland
  explícito sólo cuando resulte utilizable.
- [x] 2.2 Alinear la revisión de clipboard con el transporte efectivo. No usar
  una revisión X11 como fuente de verdad para lecturas Wayland nativas; usar
  revisión nativa confiable o `UNKNOWN` y el fallback de fingerprint existente.
- [x] 2.3 Mantener texto, texto enriquecido e imágenes en el pipeline actual,
  el `PrivacyGate` previo a persistencia y el único `CaptureWatcher`.
- [x] 2.4 Añadir diagnósticos estables para backend seleccionado,
  inicialización, lectura y fuente de cambios, sin contenido, hashes, rutas
  absolutas ni valores crudos del entorno.
- [x] 2.5 Si el soporte opcional existente de `arboard` no cubre la versión KDE
  objetivo, detener la implementación y actualizar este diseño con la
  dependencia/API alternativa, impacto y opciones descartadas antes de
  continuar.

## 3. Regresiones automatizadas

- [x] 3.1 Añadir pruebas de selección X11/Wayland/fallback usando estado de
  sesión/backend inyectable; no depender de una sesión gráfica real.
- [x] 3.2 Añadir regresión donde un payload nativo Wayland nuevo no sea
  suprimido por una revisión X11 sin cambios.
- [x] 3.3 Probar errores de inicialización/lectura y que el watcher continúa
  ejecutándose con outcomes tipados.
- [x] 3.4 Probar deduplicación y recaptura tras eliminación con las señales
  disponibles, sin inventar revisiones.
- [x] 3.5 Confirmar metadata-only en diagnósticos y preservar blacklist,
  entradas de texto/rich text/imagen, persistencia de assets y X11.
- [x] 3.6 Verificar build Linux empaquetable con sus features reales, así como
  compilación/tests relevantes para macOS; no añadir comandos de sistema como
  requisito de runtime.

## 4. Verificación manual y cierre

- [x] 4.1 En Arch Linux KDE Plasma Wayland, copiar texto desde una aplicación
  nativa Wayland y confirmar una nueva fila en el historial sin abrir
  ClipVault durante la copia. Aprobado por el usuario: las capturas desde
  distintas aplicaciones aparecen en la lista de historial.
- [ ] 4.2 Repetir con una aplicación XWayland y comprobar que no pierde la
  captura existente.
- [ ] 4.3 Verificar texto enriquecido e imagen sólo si son payloads soportados
  por la build validada; conservar los formatos y assets sin regresión.
- [ ] 4.4 Repetir las capturas representativas en Ubuntu GNOME Wayland, Ubuntu
  GNOME X11, Linux X11 y macOS; no marcar plataformas no ejecutadas como
  verificadas.
- [x] 4.5 Ejecutar los tests relevantes, `cargo fmt --all -- --check`, las
  comprobaciones/builds afectadas y `openspec validate
  kde-wayland-clipboard-capture --strict --type change`.
- [x] 4.6 Revisar el diff, errores/warnings, dependencias nuevas, privacidad y
  estado de assets; dejar explícitas las verificaciones manuales pendientes.
