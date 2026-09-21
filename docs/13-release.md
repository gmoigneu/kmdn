# Releasing kmdn

Decision D34: GitHub Releases, Tauri updater, signed and notarized DMG, AppImage and .deb. This is how far the pipeline goes today and what the owner still has to provide.

## What happens on a tag

Push a tag `vX.Y.Z` (or run the `release` workflow by hand) and `.github/workflows/release.yml` builds:

| Job | Runner | Output |
|---|---|---|
| cli | ubuntu-22.04, macos-14, macos-13 | `kmdn-cli-<target>.tar.gz` plus a `.sha256` for x86_64 Linux, arm64 macOS, x86_64 macOS |
| desktop | ubuntu-22.04 | `.deb` and `.AppImage` |
| desktop | macos-14, macos-13 | `.dmg` for arm64 and x86_64 |

Everything is attached to a **draft** release named after the tag. The owner reviews the draft, writes the notes, and publishes. Nothing is public until then.

The KB CI templates in `templates/ci/` download `kmdn-cli-x86_64-unknown-linux-gnu.tar.gz` from the latest release, so the first published release also turns on the optional CI check for knowledge bases.

## Version

`version` in the workspace `Cargo.toml` and `apps/desktop/src-tauri/tauri.conf.json` must match the tag without the `v`. Bump both in the release commit.

## Secrets the owner must add (see issue 48)

| Secret or variable | Effect when present |
|---|---|
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY` | macOS builds are code-signed |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | and notarized, so Gatekeeper opens them without a right-click |
| variable `KMDN_GITHUB_CLIENT_ID` | the GitHub device flow is compiled in (D30) |
| `KMDN_E2E_GITHUB_TOKEN` | the nightly golden path runs |

Without the Apple secrets the DMGs are unsigned. They still run after right-click, Open.

## Not done yet

- **Updater.** The Tauri updater needs a signing key pair (`pnpm tauri signer generate`) with the public key in `tauri.conf.json` and the private key in `TAURI_SIGNING_PRIVATE_KEY`. Add the plugin and the `updater` endpoints once the key exists; until then users download new versions from the release page.
- **Homebrew cask and Flatpak.** Explicitly later per D34.
- **Windows.** Not in v1 (D23).
