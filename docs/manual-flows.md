# Manual flows

These flows exercise behaviour that automated tests cannot cover reliably
end-to-end (real clipboard backends, real hotkey bindings, OS settings panes).
Each flow is short, reproducible and asserts the outcome that the matching
OpenSpec capability promises.

All flows assume ClipVault is running locally on macOS or Linux (X11). The
test data is created in the real `~/.clipvault/clipvault.db`; back it up or
delete the file before repeating.

## Add a blacklisted application and verify it is skipped (privacy-settings)

Goal: prove `clipvault_ignored_apps_add` plus the `PrivacyGate` keep the
captured text out of the local history.

> **Boot output on a real macOS checkout.** The `info!` line on stdout
> must say `macOS active-app cache will refresh on the main queue every
> 1s`. The older `macOS main-queue refresher not active; falling back
> to on-demand refresh` message MUST NOT appear — its presence means
> the shell was compiled with the `macos-native` stub active and the
> real `dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, …)` timer
> never installed. The diagnostics snapshot must report
> `refresher_installed=true` and `timer_callback_count` greater than
> zero after a five-second warm-up.

1. Open the ClipVault main window. If the Privacidad card is not visible,
   restart the application — the panel re-fetches on mount.
2. In the **Privacidad** section, type `com.apple.TextEdit` (macOS) or
   `gedit` (Linux GNOME) in the **Identificador** input and click
   **Añadir**. The identifiers panel shows the new entry below the form;
   the status line confirms “Aplicación añadida a la lista ignorada.”
3. **Click somewhere else first** so the text input loses focus and
   ClipVault stops being the frontmost app. (Clicking **Añadir**
   itself leaves ClipVault focused, which would make the cached
   probe report `com.clipvault.app` instead of the blacklisted
   app.) Then open the blacklisted application (TextEdit / gedit)
   and copy a distinctive string, e.g. `cv-blacklist-marker-42`.
   **Leave the source application focused** when the capture
   happens — the background capture loop polls the clipboard every
   ~750 ms and on macOS the cached active-app probe is refreshed
   every second by the dispatch-timer on the Cocoa main queue, so
   the matcher sees the real source identifier while the
   originating app is still in the foreground.
4. Wait for at least one capture tick (one second is enough — the
   timer fires once per second on macOS; the loop polls every
   ~750 ms).
5. Open the **Local search** input and search for `cv-blacklist-marker-42`.
   The query must report **0 matches**: the captured text was filtered
   out by `PrivacyGate` before reaching `EntryRepository::insert_or_touch`.
6. Open the **Privacidad** card and inspect **Diagnóstico de la
   caché**. `Estado` must report the blacklisted identifier, and
   `Última decisión` must read `discarded:blacklisted`.
7. Optional confirmation: from the SQLite shell run
   `SELECT COUNT(*) FROM clipboard_entries WHERE content LIKE '%cv-blacklist-marker-42%';`
   — the result is `0`.

> **Why the manual flow does not use Tick capture.** Tick capture
> changes the focused application to ClipVault before the watcher
> reads the source identifier, so the cached probe reports ClipVault
> (or `None` on Wayland) instead of the blacklisted app. The shared
> `CaptureWatcher` keeps a single `last_hash` between the background
> loop and Tick capture, so a Tick capture that runs *after* the loop
> has already discarded the payload simply returns `unchanged`; that
> is the regression pin covered by the regression suite, not a
> user-facing proof of the blacklist. To prove the blacklist from
> the UI keep the source app focused and let the background loop
> handle the capture.
>
> **Why we no longer ask the operator to click "Refrescar
> diagnóstico" after the copy.** The dispatch-timer fires every
> second on the main queue regardless of UI activity, so the cache
> is fresh without a manual refresh. Pressing **Refrescar
> diagnóstico** would flip the focus back to ClipVault and make the
> cached probe report `com.clipvault.app` — that is exactly the
> false positive the original manual flow documented as a "we expect
> TextEdit here" expectation. The button is reserved for debugging
> scenarios where the timer is genuinely stuck.

Negative control:

8. Remove the identifier via the **Eliminar** button next to it.
9. With the source app focused, copy `cv-blacklist-marker-43` from it.
10. Search for `cv-blacklist-marker-43`. The entry now appears in the
    history.

The mismatch between step 5 and step 10 confirms the blacklist gates writes,
not reads.

## Diagnose a slow or empty active-app cache (privacy-settings)

Goal: confirm `clipvault_active_app_diagnostics` reports the
**real identifier** the matcher observed on the most recent capture
tick, plus the outcome of the most recent refresh attempt, so a user
can attribute a "blacklisted content still leaked" report to either a
stale cache, a failed refresh or a wrong identifier.

1. Focus a non-blacklisted application (for example Safari) and copy
   any text so the loop has a recent snapshot in the cache.
2. In the **Privacidad** card, expand the **Diagnóstico de la caché**
   section. The card exposes two distinct buttons:
   - **Refrescar diagnóstico** invokes the dedicated
     `clipvault_refresh_active_app_diagnostics` Tauri command,
     which schedules a synchronous main-thread refresh, records
     the outcome on the diagnostics state and returns the resulting
     snapshot. Use this button to force an on-demand refresh from
     the UI; the helper detects that the call already runs on the
     main thread (the typical Tauri-command body) and bypasses the
     synchronous channel so the UI never blocks waiting on itself.
   - **Consultar diagnóstico** invokes the read-only
     `clipvault_active_app_diagnostics` command. Use it when you
     only want to re-read the latest snapshot without forcing a
     refresh.
3. The card reports the same fields regardless of which button you
   clicked:
   - `Adaptador`: the active-app backend kind (`macOS NSWorkspace`
     on macOS, `X11 EWMH` on X11, `no disponible` on Wayland).
   - `Estado`: the identifier the cached probe returned on the most
     recent refresh, with the display name in parentheses.
   - `Refresco`: `Último refresco correcto.` (or the sanitised
     failure reason if the refresh failed). The dashboard never
     reports `pending` while the shell has actually tried and
     failed.
   - `Causa`: the structured failure taxonomy the
     `ActiveAppFailureKind` enum exposes. When `Refresco` is
     `failed`, `Causa` reports one of four snake_case values that
     match the canonical copy the operator needs to read:
     - `schedule` — Tauri refused to enqueue the closure (for
       example after the event loop shuts down).
     - `timeout` — the closure was enqueued but the main thread did
       not pick it up inside the synchronous timeout (the old
       `Intentos: 214, Correctos: 0, Fallidos: 214` regression
       would surface here on Tauri 2.x where `run_on_main_thread`
       from a background thread can stall).
     - `unavailable` — Apple's `NSWorkspace` (or the Linux/X11
       adapter) cannot run on this session because the closure
       reached a non-main executor.
     - `backend` — the platform probe returned a typed error
       (X11 disconnect, NSWorkspace exception, …).
- `Bucle`: `El bucle está en marcha.` once the background
      capture loop has spawned; `El bucle todavía no ha arrancado.`
      otherwise.
    - `Refresher instalado`: `Sí` once the shell holds a live
      `MainQueueActiveAppRefresher` handle on `AppState`. On Linux
      and Wayland this stays `No` because the platform only ships a
      periodic refresher on macOS. The flag is sticky: if the timer
      ever installed it does not revert to `false`.
    - `Callbacks`: the number of times the dispatch main-queue
      timer fired its callback. Increments once per fired callback
      regardless of probe success so it is observable when
      `Contadores.Correctos` stays at `0` and `Fallidos` is
      increasing.
    - `Contadores`: `Intentos: N · Correctos: M · Fallidos: K`. The
      counters increment by exactly one per refresh attempt so a
      flaky adapter is visible at a glance.
   - `Última decisión`: the metadata-only label the loop recorded
     on the most recent capture tick (`allowed:stored`,
     `allowed:duplicate`, `discarded:blacklisted`,
     `ignored:empty_clipboard`, `failed:backend`, `failed:watcher`
     or `unchanged`).
   - `Último intento`: the epoch-millisecond timestamp of the most
     recent refresh attempt.
   - `Lista ignorada`: whether the captured identifier matches an
     entry in the persisted blacklist (`true` / `false`).
4. The card must NEVER print:
   - the clipboard content itself,
   - the content hash,
   - a source identifier that was used in a past capture.
   - the captured payload in `Última decisión`.

5. Reproduce the original bug condition: focus a blacklisted
   application (the one you added via step 2 of the previous flow),
   wait one capture tick, then inspect **Diagnóstico de la caché**
   directly — the dispatch-timer refresh keeps the cache fresh
   without operator intervention. The `Estado` must show the
   blacklisted identifier, `Lista ignorada` must report `true`,
   `Contadores` must have incremented and `Última decisión` must
   read `discarded:blacklisted`.

   If the snapshot stays empty (`Caché vacía: el bucle todavía
   no ha visto una aplicación activa`) and `Bucle` reports `El
   bucle está en marcha.`, the dispatch-timer install failed.
   Inspect the `Causa` field: a `backend` failure usually means the
   `dispatch_source_create` call returned null on a constrained
   environment. Re-run
   `cargo test -p clipvault-platform --features macos-native macos_main_queue`
   to confirm the timer still installs in a unit test.

## Retention pass (privacy-settings + clipboard-management)

Goal: confirm `clipvault_apply_retention` honours the configured
`RetentionPolicy` while keeping favourites.

1. In the **Privacidad** section, choose **7 días** under **Retención del
   historial**. The status line confirms the new policy.
2. Restart ClipVault (the retention sweep also runs at boot).
3. From a terminal create two clipboard entries with timestamps older than
   the horizon:
   ```bash
   sqlite3 ~/.clipvault/clipvault.db \
       "INSERT INTO clipboard_entries
        (content, content_type, content_size, content_hash,
         source_app, created_at, updated_at, last_seen_at)
        VALUES
        ('older-pinned', 'text', 12, 'cpinned01',
         NULL,
         datetime('now','-30 days'), datetime('now','-30 days'), datetime('now','-30 days')),
        ('older-normal', 'text', 13, 'cnopinn01',
         NULL,
         datetime('now','-30 days'), datetime('now','-30 days'), datetime('now','-30 days'));"
   ```
4. Open ClipVault, pin **older-pinned** via the **Pin** button.
5. Click **Apply retention now** in the **History management** card. The
   confirmation line reports `removed: 1`.
6. **older-normal** is gone from the **Local search** list and from
   `clipboard_entries`; **older-pinned** is still listed and pinned.

## Diagnostics scrub sensitive values (privacy-settings)

Goal: confirm `RedactingMakeWriter` strips secrets before they hit the
log stream.

1. Open two terminals. In the first run ClipVault so its logs go to stderr
   (`RUST_LOG=info cargo run -p clipvault-app`).
2. From a real terminal copy a value containing a high-entropy string,
   for example the output of `head -c 24 /dev/urandom | base64`.
3. Tick capture from the second terminal.
4. Back in the first terminal inspect the most recent log line. Values
   tagged `password=`, `token=`, JWT-shaped strings (`eyJ…`) and AWS key
   prefixes (`AKIA…`) are replaced by `[REDACTED:secret]`. The clipboard
   **content** itself is never present in the log line — only the
   error category / counter is. The
   `clipvault_active_app_diagnostics` JSON also satisfies this
   contract: it surfaces the cached identifier (already known to the
   platform layer) but never carries content the user copied.

If the unredacted value ever appears in the log stream, the redactor is
broken. Re-run `cargo test -p clipvault-core redact` to confirm the
deterministic redactor still works.
