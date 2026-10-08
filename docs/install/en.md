# Install ClipVault

Download ClipVault from the [latest official release](https://github.com/dgadduci/clipvault/releases/latest). The instructions below were prepared against `v0.0.20` (2026-10-07). The release page always shows the current version.

## Choose your download

| Computer | File in release `v0.0.20` | Installation route |
| --- | --- | --- |
| Mac with Apple silicon | `ClipVault_0.0.20_aarch64.dmg` | Disk image (`.dmg`) |
| Mac with Intel processor | `ClipVault_0.0.20_x64.dmg` | Disk image (`.dmg`) |
| Ubuntu GNOME, 64-bit Intel/AMD | `ClipVault_0.0.20_amd64.deb` | Ubuntu software installer |
| Arch Linux with KDE Plasma Wayland, 64-bit Intel/AMD | `ClipVault_0.0.20_amd64.AppImage` | AppImage |

The file names include the release version. If a newer version is listed, choose the file with the same operating system and processor suffix. Linux ARM packages, RPM, Flatpak, Snap, AUR and native Arch packages are not published.

## macOS

1. On your Mac, open **Apple menu → About This Mac**. Choose the `aarch64` file for Apple silicon or the `x64` file for Intel.
2. Open the downloaded `.dmg` file.
3. In the window that opens, drag **ClipVault** to **Applications**. Eject the disk image when copying finishes.
4. Open **Applications** and launch ClipVault. A short startup screen appears while the app initializes, then the main window opens.

### If macOS blocks the first launch

The release is built without Apple notarization, so macOS may show a security warning. First confirm that the download came from the official ClipVault release above. Try opening ClipVault once; if macOS blocks it, go to **System Settings → Privacy & Security → Open Anyway**, then confirm the prompt. Do not turn off Gatekeeper globally. See [the release security notes](../releases.md#macos-builds-without-an-apple-developer-account) for details.

## Ubuntu GNOME

The same `.deb` installation steps apply to Ubuntu GNOME sessions using Wayland and X11. The desktop session can affect runtime integration, but it does not change which package to download.

1. Download `ClipVault_0.0.20_amd64.deb` from the [latest release](https://github.com/dgadduci/clipvault/releases/latest). It is for 64-bit Intel/AMD computers.
2. Open **Files**, then open **Downloads**.
3. Double-click the `.deb` file. Ubuntu opens its software installer. Select **Install** and enter your computer password if requested.
4. Open **Show Applications**, search for **ClipVault**, and launch it.

The published `.deb` was installed and opened successfully in Ubuntu GNOME Wayland and X11. The AppImage is also published, but its Ubuntu installation flow has not been validated, so this guide recommends the `.deb` route.

## Arch Linux with KDE Plasma

The official Linux download for this configuration is the x86_64 AppImage. ClipVault does not publish an AUR or native Arch package.

1. In Dolphin, open your **Home** folder and create a folder named `Applications` if it does not already exist.
2. Download `ClipVault_0.0.20_amd64.AppImage` from the [latest release](https://github.com/dgadduci/clipvault/releases/latest), then move it from **Downloads** to `Home/Applications`.
3. Rename the file to `ClipVault.AppImage`. Keeping it in this location means a panel shortcut can continue to find it.
4. Right-click the file, choose **Properties**, then **Permissions**. Enable **Is executable** and close the properties window.
5. Double-click the AppImage. If KDE asks whether to execute or display the file, choose **Execute**.
6. Once ClipVault is open, right-click its icon on the bottom panel and choose **Pin to Task Manager**.

ClipVault has launched successfully from `Home/Applications` in Arch KDE
Plasma Wayland, and a new panel shortcut was created from that location. The
previous launcher still points to an old location and could not be removed;
use the new shortcut for the current AppImage. The old launcher does not block
the new one. When updating, replace `ClipVault.AppImage` in the same folder to
keep its path. No administrator password or package manager is required.

## First launch and optional desktop integrations

ClipVault shows a short startup screen while it initializes, then opens the main window. You can start using the app without enabling a desktop integration.

On GNOME Wayland or KDE Plasma Wayland, an optional integration can identify the source application for a capture and provide ClipVault's global keyboard shortcuts. To configure it later, open **Settings → Desktop integrations** in ClipVault and follow the status shown there. You can skip or postpone this step; it is not required to install or open ClipVault.

Linux X11 and Wayland are separate session types. A check on one does not establish the behavior of the other. See the verification matrix below before relying on a platform-specific feature.

## Verification status

| Configuration | Manual app check | Clean install steps in this guide |
| --- | --- | --- |
| macOS | Published DMG installed and opened; processor/version not recorded | Passed; processor/version not recorded |
| Ubuntu GNOME Wayland | Published `.deb` installed and opened successfully | Passed |
| Ubuntu GNOME X11 | Published `.deb` installed and opened successfully | Passed |
| Arch KDE Plasma Wayland | Published AppImage launched from `Home/Applications`; new panel shortcut created. Previous launcher still points to an old path | Passed; previous launcher remains |
| Other Linux distributions and desktops | Not established by these checks | Not documented as supported |

## Privacy and help

Clipboard history stays on your computer. To check for updates, ClipVault contacts GitHub over HTTPS and sends the app version, operating system, and processor architecture; it does not send clipboard contents. Read the [release and updater notes](../releases.md).

For help, see [Support](../../SUPPORT.md). Do not include clipboard contents, passwords, tokens, or private keys in a support request.
