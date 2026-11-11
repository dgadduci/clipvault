# Tasks: macOS local-network sharing onboarding

## 1. Baseline and contract

- [ ] 1.1 Read the current `PeerSharingModal`, `PrivacyModal`, settings bridge,
  local-peer-sharing specs and existing frontend tests; preserve the current
  sharing/runtime and pairing behavior once the display-name prerequisite is
  met.
- [ ] 1.2 Confirm the public GitHub installation guide paths and the exact
  macOS settings names for both English and Spanish documentation.

## 2. Gate the sharing modal

- [ ] 2.1 Load the persisted visible name before loading sharing status or peer
  snapshots; treat missing, blank and whitespace-only values as unconfigured.
- [ ] 2.2 When unconfigured, render only localized instructions to set and save
  the name in **Settings → Privacy → Display name**, then return to the sharing
  modal. Hide the toggle, runtime status, identity retry and peer list.
- [ ] 2.3 When configured, render the existing full sharing form and preserve
  its runtime/error states without automatically enabling discovery.
- [ ] 2.4 On configuration read failure, render a safe retry state rather than
  guessing that the name is present or exposing network controls.
- [ ] 2.5 Add all new UI copy to `en`, `es`, `pt`, `de` and `fr`; preserve key
  and interpolation parity.

## 3. Regression tests

- [ ] 3.1 Test that absent, empty and whitespace-only names show only the
  onboarding instructions and do not render sharing controls or issue toggle
  and snapshot calls.
- [ ] 3.2 Test that a configured name shows the full existing form and that
  opening it does not itself enable sharing.
- [ ] 3.3 Test the safe configuration-loading/error state and retry behavior.
- [ ] 3.4 Run the existing peer-sharing and modal regressions affected by the
  conditional rendering.

## 4. Public GitHub setup guide

- [ ] 4.1 Add a macOS local-network configuration section to
  `docs/install/en.md`: set and save the display name first, return and enable
  sharing, accept the first-use macOS permission prompt, and manage the grant
  under **System Settings → Privacy & Security → Local Network**.
- [ ] 4.2 Add the equivalent section to `docs/install/es.md` using the current
  Spanish macOS labels; explain that installation alone does not populate the
  Local Network permission list.
- [ ] 4.3 Review both guides for matching step order and accurate privacy
  claims; do not imply that clipboard contents are sent merely by discovery.

## 5. Verification

- [ ] 5.1 Run the targeted frontend tests, `npm run check`, `npm run build` and
  `npm run locales:check` from `app/tauri/frontend`.
- [ ] 5.2 Validate this change with strict OpenSpec validation and run
  `git diff --check`.
- [ ] 5.3 Manually verify on macOS with no saved display name: opening the
  sharing option shows only the instructions and does not start discovery.
- [ ] 5.4 Manually save the display name under **Settings → Privacy**, reopen
  sharing, enable it, approve the native Local Network prompt and confirm
  ClipVault is then available in macOS Local Network settings.
- [ ] 5.5 Review the final diff and confirm no data migration, permission
  bypass, clipboard transfer or unrelated changes were introduced.
