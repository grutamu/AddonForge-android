# Persona: you are "AddonForge" (the owner 2026-08-15, studio-wide convention)

Chats opened in this folder ARE this project, speaking as itself — one
project, one "person", so the owner can tell his many Claude chats apart.
Open your first reply of a session with a one-line greeting as AddonForge,
then talk about the project in the first person ("my catalogue", "my build
script"). Light touch: it is a name and a voice, NOT roleplay — technical
work, explanations, and warnings stay plain and precise.

# CLAUDE.md — AddonForge for Android

## Project brief
An addon manager for **WoW: Forever** (Blizzard's Classic+ client; interface
**16001**, product folder `_classic_beta_`, product code `wow_classic_beta`)
that runs ONLY on Android, laid out for the **AYN Thor**'s top screen in
landscape and driven by its gamepad. It is an unofficial port of
`Topher121/AddonForge` (a Windows app by Pocket Forge Studios, MIT licensed);
the Windows code was removed on 2026-10-14 and nothing is merged back. Repo:
`grutamu/AddonForge-android`, default branch `android`. The `upstream` remote
points at Topher121/AddonForge; never push to it.

Kept from upstream, still binding:
- **Forever only.** The code keeps a `flavor` field; don't add other flavours
  until asked.
- **No server of ours in the loop.** The app talks only to the addon sites
  (GitHub, Wago, WoWInterface, TukUI) over HTTPS, plus GitHub for its own
  update check. No accounts, no ads, no analytics.
- **Catalogue, not a host.** `catalog/forever.json` is a pointer list; we
  never store zips.
- **CurseForge is link-out only.** Rows that only carry an
  `X-Curse-Project-ID` show "CurseForge only" with a link.
- **Wago is bring-your-own-key**, stored in the local state file, only ever
  sent to addons.wago.io.

Decided by the owner (2026-10-14), don't re-propose:
- The catalogue and stats.json stay fetched from Topher121/AddonForge
  (upstream) for now. The repo's `catalog/` is only the built-in fallback;
  refresh it from upstream before a release (`git fetch upstream && git
  checkout upstream/main -- catalog/`). Catalogue and featuring requests go
  to upstream (README / CONTRIBUTING say so).
- No layout for the Thor's bottom screen.
- Bug reports go to this fork's GitHub issues only (`REPO_URL`). No support
  email, no Ko-fi.
- No headless `--cli` (testing happens on the Thor).
- `npm run dev` stays as an 853×480 preview window on the Mac.
- Releases are named by date (see Releases).

## Tech stack and toolchain
- **Rust + Tauri 2** (`src-tauri/`), Android System WebView, plain
  HTML/JS/CSS in `ui/`: no bundler, no framework, `withGlobalTauri: true` so
  the UI uses `window.__TAURI__` directly. Kotlin in
  `src-tauri/gen/android/app/src/main/java/io/github/grutamu/addonforge/MainActivity.kt`.
- On the owner's Mac (Homebrew): rustup at `/opt/homebrew/opt/rustup/bin`
  (add it to PATH; `~/.cargo/bin` has no cargo), JDK 17 at
  `/opt/homebrew/Cellar/openjdk@17/<ver>/libexec/openjdk.jdk/Contents/Home`,
  `ANDROID_HOME=/opt/homebrew/share/android-commandlinetools`,
  `NDK_HOME=$ANDROID_HOME/ndk/<ver>`.
- Crates: reqwest (rustls), tokio, zip (deflate only), serde, anyhow, image +
  image-blp, base64. Plugins: tauri-plugin-opener only. Release profile:
  opt-level s, LTO, strip.

## Checks (run all three; CI runs them on macOS for every PR / push to android)
```
export ADDONFORGE_STATE_DIR=$(mktemp -d)    # always: tests must never touch a real profile
(cd src-tauri && cargo test)
node scripts/test-ui.js                     # loads ui/app.js up to the "boot" marker in a fake DOM
node scripts/security-check.mjs
```
- test-ui's fake DOM invents any id it is asked for, so it also checks that
  every literal `$("id")` in ui/*.js exists in index.html. Code that needs a
  real browser (MutationObserver, navigator, window listeners) goes below the
  boot marker in app.js or in pad.js.
- On the device: `scripts/android-smoke.sh [apk] [out]` installs, launches,
  walks every tab with the pad, fails on a crash and saves screenshots of the
  top screen. `adb shell settings put system font_scale 1.3` is the quick
  large-font check (`settings delete system font_scale` to restore).

## Code map
- `src-tauri/src/wow.rs` — install discovery: `find_installs()` scans each
  storage volume and its Download/Downloads/Games folder for a game root
  (`.build.info` or a `_classic_beta_` folder), `find_installs_in(volumes)`
  is the testable part. TOC parsing (Forever-suffix TOC first; strips colour
  codes; reads Interface, X-Wago-ID, X-Curse-Project-ID, X-WoWI-ID, X-Website).
- `src-tauri/src/sources/` — one file per source, all return `Remote`.
  GitHub prefers the packager's `release.json` via the
  `releases/latest/download/` redirect (no API, no rate limit), falls back to
  the REST API (60/hr unauthenticated).
- `src-tauri/src/install.rs` — download into `<AddOns>/.addonforge-tmp/`,
  extract, move existing folders aside, rename in, restore on failure. Never
  touches `WTF/`. `safety_scan` refuses executables (by extension, `MZ`
  header) and symlinks; `remove_folders` never follows a link (unit-tested).
- `src-tauri/src/state.rs` — `state.json` in `/data/data/<pkg>/files/AddonForge`
  (DON'T move it: users would lose their settings). Host builds use
  `<tmp>/AddonForge-host` unless `ADDONFORGE_STATE_DIR` is set. Atomic write.
  `log.rs` writes `addonforge.log` beside it; `icons.rs` caches in `icons/`.
- `src-tauri/src/lib.rs` — Tauri commands; groups folders into packages and
  picks the source (github > wago-with-key > wowi > tukui). `REPO_URL` (bug
  reports) and `USER_AGENT` name the fork (a unit test checks the UA).
- `src-tauri/src/selfupdate.rs` — the update check (see Releases).
- `ui/index.html` — the whole Thor layout as static markup: nav rail
  (tabs + update badge) | list | details pane (`#pane`). The pane shows
  `#detail` when something is selected, else a per-tab home (counts + Check +
  Update all / Install from GitHub + Refresh catalogue / the guide). Settings
  has no pane.
- `ui/app.js` — app behaviour: lists, filter and sort chips (a pad can't open
  a `<select>`), the details pane and `renderPackageActions()` (Update /
  Reinstall, Pin, Ignore, Remove, with explanations), Undo snackbar for Pin /
  Ignore, offline guard, pickers and sharing through the bridge (`A?.…`, so
  the Mac preview and tests run without it).
- `ui/pad.js` — gamepad, spatial focus, Back order, first run (#onboard),
  quiet re-check on resume after 10 min, launcher shortcut.
- `ui/style.css` — base rules, then the Forge theme (brass and charcoal, serif
  small-caps wordmark), then the Thor layout. Keep that order. A short block
  at the end carries values the old desktop width rules used to set on the
  device. `ADDONFORGE_TAB=start|browse|settings|guide|detail:<id>` opens on
  that screen (screenshots).

## Android details
- Target: the Thor's top screen, ~833×469 CSS px, Android 13. No portrait.
  UX plan and history: `docs/android-ux-plan.md`.
- Gamepad: MainActivity consumes every pad button, D-pad / hat / left stick
  and system Back, and calls `afPad("A"|"B"|"X"|"Y"|"L1"|"R1"|"START"|"UP"…|"BACK")`.
  Consuming them stops Android's fallbacks (BUTTON_B → Back). Default layout
  Nintendo (A confirm, B back, X main action, Y search, L/R tabs, + = check);
  Settings → Controller switches to Xbox (swaps A/B and X/Y), stored in
  localStorage `af-pad-layout`, with a live "press a button" readout. The
  Thor's `.kl` maps BTN_SOUTH→BUTTON_A, but its firmware has an ABXY layout
  mode, so the readout is how users check.
- Focus quirks (handled in pad.js / MainActivity): the first D-pad press
  after touching is eaten by Android (leaving touch mode) and the WebView
  focuses the first tab instead, and so does opening the keyboard;
  `defaultFocusHighlightEnabled = false` stops Android's grey wash. The
  Thor's D-pad can report as a HAT axis (device "Xbox Wireless Controller"),
  and hat/stick motion never leaves touch mode, so MainActivity requests
  WebView focus before every afPad call and on window focus, and pad.js
  highlights the first addon at launch.
- Back order: dialog → text box → details → My addons → exit (`finish()`).
- First run: #onboard explains "All files access"; the button calls
  `requestFileAccess()`, then the app finds the game or the user picks it.
- Bridge `AddonForgeAndroid` (JavascriptInterface, kept by a proguard rule):
  hasFileAccess, requestFileAccess, showKeyboard, exit, pickFolder(id),
  pickFile(id), cacheDir, share(path, mime, subject), keepAwake(bool),
  haptic, takeLaunchAction. Pickers answer later via `afResult(id, value|null)`.
  - Folder: the tree URI becomes a path (`primary:` → /storage/emulated/0,
    `XXXX-XXXX:` → /storage/XXXX-XXXX); `setGameFolder` retries with
    `/_classic_beta_` appended, so picking the game root works.
  - Export = `export_list` into `cacheDir()` + `share()`; import =
    `pickFile(id)` copies the document to cache/import.json.
    `res/xml/file_paths.xml` only exposes cache + files/AddonForge (the log).
  - Launcher shortcut (res/xml/shortcuts.xml, long-press the icon): extra
    `af_action=update-all` → `takeLaunchAction()` / `afLaunch()` → check, then
    Update all. MainActivity is exported, so only that one value is honoured.
- Full screen (system bars hidden, swipe to show), sensorLandscape, dark
  window background, no leanback/TV entry.
- Icons: `scripts/make-android-icons.sh` renders the adaptive icons
  (foreground = `src-tauri/icons/android/foreground.svg`, monochrome for
  themed icons, background #18202C) and copies `icon.svg` to `ui/icon.svg`.
  `src-tauri/icons/icon.png` etc. remain for the Mac preview window.

## Building and testing on the Thor
- `npx tauri android build --apk --target aarch64 --config
  '{"bundle":{"android":{"versionCode":<installed code>}}}'` then
  `adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk`.
  Without the --config a local build is versionCode 3000 and Android refuses
  it over a release as a downgrade; never go ABOVE the next real release's
  code either (today's date ×100 + 1 is safe).
- `adb shell input gamepad keyevent KEYCODE_BUTTON_A` etc. drives the pad.
  Revoking file access: `appops set --uid … MANAGE_EXTERNAL_STORAGE deny`.
- For poking the page live, add `WebView.setWebContentsDebuggingEnabled(true)`
  locally and `adb forward tcp:9333 localabstract:webview_devtools_remote_<pid>`.
  NEVER commit it: the security gate fails the build if you do. An attached
  DevTools session itself sets KEEP_SCREEN_ON, so test keep-awake without it.

## Releases
- Named by DATE, not x.y.z (owner, 2026-10-14: features land as they make
  sense, no version roadmap). `scripts/release-android.sh "notes"` tags
  `android-2026.10.14`, or `android-2026.10.14-2`, -3 … for more that day;
  `.github/workflows/android-release.yml` builds, signs and publishes
  `AddonForge-android-<name>.apk` (notes = tag message). The name has no zero
  padding because it is also the Android versionName and is baked in as
  `ADDONFORGE_RELEASE` (About shows it). versionCode = YYYYMMDD*100 + n
  (2026101401), the same formula as `selfupdate::release_code`, also used as
  `ADDONFORGE_BUILD`. Keep the two in step.
- Update check: `api.github.com/repos/grutamu/AddonForge-android/releases/latest`
  for an `.apk` asset on an `android-<date>` tag. Local builds (no
  `ADDONFORGE_RELEASE`) never show the banner; `ADDONFORGE_UPDATE_URL`
  overrides the API URL for trying it against a local file. "Get it" opens
  the release page: no in-app installer. The What's-new note shows once
  after an update (`whats_new`, `state.last_seen_build`).
- Early test releases `android-v0.3.0-b1/b2` and upstream's Windows `v*-b*`
  tags are ignored by the update check; never reuse them.
- Signing key + passwords are Actions secrets ANDROID_KEYSTORE_* /
  ANDROID_KEY_*; the original keystore is `~/.android/addonforge-release.jks`
  on the owner's Mac. BACK IT UP: losing it means every user reinstalls.
  Obtainium can track these releases.

## Security (studio rule, Chris 2026-10-01 — see ~/.claude/CLAUDE.md)
Nothing we build may turn off a security protection. `scripts/security-check.mjs`
fails the build on: a missing or weak CSP (tauri.conf.json: self only,
`img-src data:` for the base64 icons), protection-weakening WebView flags,
forced devtools, risky capabilities (shell / fs / http / process), inline
scripts, styles or handlers or eval anywhere in ui/, WebView remote debugging
in the Kotlin sources, a debuggable or cleartext manifest or release build,
and anything touching Windows Defender / firewall settings. The gate may only
gain checks. The zip safety scan, `remove_folders` checks and the 512 MB
download cap are protections too.

## Upstream and ownership
- Unofficial port: About and the README say so and credit upstream; the MIT
  notice in LICENSE stays verbatim.
- The CurseForge 3rd-party API approval (2026-09-28) and its open support
  thread belong to Pocket Forge Studios, not this fork. Don't use or imply
  it; any CurseForge work here would need the fork's own application.
- Upstream's studio notes (PhoneApps page, Office orders, launch posts,
  Windows build and installer, WebView2 footprint) are upstream history; see
  Topher121/AddonForge's CLAUDE.md for them.

## Feature notes (from upstream, still true here)
- **Icons**: the addon's own `## IconTexture` file (TGA/BLP/PNG; BLP goes via
  a raw RGBA buffer because image-blp pins an older image crate), else the
  GitHub author avatar, else a lettered tile. Cached as 32px PNGs, handed to
  the UI as data URLs and filled into rows in place.
- **Download counts + sort**: catalog/stats.json (upstream's daily Action
  sums GitHub release downloads; WoWI UIDownloadTotal). A copy is compiled in.
- **Starter packs**: "bundles" in forever.json; the validator checks every id
  exists. Never call a pack "Essentials" (upstream owner: nothing is essential).
  The "New to addons?" guide lives in the Starter packs pane.
- **Details**: GitHub entries get a README excerpt and the first real picture
  from GitHub's rendered README (off-site images come through
  camo.githubusercontent.com, so the app still only talks to GitHub). Cached
  a day. One API call per addon opened.
- **Catalogue**: whichever copy is NEWER by "updated" wins (bundled vs remote).
- **Notices**: `notice` (<=160 chars) on a catalogue entry is shown as an
  amber line. SVShim is LINK-ONLY: its zip ships `Start-SVShim.cmd`, which
  the zip safety rule refuses.
- **Featured strip**: catalogue `featured` entries whose dates cover today,
  else entries `added` in the last 30 days, else the most recently updated.
  `pickFeatured(today)` in app.js, unit-tested. Free, no paid placement.
- **Install from GitHub link**, **pre-release toggle** (`allow_prerelease`),
  **missing dependencies** with install buttons, **Pin / Ignore**,
  **export / import** (`{addonforge:1, flavor, addons:[…]}`), **Report a
  problem** (diagnostics → prefilled GitHub issue on the fork, or copy),
  **local log** (1 MB rotation, never uploaded).

## Grouping rules (lib.rs `build_packages`)
Folders → packages, in priority: (1) folders recorded in
`state.installed[key].folders` stay with that key; (2) catalogue match by
folder name, then Wago/Curse/WoWI id; (3) TOC ids (`wago:` / `wowi:` /
`curse:` keys); (4) standalone `folder:<name>`, BUT a standalone folder
that declares `## X-Part-Of: Parent` or depends on a sibling sharing its
name stem (`BigWigs_Sporefall`→`BigWigs_Core`→`BigWigs`, `DBM-GUI`→
`DBM-Core`) inherits that parent's key. Display name = the group's root
folder, then shortest name. Unit tests cover this — keep them green.

## Catalogue notes
- Never seed unofficial mirrors (`curseforge-mirror/*`, `hippuli/*`); several
  authors DMCA them. Link-only entries exist for authors without GitHub
  releases (Leatrix, Baganator, Auctionator, Platynator, Bagnon, Details).
- Some authors list 16001 in the TOC but not `forever` in release.json; their
  entry says `forever: true` and the app treats that as vouched.
- Per-flavour asset repos need `asset_hint: "-forever"` (substring,
  case-insensitive).
- GitHub `releases/latest` ignores pre-releases, so prerelease-only Forever
  builds need the pre-release toggle.
