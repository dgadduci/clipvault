## MODIFIED Requirements

### Requirement: Linux X11 adapters compile against x11rb 0.13.2

ClipVault SHALL keep the Linux X11 adapters buildable against the version of
`x11rb` pinned by the workspace (`0.13.2`). The adapters SHALL consume the
real types and field names exposed by that version, SHALL be `Send + Sync`
when wrapped in the platform probe and paste traits, and SHALL propagate
the real connection and flush errors instead of swallowing them.

#### Scenario: RustConnection is imported from rust_connection

- **WHEN** `X11ActiveApplication` or `X11PasteController` is compiled with
  feature `linux-x11`
- **THEN** it imports `RustConnection` from `x11rb::rust_connection` (the
  canonical 0.13.x path) and not from the crate root

#### Scenario: setup().roots is the correct screen accessor

- **WHEN** an adapter needs the root window of the active screen
- **THEN** it reads `conn.setup().roots[screen_number].root` and never the
  non-existent `conn.setup().screens` field

#### Scenario: X11 adapters are Send + Sync

- **WHEN** the bootstrap wraps `X11ActiveApplication` in
  `Arc<dyn ActiveApplicationProbe>` or `X11PasteController` in
  `Arc<dyn PasteController>`
- **THEN** the wrapper type-checks because the adapter's internal state is
  held in `Arc<X11State>` and no manual `unsafe impl Send`/`Sync` is used

#### Scenario: synthetic paste propagates X11 errors

- **WHEN** the synthetic paste controller calls `send_fake_key` and the X
  server rejects the request or the socket flush fails
- **THEN** `send_fake_key` returns `Result<(), x11rb::errors::ConnectionError>`
  and the controller surfaces a typed `PasteError::Backend` carrying the
  real error instead of silently discarding it

#### Scenario: WM_CLASS stays the canonical identifier

- **WHEN** the active application probe reads the window class for the
  focused window
- **THEN** it uses `WM_CLASS` as the stable identifier, with `_NET_WM_NAME`
  as a display label only when `WM_CLASS` is missing

#### Scenario: synthetic Ctrl+V flow uses XTEST

- **WHEN** the user triggers a paste on Linux X11 with XTEST available
- **THEN** the controller emits four `xtest::FakeInputRequest` events
  (Ctrl-down, V-down, V-up, Ctrl-up) using the keycodes assigned at
  connection time and then flushes the connection

### Requirement: no behavioural regression on macOS

ClipVault SHALL keep macOS adapters and tests unchanged by the X11
compatibility fix. macOS SHALL continue to route through
`macos_active_app` and `macos_paste`, and the platform-independent tests
SHALL stay green.

#### Scenario: macOS paste and active-app adapters untouched

- **WHEN** ClipVault is built on macOS or `cargo test --workspace` runs
- **THEN** every macOS-specific test passes and no macOS source file is
  modified by this change

### Requirement: platform-neutral handle for the active-app refresher

The Tauri shell carries an `Option<...>` slot on `AppState` so the
installed active-app refresher stays alive for the lifetime of the
application. ClipVault SHALL expose that slot through a
`clipvault_platform::ActiveAppRefresherHandle` alias that resolves to
the macOS timer on macOS builds and to `()` on every other platform,
so the shell never needs to repeat the
`#[cfg(all(target_os = "macos", feature = "macos-native"))]` gate at
every reference site.

#### Scenario: macOS shell wires the real timer through the alias

- **WHEN** the shell starts on macOS with the `macos-native` feature
  enabled
- **THEN** `clipvault_platform::ActiveAppRefresherHandle` is type-equal to
  `clipvault_platform::MainQueueActiveAppRefresher`, the
  `install_active_app_main_queue_refresher` branch returns
  `MainQueueInstallOutcome::Installed` and stores `Some(handle)`, and the
  existing diagnostics surface (`refresher_installed`, callback counter,
  cache refresh) keeps working unchanged.

#### Scenario: Linux / non-macOS shell ignores the macOS refresher

- **WHEN** the shell starts on Linux X11, Wayland, or any other
  non-macOS host
- **THEN** `clipvault_platform::ActiveAppRefresherHandle` resolves to
  `()`, the `install_active_app_main_queue_refresher` branch returns
  `MainQueueInstallOutcome::SkippedUnsupported` and `None`, and the
  shell does not reference `MainQueueActiveAppRefresher` anywhere
  outside a `#[cfg(target_os = "macos")]` gate.