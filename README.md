# ClipVault

**Local clipboard history for macOS and Linux.** Search, organize and reuse
copied items with a keyboard-first desktop app.

[English](README.md) · [Español](README.es.md) ·
[Download the latest release](https://github.com/dgadduci/clipvault/releases/latest) ·
[Installation guides](docs/install/README.md)

![ClipVault social preview](docs/assets/github-social-preview.png)

ClipVault keeps clipboard history on your computer. It can capture text, rich
text and images, then help you find and organize items with local search,
favorites, collections and tags. Quick Paste gives you a compact way to reuse
recent items.

## What it includes

- Search clipboard history from the desktop app or Quick Paste.
- Keep important items with favorites; organize items with collections and
  tags.
- Review captured text, rich text and images in the same history.
- Pause clipboard capture when you want a break.
- Use configurable keyboard shortcuts for quick access.

## Screenshots

![ClipVault history with collections and captured images](docs/assets/2e07658ad0628f0b10cdfdb22405978219e92626e90ca0c088d2c9feb8650a18.png)

<details>
<summary>More views of ClipVault</summary>

![Clipboard history search](docs/assets/93eed21417e1fedb9ab01ca84254c07c85278b45ef4185b5008b74ccd59d4077.png)

![New capture dialog](docs/assets/404511162e0898999cb0c349e00f93c58b89cb5492ba65f443a2b54d6302b108.png)

![Collections and linked devices](docs/assets/d470ca8e75933e5c30f87d4129c9c7a3769683b03752d79afe5d452c92c79ebb.png)

</details>

## Downloads and platforms

Official releases provide macOS disk images for Apple silicon and Intel, plus
Linux x86_64 DEB and AppImage packages. The installation guide lists the
tested and pending desktop/session combinations; a check on one Linux session
does not establish support for another.

- [Download the latest release](https://github.com/dgadduci/clipvault/releases/latest)
- [Choose an installer and read first-run steps](docs/install/README.md)
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
