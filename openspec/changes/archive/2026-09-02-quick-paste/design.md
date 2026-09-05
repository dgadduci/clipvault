# Design: quick-paste

## Context

El spec `openspec/specs/quick-paste/spec.md` exige una ventana
transient, un hotkey global por plataforma y un pegado que regrese el
foco a la aplicación activa. Hoy el shell Tauri ya emite
`clipvault://quick-search` cuando el adaptador `global-hotkey` recibe
la pulsación, pero la ventana receptora no existe. La ventana
principal renderiza resultados y cuenta con un botón "Paste latest"
que sólo funciona sobre la primera entrada, lo que rompe la promesa
"atajo → escribir → Enter → pegar".

El adapter `global-hotkey` también pierde la promesa de la spec:
`HotkeyModifiers::CMD_SHIFT` y `HotkeyModifiers::CTRL_SHIFT` se mapean
siempre a `GhModifiers::CONTROL`, así que en macOS el usuario termina
presionando `Ctrl+Shift+V` y no `Cmd+Shift+V`.

Por último, `frontend/src/lib/search.ts` (helper `runSearch`) traga los
errores del backend con `.catch(() => {})`. Eso evita unhandled
rejection pero impide que el caller distinga "búsqueda cancelada" de
"error del backend" y deja la promesa sin valor terminal cuando hay
fallos.

## Goals / Non-Goals

**Goals:**

- Ventana Tauri dedicada `quick-paste` que no existe todavía.
- Hotkey global respeta la plataforma (SUPER en macOS, CONTROL en
  Linux) sin cambiar los defaults.
- Listener único e idempotente del evento del hotkey en el frontend
  principal, con captura del active app previa a mostrar la ventana.
- `QuickPaste.svelte` con búsqueda, selección circular, Enter / Escape
  y manejo correcto de pegado exitoso y fallido.
- Reutilizar `PlatformGuidanceModal` y el comando `pasteEntryCommand`
  existente. Sin nueva API de foco en el core; ocultar la ventana es
  el mecanismo actual para devolver el foco al target previo.
- `runSearch` robusto: cancelaciones resuelven o terminan limpias,
  errores del backend no quedan como promesas pendientes y
  resultados obsoletos no sobrescriben a los nuevos.

**Non-Goals:**

- Favoritos, snippets, transformaciones, retención, borrado.
- Red, telemetría, embeddings, cloud, cuentas.
- Cambios en la API del core o en el contrato de `PasteService`.
- API nueva para mover foco del sistema operativo (la spec
  explícitamente lo prohíbe).

## Decisions

### Ventana Tauri dedicada

`app/tauri/tauri.conf.json` declara la ventana principal y, ahora,
una segunda ventana `quick-paste`:

```json
{
  "label": "quick-paste",
  "title": "ClipVault Quick Paste",
  "url": "quick-paste.html",
  "width": 640,
  "height": 420,
  "minWidth": 480,
  "minHeight": 280,
  "resizable": false,
  "decorations": false,
  "alwaysOnTop": true,
  "skipTaskbar": true,
  "visible": false
}
```

- `visible: false` para que la app arranque con la ventana oculta.
- `skipTaskbar` para evitar ensuciar el dock de macOS o la barra de
  tareas de Linux.
- `alwaysOnTop: true` para que la ventana quede encima de la app
  activa; la spec lo requiere para que el pegado se vea natural.
- `decorations: false` para conservar el estilo compacto.
- `resizable: false` y dimensiones fijas para evitar cambios de
  layout al pegar.

`app/tauri/src-tauri/capabilities/default.json` agrega
`"quick-paste"` a la lista de ventanas. Esto es lo que permite al
frontend de la segunda ventana ejecutar comandos Tauri sin abrir
permisos adicionales.

La segunda ventana carga `app/tauri/frontend/quick-paste.html`, un
HTML mínimo que monta `QuickPaste.svelte`. La ventana principal
sigue montando `App.svelte`.

### Listener único e idempotente

`App.svelte` registra el listener `clipvault://quick-search` dentro de
`onMount` usando un módulo helper (`lib/quickPasteBridge.ts`) que
mantiene una bandera estática: si la función ya se llamó, devuelve el
`unlisten` existente. Esto evita que múltiples mounts (HMR, hot
reload, doble inicialización) terminen disparando el mismo flujo más
de una vez. La única acción del listener es invocar una callback
`onQuickSearch` que el componente proporciona; el módulo no conoce
Tauri directamente para mantenerlo testeable.

### Captura de active app antes de mostrar

Antes de `WebviewWindow::show` y `set_focus` se ejecuta
`activeApplicationCommand`. Esto preserva la pista del target previo
sin acoplarse al sistema: el valor se consume localmente y nunca se
serializa en un evento. Si la probe devuelve `available: false`, el
flujo sigue (pegado podría no funcionar, pero `PlatformGuidanceModal`
lo explicará al fallar).

### Mostrar y enfocar

Se llama a `WebviewWindow::show()` y `WebviewWindow::set_focus()` en
ese orden. Luego se emite `clipvault://quick-paste-opened` dirigido a
la ventana `quick-paste` con payload `null` (sólo señal). El evento
no incluye query, snippet ni contenido.

### Pegado con ocultamiento previo

`QuickPaste.svelte`:

1. Al confirmar (Enter), toma el `entry_id` seleccionado.
2. Llama a `hideQuickPasteWindow()` (IPC) **antes** de
   `pasteEntryCommand`.
3. Si la respuesta es `pasted`: la ventana queda oculta. Se cierra el
   flujo.
4. Si la respuesta es `failed` o `capability_unavailable`: re-mostrar
   la ventana `quick-paste` y abrir `PlatformGuidanceModal` con la
   `guidance` recibida (puede ser `null`, en cuyo caso el modal
   muestra el fallback manual o el botón de cerrar).
5. Si la promesa rechaza (error de IPC): mismo manejo que `failed`.

Esto preserva el historial porque `pasteEntryCommand` no muta la
entrada (contrato existente en `clipvault-core/src/paste.rs`).

### runSearch revisado

```ts
function runSearch({ query, invoke, debounceMs }) {
  let token = 0;
  let cancelled = false;
  let timer = null;
  return {
    result: new Promise((resolve, reject) => {
      const fire = () => {
        const my = ++token;
        invoke(query)
          .then((resp) => {
            if (cancelled || my !== token) return; // result obsoleto
            resolve(resp);
          })
          .catch((err) => {
            if (cancelled || my !== token) return;
            reject(err); // ahora rechaza limpiamente
          });
      };
      if (debounceMs > 0) timer = setTimer(fire, debounceMs);
      else fire();
    }),
    cancel: () => {
      cancelled = true;
      token = Number.MAX_SAFE_INTEGER;
      if (timer) clearTimer(timer);
      timer = null;
    },
  };
}
```

- Cancelaciones terminan sin resolver ni rechazar el promise de la
  controller cancelada (queda pending), pero ningún listener lo
  consume, así que no se filtra como unhandled rejection.
- Errores del backend rechazan el promise del controller "más
  reciente" en el momento del fallo. Si llega otro invoke después, el
  token nuevo crea un nuevo controller y no se confunde con el
  viejo.
- Resultados viejos se descartan comparando `my === token`.

### Hotkey por plataforma

`hotkey_global.rs::map_modifiers` cambia de:

```rust
if modifiers.cmd_or_ctrl { out |= GhModifiers::CONTROL; }
```

a:

```rust
if modifiers.cmd_or_ctrl {
    #[cfg(target_os = "macos")]
    { out |= GhModifiers::SUPER; }
    #[cfg(not(target_os = "macos"))]
    { out |= GhModifiers::CONTROL; }
}
```

- `default_macos_binding` y `default_linux_binding` siguen iguales.
- Los tests del módulo validan el mapeo para macOS y Linux.

### Privacidad y logs

- Ningún evento lleva query, snippet o contenido del clipboard.
- Los logs en backend siguen siendo `tracing` sin payloads (no
  introducimos nuevos campos).
- `pasteEntryCommand` ya excluye el contenido del portapapeles de la
  respuesta (`PasteResponse.id` y `kind`); reutilizamos el comando
  existente tal cual.

### Tests

Frontend (TS, `node:test`):

- `runSearch` resuelve el resultado del último invoke y descarta los
  anteriores; rechaza con el error del backend.
- `quickPasteBridge` registra exactamente un listener aunque se
  llame varias veces.
- `quickPasteController` (helper puro) ejecuta el orden
  `target → show → focus → emit` y respeta los flags.
- `quickPastePasteFlow` (helper puro) recibe pasted y mantiene la
  ventana oculta; recibe failed o capability_unavailable y la re-
  muestra con la modal de guidance.

Backend (Rust, `cargo test`):

- `hotkey::map_modifiers` para macOS y Linux.
- `hotkey_global.rs::GlobalHotkeyManagerAdapter::map_modifiers` con
  `cfg(target_os = "macos")` (compilación condicional).
- Tests existentes de `PasteService` siguen pasando: el cambio no
  altera su contrato.

## Risks / Trade-offs

- **Mostrar la ventana antes del paste puede provocar un "flash"
  visual en hosts lentos.** → Aceptable: la ventana es compacta y la
  spec ya exige "transient". El flujo "hide → paste → (re-show on
  error)" mantiene el pegado funcional con un flash mínimo.
- **`runSearch` ahora rechaza errores.** Los componentes que ya
  esperaban "siempre resuelve" deben añadir `try/catch` o
  `.catch()`. → Aplicado en `App.svelte` y `QuickPaste.svelte` desde
  el primer commit.
- **Listener único implica un módulo estático.** Si HMR re-monta el
  módulo, el bridge debe limpiar el listener en `destroy`. → El
  registrar idempotente maneja eso: si ya hay listener, devuelve el
  mismo unlisten; sólo se registra una vez.
- **Re-mostrar la ventana tras error desplaza el foco del target.**
  → La spec lo acepta ("reports the failure without deleting"). El
  usuario debe presionar Escape para reintentar el pegado.

## Migration Plan

No aplica: cambio aditivo. La ventana principal sigue funcionando
igual y la spec `desktop-foundation` no se ve afectada. No hay
migración de datos ni release que coordinar.

## Open Questions

- ¿La ventana `quick-paste` debe permitir pegado por teclado
  adicional (Ctrl+V nativo dentro del input)? No es requisito del
  MVP. El input usa `type="search"` y se controla con atajos de la
  spec.
- ¿Mostrar un toast cuando se re-muestra la ventana tras un error?
  El modal ya cubre el caso; no se agrega otra capa.
