# Contributing

## Adding an addon, or getting one featured

The catalogue (`catalog/forever.json`) and the download counts (`catalog/stats.json`) are maintained by the original [AddonForge](https://github.com/Topher121/AddonForge) project, and this app reads them from there. To list an addon or ask for it to be featured, follow [their contributing guide](https://github.com/Topher121/AddonForge/blob/main/CONTRIBUTING.md). Featuring is free there too.

The copy in this repo's `catalog/` is only the offline fallback built into the APK. It is refreshed from upstream before releases (`git checkout upstream/main -- catalog/`), so please don't send catalogue changes here.

## Bugs

Use **Settings → Report a problem → Report on GitHub** in the app, or [open an issue](https://github.com/grutamu/AddonForge-android/issues). The diagnostics report helps a lot.

## Code changes

Rust core in `src-tauri/`, plain HTML/JS/CSS in `ui/` (no bundler, no framework). Keep the promises in the README: no accounts, no ads, no analytics, no server of ours in the loop, no CurseForge API key in the app, and nothing outside `Interface/AddOns` changed.

### Build

Needs Rust with the `aarch64-linux-android` target, Node, JDK 17, the Android SDK and NDK (`ANDROID_HOME`, `NDK_HOME`).

```
npm install
npx tauri android build --apk --target aarch64
adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk
```

A release build needs signing details in `src-tauri/gen/android/keystore.properties` (gitignored). To install over a release from this repo, it must be signed with the same key and pass the installed versionCode: add `--config '{"bundle":{"android":{"versionCode":<installed code>}}}'`. Local builds never show the update banner.

`npm run dev` opens the same UI in an 853×480 window on your computer, handy for layout work. The arrow keys stand in for the D-pad there; the Android-only buttons (pickers, sharing) do nothing.

### Checks

All three run in CI on every pull request, and the security gate also runs before every release:

```
(cd src-tauri && cargo test)          # set ADDONFORGE_STATE_DIR to a temp folder
node scripts/test-ui.js               # UI behaviour without a browser
node scripts/security-check.mjs       # CSP, no inline scripts, no debuggable or cleartext release
```

Then on a device: `scripts/android-smoke.sh path/to/app.apk` installs the APK, walks every tab with the gamepad, fails on a crash and saves screenshots.

The full testing guide, including the manual checks on the Thor and the test history, is in [docs/testing.md](docs/testing.md).

### Releases

Releases are named by date and built and signed by GitHub Actions. `scripts/release-android.sh "what changed"` tags today's release (`android-2026.10.14`, then `-2` for a second one that day) and the workflow publishes the APK. Only the maintainer can release, because only the maintainer has the signing key.
