# Native macOS desktop updates

The native window shows the installed marketing version and build. It checks
GitHub for the latest stable release at launch and when requested. An eligible
release includes **Install update and restart**, **Later**, and release notes
as plain text. Acceptance applies to the specific offered asset. Downloads can
be cancelled until activation starts. A failed or cancelled attempt requires
another acceptance. Save any unsaved cube input before restarting.

The server checks for a disconnected, idle robot and an empty solve queue both
before download and immediately before activation. The final check and
replacement exclude new robot, solve, and settings mutations. The updater
leaves settings, calibration, and solver tables in their existing data
directories. A bundle without a configured published channel shows an in-app
message and stays on its current build.

## Install the CLI and native app

The manually dispatched GitHub Actions release build uploads CLI archives as
workflow artifacts. On macOS the tarball contains the `anycube` executable.
On a Mac, extract the archive for your architecture from a build that includes
the native launcher, or build this checkout with `cargo build --release --locked`.
Run the extracted executable as `./anycube app --install` (for a local build,
`target/release/anycube app --install`), then open `~/Applications/anycube.app`.

Maintainers can also distribute the CLI and assembled app together. On the
build Mac, run the following after installing the matching release build:

```sh
scripts/package-macos-archive.sh target/release/anycube "$HOME/Applications/anycube.app" arm64 dist
```

Use `x86_64` for Intel. The helper checks the plist, bundle signature and
binary architectures, then writes `dist/anycube-macos-<arch>-with-app.tar.gz`.
Recipients can extract it and copy `anycube.app` into `~/Applications`.

A signed in-app update becomes available only when a maintainer signs,
notarizes, and publishes a compatible updater asset as described under
Release inputs. [Manual archive helper](../scripts/package-macos-archive.sh)

## Native launcher integration

- Use a self-contained, writable `anycube.app` with bundle ID
  `local.anycube.app`, the native launcher at `Contents/MacOS/anycube`, and
  the Rust executable at `Contents/MacOS/anycube-server`. The existing
  generated launcher pointing at an external CLI binary cannot bootstrap this
  in-place update. Users need an initial compatible bundle.
- The native launcher starts its embedded backend with
  `app --no-window --with-parent --desktop-session` on a private port. Load
  the full first stdout line in the native web view, including its fragment.
  Keep the line out of logs and browser launchers. The embedded page removes
  the fragment and uses its per-process capability with exact-origin JSON
  POSTs. A CLI, browser or headless server started without the private
  launcher flag has no update endpoint.
- Keep the native window and backend lifetimes paired. When the backend exits
  after update, close the old window and terminate the old launcher. The
  backend has already requested a fresh instance of the same bundle with
  Launch Services using `/usr/bin/open -n`. Allow this short handoff in any
  single-instance logic. A native window closing should release its parent
  lifecycle and allow the backend to finish any activation already in progress.
  Present startup failures in the native window.
- A failed atomic exchange leaves the installed bundle in place. An immediate
  Launch Services failure attempts to restore the old bundle. An interrupted
  activation can leave a backup in `.anycube-update/attempt/anycube.app`
  beside the app. Inspect it and the installed bundle before deleting the
  attempt directory or restoring the backup. Interrupted downloads are
  discarded on the next accepted attempt.

## Release inputs

1. For each supported architecture, build the backend with
   `ANYCUBE_UPDATE_CHANNEL`, `ANYCUBE_UPDATE_BUILD`, and `ANYCUBE_UPDATE_TEAM`
   in the **build environment**. The Team ID is the owner's Apple Developer
   ID Application signing team. Leave unpublished or locally customized
   builds unconfigured. Allocate a strictly increasing integer build number
   within each channel. A channel promises the same robot and camera
   capabilities and data compatibility across its updates. Preserve unpublished
   features by keeping those builds unconfigured or in a separate release
   channel. The release tag matches the marketing version, which may stay the
   same across successively numbered compatible builds.
2. Write `Contents/Resources/desktop-update.json` before signing. For example,
   `{"platform":"macos","architecture":"arm64","channel":"robots-v1","build":42,"version":"0.2.0"}`.
   Use a thin arm64 or x86_64 launcher and backend. Sign the backend and app
   with the configured Developer ID team, then notarize and staple the app.
   [Apple notarization workflow](https://developer.apple.com/documentation/Security/customizing-the-notarization-workflow)
3. Run `python3 scripts/package-desktop-update.py path/to/anycube.app dist`.
   The package has one app bundle in gzip-compressed POSIX ustar. Keep the
   bundle tree to files and directories, with the launcher and backend as the
   two executables. Validate signature and stapling again after unpacking the
   package on a Mac. The existing CLI archives retain their current format.
4. Attach the resulting
   `anycube-desktop-macos-<arch>-<channel>-build-<number>.tar.gz` to the
   matching stable GitHub release in `Horia73/anycube`, with a GitHub
   SHA-256 asset digest. Include every supported architecture/channel on the
   latest stable release. The updater uses the repository metadata for the
   offer and digest and verifies the staged bundle against its pinned Apple
   signing team, identifier, architecture, and release manifest before
   replacing the app.
   [GitHub release API](https://docs.github.com/en/rest/releases/releases)

The release integrator owns the native packaging workflow and uploading
assets. Before enabling the channel, verify on each supported Mac architecture
that the native view can authenticate, display notes, cancel, retry, and
relaunch the same app, including when a robot is connected and when the bundle
directory is unwritable. Also verify Gatekeeper, camera permissions, and robot
calibration across the update and after relaunch.
