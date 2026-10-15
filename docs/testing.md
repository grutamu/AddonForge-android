# Testing

How AddonForge for Android is tested. There are three layers:

1. **Host checks:** Rust unit tests, UI checks and the security gate. They
   run on the Mac and in CI.
2. **Device smoke test:** a script that drives the real app on the Thor with
   the gamepad.
3. **Manual checks on the Thor:** anything that changes the game folder, plus
   the Android system screens.

The commands below assume the toolchain from CONTRIBUTING.md (Rust with
`aarch64-linux-android`, JDK 17, Android SDK + NDK, Node, adb).

## 1. Host checks

```sh
export ADDONFORGE_STATE_DIR=$(mktemp -d)   # always: never touch a real profile
(cd src-tauri && cargo test)
node scripts/test-ui.js
node scripts/security-check.mjs
```

All three must pass before a commit. CI runs the same three on macOS for
every pull request and every push to `android` (`.github/workflows/build.yml`;
changes to only `catalog/` or Markdown skip it). The release workflow runs
the UI checks and the security gate again before it builds.

### `cargo test`: 14 tests

| Area | Test | What it proves |
|---|---|---|
| Install discovery | `wow::finds_game_roots_on_a_volume_and_in_download` | The Android storage scan finds a game root (`.build.info` + `_classic_beta_`) at a volume's top level and in its Download folder |
| TOC parsing | `wow::strips_colour_codes` | `\|cffXXXXXX` colour codes are stripped from titles |
| Grouping | `managed_folders_stay_with_their_package`, `modules_group_under_their_core_without_catalogue`, `managed_source_wins_and_github_key_is_used` | Folders group into the right addon (BigWigs/DBM modules under their core); a recorded install source wins |
| Dependencies | `missing_dependencies_are_reported_and_matched_to_catalogue` | Missing required deps are listed and matched to catalogue entries |
| Install safety | `install::safety_scan_refuses_executables` | A zip with an executable (extension or `MZ` header) or a symlink is refused |
| Install safety | `install::removing_a_folder_never_follows_a_link` | Removing an addon folder never deletes through a link (SavedVariables stay safe) |
| Sources | `sources::parses_github_inputs` | `owner/repo` and GitHub URLs parse |
| Details pane | `icons::readme_image_skips_badges_and_resolves_relative`, `icons::html_image_prefers_camo_and_skips_badges`, `icons::excerpt_strips_markup` | The README picture and excerpt skip badges and come only through GitHub |
| Update check | `selfupdate::android_release_names` | `android-YYYY.M.D[-n]` tags turn into the right release code; other tags are ignored |
| Network identity | `user_agent_names_the_fork` | The User-Agent names this fork |

No test touches the network or a real game folder.

### `scripts/test-ui.js`

This loads `ui/app.js` up to its "boot" marker in a fake DOM and checks
counts, which addons are eligible for updates, filters, search, HTML
escaping, empty states, modal focus and cancel, and the addon actions in the
details pane. It also fails if any `$("id")` in `ui/*.js` is missing from
`index.html`, because the fake DOM would otherwise invent the element. Code
that needs a real browser (MutationObserver, navigator, window listeners)
lives below the boot marker or in `pad.js`, so this script doesn't cover it.
The smoke test covers it instead.

### `scripts/security-check.mjs`

Fails the build if:

- the CSP is missing or weak,
- there is an inline script or style anywhere in `ui/`,
- the Android release is debuggable or allows cleartext traffic,
- WebView debugging was left enabled,
- anything tries to turn off a webview or Windows protection.

## 2. Device smoke test (Thor over USB)

Turn on USB debugging, connect the Thor and check that `adb devices` lists
it. Then:

```sh
scripts/android-smoke.sh [path/to/app.apk] [out-dir]
```

What the script does:

1. Installs the APK, if you pass one.
2. Wakes the screen, force-stops the app, clears logcat and launches the app.
3. Opens an addon in the details pane with the pad, then closes it.
4. Steps through every tab with R1, opens search with Y and backs out with B.
5. After each step, fails if the process has died, and at the end fails if
   anything was written to the crash log.
6. Saves screenshots of the top screen (`1-launch.png` … `6-back-home.png`)
   to the output folder, by default `$TMPDIR/addonforge-smoke`.

It never presses A on an addon action, so it doesn't install, update or
remove anything. Look through the screenshots afterwards: the script catches
crashes, not layout bugs.

### Building an APK to test

```sh
npx tauri android build --apk --target aarch64 \
  --config '{"bundle":{"android":{"versionCode":<installed code>}}}'
adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk
```

Set `versionCode` to the code of the release already installed. Without it a
local build is 3000, and Android refuses it over a release as a downgrade.
Never go above the next real release's code either (today's date × 100 + 1
is safe). The signing key has to match the installed copy:
`~/.android/addonforge-release.jks` via the gitignored
`src-tauri/gen/android/keystore.properties`.

### Useful adb commands

```sh
adb shell input gamepad keyevent KEYCODE_BUTTON_A        # drive the pad (B, X, Y, R1, DPAD_DOWN…)
adb exec-out screencap -p -d <display-id> > shot.png     # top screen only (the script finds the id)
adb logcat -d -b crash | grep io.github.grutamu.addonforge
adb shell settings put system font_scale 1.3             # large-font check
adb shell settings delete system font_scale              # restore
adb shell appops set --uid io.github.grutamu.addonforge MANAGE_EXTERNAL_STORAGE deny     # revoke file access
adb shell appops set --uid io.github.grutamu.addonforge MANAGE_EXTERNAL_STORAGE allow    # grant without the Settings screen
adb shell pm clear io.github.grutamu.addonforge          # fresh first launch (loses AddonForge's own settings only)
```

Inspecting the page live: add `WebView.setWebContentsDebuggingEnabled(true)`
locally, then
`adb forward tcp:9333 localabstract:webview_devtools_remote_<pid>` and open
it in Chrome. Never commit that line; the security gate fails if you do. An
attached DevTools session sets KEEP_SCREEN_ON on its own, so test keep-awake
without it.

## 3. Manual checks on the Thor

No automated check covers these, so go through them by hand on the device
when the related code changes. The ones marked **writes** change the real
AddOns folder. Before trying them, back up
`<game folder>/_classic_beta_/Interface/AddOns`, e.g.
`adb pull "/storage/emulated/0/Download/World of Warcraft/_classic_beta_/Interface/AddOns" ./addons-backup`.

| Check | How |
|---|---|
| First-launch permission flow | `pm clear`, set `MANAGE_EXTERNAL_STORAGE` to `deny`, launch. The app should explain why, send you to the switch, and find the game folder once you come back |
| Game folder detection | Launch with the game in the default folder, in `Download/`, and (if you have one) on an SD card |
| **writes** Install | Install one small addon (e.g. BlizzMove). It appears in My addons, `Interface/AddOns` has its folder(s) and no `.addonforge-tmp` is left behind, and it shows in the in-game addon list |
| **writes** Update | Pin a version or install an older one, run Check, then Update. Only that addon's folders change |
| **writes** Remove | Remove it. Only the folders it owns are deleted; `WTF/` is untouched |
| **writes** Starter pack "Install all" | Each addon installs in turn; ones not built for Forever are skipped with a message |
| Export / Import | The system pickers and share sheet open, and an exported list imports back with the right plan |
| Offline | Turn off Wi-Fi: the offline banner shows, and nothing crashes on Check |
| Resume re-check | Leave the app for more than 10 minutes and come back: it re-checks quietly |
| Update banner | Set `ADDONFORGE_UPDATE_URL` to a local file on a release build. Local builds never show the banner |
| Large font | `font_scale 1.3`, then walk every tab |
| On-screen keyboard | Search and the Wago key field. On the Thor the keyboard opens on the bottom screen |

## Test history

**2026-10-14, first port (commit `ca4e065`).** This was before the Thor
layout and pad support. Host checks: 11 Rust tests, the UI checks and the
security gate all passed. The release APK (12 MB) was installed on the Thor
(Android 13) with file access granted via `appops`, then launched with
`monkey` and checked with screenshots and logcat:

- no crash,
- it found `Download/World of Warcraft/_classic_beta_` by itself,
- all 12 addons on disk were listed (they show "No source", which is
  correct),
- the update check ran,
- the live catalogue loaded (102 entries).

Installing and the permission screen were not tried in that round.

**2026-10-15, `android` at `170b1ce`.** Host checks: 14 Rust tests, the UI
checks and the security gate all passed. No device run was recorded with
this entry.
