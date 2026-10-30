# Testing

How AddonForge for Android is tested. There are three layers:

1. **Host checks:** Rust unit tests, UI checks and the security gate. They
   run on the Mac and in CI.
2. **Device smoke test:** a script that drives the real app on the Thor with
   the gamepad.
3. **Manual checks on the Thor:** anything that changes the game folder, plus
   the Android system screens.

The commands below need Rust (with the `aarch64-linux-android` target),
JDK 17, the Android SDK and NDK, Node 20 and adb. Run `npm ci` once first.

## 1. Host checks

```sh
export ADDONFORGE_STATE_DIR=$(mktemp -d)   # always: never touch a real profile
(cd src-tauri && cargo test)
node scripts/test-ui.js
node scripts/security-check.mjs
```

All three must pass before a commit. CI runs the same three on macOS for
every pull request and every push to `android` (`.github/workflows/build.yml`;
Markdown-only changes skip it). The release workflow runs the UI checks and
the security gate again before it builds.

### `cargo test`: 30 tests

| Area | Test | What it proves |
|---|---|---|
| Install discovery | `wow::finds_game_roots_on_a_volume_and_in_download` | The Android storage scan finds a game root (`.build.info` + `_forever_`) at a volume's top level and in its Download folder |
| Install migration | `adopts_a_forever_install_only_when_the_current_one_is_not_forever` | The live Forever client is adopted when the recorded selection is missing or no longer Forever (e.g. a leftover beta), and a current Forever selection is left alone |
| TOC parsing | `wow::strips_colour_codes` | `\|cffXXXXXX` colour codes are stripped from titles |
| Grouping | `managed_folders_stay_with_their_package`, `modules_group_under_their_core_without_catalogue`, `managed_source_wins_and_github_key_is_used`, `folder_matching_prefers_the_addon_named_after_the_folder` | Folders group into the right addon (BigWigs/DBM modules under their core); a recorded install source wins; a shared folder goes to the addon named after it |
| Dependencies | `missing_dependencies_are_reported_and_matched_to_catalogue` | Missing required deps are listed and matched to catalogue entries |
| Catalogue | `catalog::converts_forever_curse_entries_and_links_unique_github_repos` | instawow's list is filtered to Forever CurseForge entries, and a GitHub repo is attached only when exactly one claims the entry |
| Versions | `sources::bundle_release_names_every_folder_version`, `bundle_addon_shows_its_own_version_and_matches_the_bundle_release` | A bundle release (Questie `v12.1.0+v1.0.6`) is up to date when it names every folder's version |
| CurseForge | `sources::curseforge::picks_newest_forever_release_before_betas`, `sources::curseforge::alpha_only_when_nothing_else_and_flags_non_forever`, `sources::curseforge::cdn_url_splits_the_id_and_encodes_the_name`, `curseforge_comes_first_and_github_is_offered_beside_it`, `state::tests::old_state_with_removed_fields_still_loads` | The cfwidget file pick and download URL, CurseForge is the first choice (GitHub-only addons still use GitHub; a GitHub repo is offered as the switch, and a chosen GitHub source sticks), and a state.json with the old `curse_unofficial` field still loads |
| Install safety | `install::safety_scan_refuses_executables` | A zip with an executable (extension or `MZ` header) or a symlink is refused |
| Install safety | `install::removing_a_folder_never_follows_a_link` | Removing an addon folder never deletes through a link (SavedVariables stay safe) |
| Go back | `install::an_update_keeps_the_old_version_and_restore_swaps_them`, `install::a_failed_restore_puts_everything_back` | An update that changes the version keeps the replaced folders in `.addonforge-backup/`; going back swaps them in (removing folders the old version didn't have) and keeps the newer one, so a second go-back undoes the first; a backup missing a folder is unusable, and a go-back that fails halfway undoes every move |
| Release notes | `sources::release_notes_become_plain_text`, `sources::cumulative_changelog_stops_at_the_installed_version`, `sources::long_notes_are_clipped_at_a_line_end`, `sources::github::notes_cover_the_releases_newer_than_the_installed_one` | BBCode (WoWInterface), Markdown and HTML notes become plain text; a whole-history changelog (WoWInterface, TukUI) stops at the installed version's entry; GitHub gives each release newer than the installed one; long notes are cut at a line end |
| Sources | `sources::parses_github_inputs` | `owner/repo` and GitHub URLs parse |
| Details pane | `icons::readme_image_skips_badges_and_resolves_relative`, `icons::html_image_prefers_camo_and_skips_badges`, `icons::excerpt_strips_markup` | The README picture and excerpt skip badges and come only through GitHub |
| Update check | `selfupdate::android_release_names` | `android-YYYY.M.D[-n]` tags turn into the right release code; other tags are ignored |
| Network identity | `user_agent_names_the_fork` | The User-Agent names this fork |

No test touches the network or a real game folder.

### `scripts/test-ui.js`

This loads `ui/app.js` up to its "boot" marker in a fake DOM and checks
counts, which addons are eligible for updates, filters, search, HTML
escaping, empty states, modal focus and cancel, the addon actions in the
details pane, Update all (one addon at a time, the "no Forever build"
question asked once at the end, accepted or declined, and a summary that
counts updated, failed and skipped), release notes in the details (escaped,
and CurseForge's "no notes" line), switching an addon between CurseForge and GitHub (CurseForge ids sent as numbers), and going back to the previous version
(it pins when that is older than the latest, and Undo unpins). It also fails if any `$("id")` in `ui/*.js` is missing from
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
6. Saves screenshots of the top screen (`1-launch.png` … `5-back-home.png`)
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
`<game folder>/_forever_/Interface/AddOns`, e.g.
`adb pull "/storage/emulated/0/Download/World of Warcraft/_forever_/Interface/AddOns" ./addons-backup`.

| Check | How |
|---|---|
| First-launch permission flow | `pm clear`, set `MANAGE_EXTERNAL_STORAGE` to `deny`, launch. The app should explain why, send you to the switch, and find the game folder once you come back |
| Game folder detection | Launch with the game in the default folder, in `Download/`, and (if you have one) on an SD card |
| **writes** Install | Install one small addon (e.g. BlizzMove). It appears in My addons, `Interface/AddOns` has its folder(s) and no `.addonforge-tmp` is left behind, and it shows in the in-game addon list |
| **writes** Update | Pin a version or install an older one, run Check, then Update. Only that addon's folders change |
| **writes** Update all | With two or more updates waiting, press Update all: the pane shows "Updating 1 of N" with a progress bar, then a summary that matches what happened. An addon without a Forever build is asked about once, at the end |
| **writes** Go back | After an update, the addon's details offer "Go back to <old version>". It puts the old folders back and pins the addon; `.addonforge-backup/` then holds the newer one, and Go back again (or Undo) returns to it |
| Release notes | Open an addon with an update waiting: "What's new" shows the notes for GitHub, Wago, WoWInterface and TukUI addons, and a line saying there are none for CurseForge ones |
| **writes** Remove | Remove it. Only the folders it owns are deleted, along with its `.addonforge-backup/` copy; `WTF/` is untouched |
| Pad buttons | Press the physical buttons: A selects, B goes back, X runs the main action, Y opens search. The app follows Android's mapping, so with the Thor's ABXY layout mode switched, A and B swap |
| Report on GitHub | Settings → Report on GitHub while signed out of GitHub in the browser: GitHub's sign-in page opens (not an error page) and the diagnostics are on the clipboard |
| Export / Import | The system pickers and share sheet open, and an exported list imports back with the right plan |
| Offline | Turn off Wi-Fi: the offline banner shows, and nothing crashes on Check |
| Resume re-check | Leave the app for more than 10 minutes and come back: it re-checks quietly |
| Update banner | Set `ADDONFORGE_UPDATE_URL` to a local file on a release build. Local builds never show the banner |
| Large font | `font_scale 1.3`, then walk every tab |
| On-screen keyboard | Search and the Wago key field. With the app on the top screen the keyboard opens on the bottom screen. Moving onto a text box with the D-pad must not open it; A or Y does |
| Lower screen | `adb shell am start --display <id> -n io.github.grutamu.addonforge/.MainActivity` (the id from `adb shell dumpsys display`; `Screen-2` is the lower one), then drive it with `adb shell input -d <id> keyevent KEYCODE_…`. The list fills the width with a bar of buttons under it, an addon's details replace the list until B, and the keyboard pushes the page up instead of covering it |

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

**2026-10-15, `prototype/instawow-catalogue` merged into `android`.**
Catalogue from instawow's list; starter packs, the guide and the controller
layout setting removed; report link fix. Host checks: 22 Rust tests, the UI
checks and the security gate all passed. On the Thor, the smoke test passed
(three tabs, no crash), the catalogue loaded (5,210 entries) and A / B / Y
did their jobs when sent through adb. "Report on GitHub" while signed out
used to open a ~5,900-character link that GitHub answered with an error
page; it now opens the sign-in page with the diagnostics copied. X and the
physical buttons were not tried.

**2026-10-19, lower-screen layout.** Host checks: 23 Rust tests, the UI
checks and the security gate all passed. On the Thor, the app was run on
the lower screen (538x444 CSS px) and the top screen, driven with the pad
through adb, with screenshots of every tab, an installed addon's and a
catalogue addon's details, Back, and the keyboard in search and the Wago
key field. The first keyboard after an `adb install` once showed a blank
page until the next key press; it didn't happen again on fresh launches.
Nothing was installed or removed.

**2026-10-19, Update all progress, release notes, Go back.** Host checks:
29 Rust tests, the UI checks and the security gate all passed. The
release-notes code was also run once against the live WoWInterface, TukUI
and GitHub APIs (WeakAuras, ElvUI) with a throwaway test, and gave clean
notes cut at the installed version. On the Thor (top screen), the smoke
test passed with no crash. No addon had an update waiting, and nothing was
installed or updated, so the progress panel, What's new and Go back were
not seen on the device.

**2026-10-29, live WoW: Forever client.** The beta ended; the live client
ships in a `_forever_` folder with `.flavor.info` product `wowf` (confirmed
by reading it off the Thor over wireless adb). Detection, the folder-picker
fallback, and the discovery test were moved from the beta's
`_classic_beta_`/`wow_classic_beta` to the live strings, and a startup
migration adopts the live install when the recorded selection is a leftover
beta. Host checks: 30 Rust tests, the UI checks and the security gate all
passed. On the Thor, a release APK (versionCode 2026102900) was installed
over the beta-configured app and smoke-tested: it auto-migrated the saved
`_classic_beta_` selection to `_forever_`, showed the (empty) live AddOns
folder with "No addons found", and walked every tab with the pad with no
crash. Nothing was installed or removed.
