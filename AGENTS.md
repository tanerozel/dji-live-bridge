# DJI Live Bridge — agent instructions

Applies to Codex, Claude Code and any other coding agent working in this repo.

## Building the macOS app (read before running any build)

**The only supported build command is `npm run build:mac`.**

- Never run `tauri build`, `tauri bundle` or `npm run tauri -- build`. `scripts/tauri.sh` blocks them on purpose.
- Why: Tauri signs the bundled `mediamtx` sidecar with the app's restricted entitlements
  (`system-extension.install`, `application-groups`). Without a provisioning profile macOS SIGKILLs the
  sidecar at launch (exit 137), MediaMTX never starts, and the app shows
  `HTTP error: error sending request for url (http://127.0.0.1:9997/v3/paths/list)`.
- `build:mac` builds the `.app`, re-signs the sidecar with empty entitlements (`scripts/sign-sidecars.sh`),
  verifies the bundle (`scripts/verify-bundle.sh`), creates the `.dmg` from the fixed app, and verifies the
  app inside the DMG. If any check fails the build fails — do not work around it.
- Do not run `scripts/sign-sidecars.sh` by hand, do not use the DMG Tauri generates, and do not add
  `--deep` to a `codesign` call (it re-applies the restricted entitlements to the sidecar).
- Check any installed/built app: `npm run verify:bundle -- "<path to DJI Live Bridge.app>"`.
- `cargo` may be missing from `PATH`; `build:mac` finds the pinned toolchain itself. For manual cargo use
  `~/.rustup/toolchains/<channel from rust-toolchain.toml>-aarch64-apple-darwin/bin`.
- Quit the app before building. `build:mac` refuses to run while it is open and stops an orphaned
  MediaMTX that holds ports 1935/8554/9997.

Signing details: identity `Developer ID Application: Taner Ozel (GXDXLCQ92M)` (override with
`APPLE_SIGNING_IDENTITY`).

**Notarization is mandatory, not optional.** macOS only activates a Developer ID signed system
extension when the app is notarized; otherwise the virtual camera fails with
`code=8 domain=OSSystemExtensionErrorDomain desc=code signature invalid`. `build:mac` notarizes
and staples both the `.app` and the `.dmg` using the notarytool keychain profile `dji-live-bridge`
(override with `NOTARY_PROFILE`), and `verify-bundle.sh` fails if the ticket is missing.
- If the profile is missing, create it once:
  `xcrun notarytool store-credentials dji-live-bridge --key <AuthKey.p8> --key-id <id> --issuer <issuer>`.
  Never commit the `.p8` key or put credentials in the repo.
- `NOTARIZE=0 npm run build:mac` is only for UI-only iteration; that build's camera will not work.
- After re-signing anything inside an already stapled app, the ticket is invalid: rebuild instead.
- Replacing `/Applications/DJI Live Bridge.app` makes macOS deactivate the old camera extension; the
  user must press Start Virtual Camera (which re-requests activation) and may need to approve or reboot.

## Bundled FFmpeg

The app ships its own FFmpeg and ffprobe so users install nothing. They are built by
`./scripts/build-ffmpeg.sh` into `src-tauri/binaries/ffmpeg-<triple>` (not checked in) and
`npm run build:mac` refuses to start if they are missing.

- The build is **LGPL**: `--disable-gpl` and no external libraries. Never enable GPL or libx264;
  `verify-bundle.sh` fails the build if `--enable-gpl` appears.
- That means **no GPL-only filters** in the pipeline: use `gblur` (not `boxblur`), `colorlevels`
  (not `eq`), and H.264 through `h264_videotoolbox`. A unit test enforces this.
- FFmpeg's licence texts must stay in `Contents/Resources/licenses`; the build script copies them.

## Checks before finishing a change

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --locked --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
npm run typecheck && npm run lint
```

## Runtime invariants

- Stream keys live only in the macOS Keychain; never in config files, logs, or process arguments.
- Every child process (MediaMTX, FFmpeg) must be stopped on app exit. macOS quit emits `RunEvent::Exit`,
  not only `ExitRequested`; keep both handled (`src-tauri/src/lib.rs`).
- If MediaMTX dies at startup the error must say why (`sidecar_exit_message` in `src-tauri/src/mediamtx.rs`).
