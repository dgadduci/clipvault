## Why

On a fresh macOS installation, local-network discovery cannot start until the user has configured a visible device name. Without that prerequisite, the current sharing modal does not explain what is missing, and ClipVault does not appear in macOS's Local Network privacy list. The user needs a clear, guided prerequisite in the app and in the public GitHub setup guide.

## What Changes

- Gate the **Share on local network** modal on whether a non-empty visible device name has been saved.
- When no name is configured, show only instructions to set it under **Settings → Privacy → Display name**; hide the sharing toggle, peer list, and remaining sharing controls.
- When a name is configured, show the existing sharing form and peer content.
- Document the prerequisite and the macOS Local Network permission flow in the public installation/configuration guide in English and Spanish.
- Localize all new product-visible guidance in every supported language.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `local-peer-sharing`: define the display-name prerequisite, gated modal states, and the behavior after the prerequisite is met.

## Impact

- Frontend: `PeerSharingModal.svelte`, its modal state and tests.
- Localization: all five supported catalogs (`en`, `es`, `pt`, `de`, `fr`).
- Public documentation: `docs/install/en.md` and `docs/install/es.md` (the GitHub-hosted setup/configuration guide).
- No changes to the discovery protocol, secure identity format, macOS permission declarations, or network behavior once sharing is enabled.
