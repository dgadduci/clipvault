# Desktop releases and updater keys

The desktop release workflow builds macOS Apple Silicon and Intel installers
and Linux x86_64 AppImage and Debian packages. It creates a **draft** GitHub
Release for a `vMAJOR.MINOR.PATCH` tag. Draft releases are not offered to
installed clients; publish the draft only after reviewing its installers,
signatures and `latest.json` manifest.

The client checks GitHub Releases over HTTPS. The request contains the app
version, operating system and architecture needed to select an update. It does
not include clipboard history, capture titles, local paths or local entry IDs.
The user must choose **Download and install**; after installation, a separate
action relaunches ClipVault. Linux `.deb` updates use the operating system's
authorization flow, while AppImage updates replace the AppImage directly.

## Tauri updater key

Generate one key pair and keep the private key in a password manager or other
restricted backup. Do not commit it, paste it into a pull request, or put it in
build logs. The release workflow reads the private key from a GitHub Actions
secret; the public key is supplied as a GitHub Actions variable and compiled
into release builds. CI creates a temporary Tauri config overlay so the CLI
uses the same public key to sign updater artifacts and embed it in the app.
Local development builds omit the updater plugin and do not query GitHub.

From `app/tauri`, create a private key file outside the repository:

```sh
mkdir -p "$HOME/.tauri"
cargo tauri signer generate --write-keys "$HOME/.tauri/clipvault-updater.key"
chmod 600 "$HOME/.tauri/clipvault-updater.key"
```

Save the public key printed by the signer as the repository variable
`TAURI_UPDATER_PUBLIC_KEY`. Store the private key as the repository secret
without placing its value in the command line:

```sh
gh variable set TAURI_UPDATER_PUBLIC_KEY -R dgadduci/clipvault --body "<public-key>"
gh secret set TAURI_SIGNING_PRIVATE_KEY -R dgadduci/clipvault < "$HOME/.tauri/clipvault-updater.key"
```

If the private key was generated with a password, also add that password as
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Keep a secure backup of the private key:
losing it prevents future releases from signing updates trusted by already
installed clients. Rotating the key requires a separate migration plan.

## macOS signing and notarization

Distribution outside the Mac App Store requires an Apple Developer ID
certificate and notarization credentials. Add these repository secrets before
tagging a release:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE` | Base64 encoded Developer ID `.p12` certificate |
| `APPLE_CERTIFICATE_PASSWORD` | Password used to export the `.p12` |
| `APPLE_SIGNING_IDENTITY` | Developer ID Application identity name |
| `APPLE_ID` | Apple ID used for notarization |
| `APPLE_PASSWORD` | App-specific Apple ID password |
| `APPLE_TEAM_ID` | Apple Developer team ID |

Never commit the certificate, its password, the Apple password or any private
key. The validation job checks that the updater and Apple signing settings are
present without printing their values; it stops before building if they are
missing.

## First installation and draft review

1. Bump the Cargo workspace, Tauri and frontend versions together.
2. Push a `vMAJOR.MINOR.PATCH` tag whose commit is reachable from `main`.
3. Review the draft's macOS `.dmg` and `.app.tar.gz` artifacts, Linux
   `.AppImage` and `.deb` artifacts, signatures and `latest.json`.
4. Test updates from each supported installer type, including accepting and
   cancelling the Linux `.deb` authorization prompt.
5. Publish the draft only after those checks pass. The first client install
   must use a release build that already contains the updater public key.

Never publish a draft whose manifest points an installer type at another
bundle's artifact. The updater selects Linux assets using both architecture and
installer type (`appimage` or `deb`).
