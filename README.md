# ClipVault

**Local clipboard history for macOS and Linux.** Search, organize and reuse
copied items with a keyboard-first desktop app.

[English](README.md) · [Español](README.es.md) ·
[Download the latest release](https://github.com/dgadduci/clipvault/releases/latest) ·
[Installation guides](docs/install/README.md)

![ClipVault social preview](docs/assets/github-social-preview.png)

ClipVault keeps a local history of clipboard text, rich text and images, and
also lets you create text captures manually. Use the full history window to
edit and organize captures, or open Quick Paste for a compact, keyboard-first
way to find a recent item. Paired ClipVault devices can also browse and import
selected captures over the local network.

## What it includes

- Automatically capture clipboard text, rich text and images; see the source
  app when it is available.
- Create text captures directly in ClipVault, edit their text and titles, and
  add notes to keep context with each capture.
- Search the local history and filter by collection, tag, content type or
  source app.
- Organize captures in collections, assign tags, mark favorites and pin
  important items.
- **Quick Paste:** open its compact window with a keyboard shortcut, find a
  recent capture and copy it to paste into the app you were already using,
  without opening the full history window.
- **Local network sharing:** enable sharing and pair ClipVault devices on the
  same network to browse a trusted device's recent text and image captures.
  Import only the items you choose; imports are saved locally and history is
  not synchronized automatically.
- Pause capture, exclude selected apps and choose how long to retain history.
- Configure keyboard shortcuts for the main window and Quick Paste.

## Screenshots

![ClipVault history with collections and captured images](docs/assets/2e07658ad0628f0b10cdfdb22405978219e92626e90ca0c088d2c9feb8650a18.png)

<details>
<summary>More views of ClipVault</summary>

![Clipboard history search](docs/assets/93eed21417e1fedb9ab01ca84254c07c85278b45ef4185b5008b74ccd59d4077.png)

![New capture dialog](docs/assets/404511162e0898999cb0c349e00f93c58b89cb5492ba65f443a2b54d6302b108.png)

![Collections and linked devices](docs/assets/d470ca8e75933e5c30f87d4129c9c7a3769683b03752d79afe5d452c92c79ebb.png)

</details>

## Downloads and platforms

The current `v0.0.20` release provides macOS disk images for Apple silicon and
Intel, plus Linux x86_64 DEB, AppImage and RPM packages. The RPM is a direct
download and is not yet in the signed updater manifest. Container package
checks passed on Fedora 44 and an openSUSE library check reported no missing
libraries; desktop compatibility remains unverified. The installation guide
lists tested and pending desktop/session combinations; a check on one Linux
session does not establish support for another.

On macOS, ClipVault is also available from the official
[Homebrew tap](https://github.com/dgadduci/homebrew-tap). It installs the same
official DMG for the Mac architecture, with a reviewed SHA-256. Homebrew does
not remove Gatekeeper checks; a first launch can still require manual approval.

- [Download the latest release](https://github.com/dgadduci/clipvault/releases/latest)
- [Choose an installer and read first-run steps](docs/install/README.md)
- [Install with Homebrew](https://github.com/dgadduci/homebrew-tap)
- [See release and updater details](docs/releases.md)

## Privacy

Clipboard history is stored locally; normal use does not require a cloud
account. To check for updates, ClipVault contacts GitHub over HTTPS with the
app version, operating system and processor architecture. It does not send
clipboard contents. See the [release and updater notes](docs/releases.md) for
details.

## Help and contributing

- [Support and issue reporting](SUPPORT.md)
- [Contributing](CONTRIBUTING.md)
- [Security reporting guidance](SECURITY.md)
- [GNU GPL version 3 license](LICENSE)

ClipVault is built with Rust, Tauri and Svelte. See the [development guide](docs/development.md) for contributor toolchains and checks.
