# Removing Windows support: plan

Status: plan only, written 2026-10-14. Nothing here has been changed yet.

Target after the cleanup: one app for **Android on the AYN Thor** (landscape,
gamepad). The Thor layout from `ui/touch.css` / `ui/touch.js` becomes the only
layout. The fork stops carrying the Windows exe, installer, self-updater,
drive scan, Explorer calls and desktop UI.

**"No Windows" does not mean "no host build".** `cargo test` compiles the
crate for macOS, and `node scripts/test-ui.js` runs `ui/app.js` in Node. Both
must keep working, so the Rust code still has to compile on the Mac and the UI
still has to load without a device. Every phase below says how it keeps that
true.

## Baseline (measured 2026-10-14 on the owner's Mac, before any change)

| Check | Result |
|---|---|
| `node scripts/test-ui.js` | passes |
| `node scripts/security-check.mjs` | passes (CSP 238 chars) |
| `cargo test` (in `src-tauri/`, `ADDONFORGE_STATE_DIR` set to a temp dir) | 12 passed |

Findings from the baseline:

- `install::tests::removing_a_folder_never_follows_a_junction` is
  `#[cfg(windows)]`, so it never runs on the Mac. Today it only runs in
  `.github/workflows/build.yml` on `windows-latest`. Deleting that workflow
  without porting the test would silently drop the only test of "removing an
  addon folder never deletes what a link inside it points to". Phase 1 ports it.
- `cargo` is at `/opt/homebrew/opt/rustup/bin` on this Mac, not
  `~/.cargo/bin` as `CLAUDE.md` says.
- `scripts/test-ui.js` only loads `ui/app.js` up to the
  `// ---------------------------------------------------------------- boot`
  marker. Its fake DOM **creates any id on request**, so the test can't
  catch a reference to an element that was deleted from `index.html`. Phase 2
  adds a check for that before any markup is removed.
- `scripts/test-ui.js` currently asserts on the desktop "···" row-menu
  positioning (the `toggle` listener on `#installed-list`). Removing the row
  menu means rewriting those assertions, not just deleting code.
- `scripts/security-check.mjs` only scans `ui/index.html` and `ui/app.js` for
  inline scripts, styles and handlers. `ui/touch.js` isn't scanned.
- **Ordering trap:** `isAndroid()` is `settings?.platform === "android"`. If
  the Rust `platform` field is removed before the UI branches are collapsed,
  the device gets `undefined`, `isAndroid()` turns false and the Thor shows
  the desktop layout. The UI collapse has to land first (phase 6, before
  phase 7).

## In-flight work this plan has to wait for

While this plan was being written (2026-10-14), uncommitted changes appeared
in the checkout, not part of this plan: `MainActivity.kt` (+94 lines),
`AndroidManifest.xml`, `res/values/strings.xml`, `res/xml/file_paths.xml`,
new `res/xml/shortcuts.xml`, `ui/app.js`, `ui/index.html`, `ui/touch.css`,
`ui/touch.js`, and a new `scripts/android-smoke.sh`. They implement UX-plan P1
items: a system folder picker and file picker through new `Bridge` methods
(`pickFolder`, `pickFile`, answered by `window.afResult(id, value)`), export
through the share sheet (`cacheDir`, `share`), "Share the log", `keepAwake`
during installs, a haptic tick, and launcher shortcuts (`takeLaunchAction`).
The rows below take that work into account. **Start phase 1 only after that
work is committed**, then re-check the UI rows against what landed.

What it changes here:
- `#btn-pick` stays ("Choose folder…" through the bridge). Only its desktop
  `T.dialog` handler goes.
- Export / import no longer use the fixed `Download/addonforge-list.json`
  (`ANDROID_LIST` is gone). `touch.js` replaces the `app.js` handlers with the
  share sheet and the file picker. `app.js` still has the `T.dialog` versions,
  now only reachable on the desktop, so removing `tauri-plugin-dialog` still
  holds.
- `touch.js` now wraps `setBusy` and `toast` by reassigning the `app.js`
  functions. With one layout, call `A?.keepAwake` / `A?.haptic` directly in
  `app.js` instead (with `?.`, so the test's fake DOM is unaffected).
- `scripts/android-smoke.sh` (install, walk every tab with the pad, fail on a
  crash, screenshot the top display) is the on-device check used below.

## 1. Inventory

Actions: **remove**, **rewrite** (keep the job, change how), **keep**.
Risk: what could break, and how badly.

### Build and release

| File / symbol | What it does | Action | Why | Risk |
|---|---|---|---|---|
| `build.ps1` | Bumps `build\.buildnum`, runs tests, builds the exe, writes `latest.json`, publishes `v<ver>-b<n>` releases to Topher121 | remove | Windows-only. Android releases are `scripts/release-android.sh` + `android-release.yml`. Its `-SetVersion` sync (tauri.conf.json, Cargo.toml, package.json) is the only Windows-free part | Low. The version is bumped by hand in three files after this. Optionally add a `--set-version` flag to `release-android.sh` later (not part of this cleanup) |
| `scripts/make-icons.ps1` | Renders desktop icons into `src-tauri/icons/` and **copies `icon.svg` → `ui/icon.svg`** | remove, after moving the `ui/icon.svg` copy into `scripts/make-android-icons.sh` | The header/onboarding mark in the UI comes from that copy | Low if the copy line moves first |
| `scripts/dev-shot.ps1` | Win32 screenshot of the exe window | remove; document `adb exec-out screencap -p > shot.png` in CLAUDE.md instead | Windows-only | None |
| `.github/workflows/build.yml` | `cargo test` on `windows-latest`, on push to `main` / PRs | rewrite: host job (macOS) running `cargo test`, `node scripts/test-ui.js`, `node scripts/security-check.mjs`; trigger on `android` branch + PRs | It's the only CI test run outside a release, and it runs on Windows. The fork's default branch is `android`, so the `push: main` trigger never fires | Medium: the junction test only runs here today (port it first, phase 1) |
| `.github/workflows/android-release.yml` | Signed APK release from `android-v*` tags | **keep untouched** | The release pipeline. It already runs test-ui and the security gate | High if touched. See "What not to touch" |
| `scripts/release-android.sh` | Tags `android-v<ver>-b<n>` | keep; only the comment about "Windows tags (v*-b*)" stays true and stays | Tag scheme stays | None |
| `scripts/make-android-icons.sh` | Adaptive launcher icons | keep, plus the `ui/icon.svg` copy from make-icons.ps1 | | Low |
| `package.json` `"build": "tauri build --no-bundle"` | Builds the desktop exe | rewrite to `tauri android build --apk --target aarch64`, or remove | Produces a Windows/mac exe nobody ships | None |
| `package.json` `"dev": "tauri dev"` | Desktop dev window | keep **if** the Mac preview host is kept (open question Q4) | Fastest loop for layout work | None |
| `tauri.conf.json` `bundle.targets: ["nsis"]`, `bundle.windows.nsis`, `bundle.icon` `icon.ico` | NSIS installer and Windows icon | remove; `bundle.active` can stay true (Android ignores `targets`) | Windows-only | Low; prove with a CI APK build |
| `tauri.conf.json` `bundle.publisher: "Pocket Forge Studios"` | Installer publisher name | remove with the NSIS block | Only the installer used it. Branding question either way | None |
| `tauri.conf.json` `app.windows[0].additionalBrowserArgs` | WebView2 memory flags | remove | WebView2 only. Android's System WebView ignores them | Low. The security gate still loops over `windows` and still has an entry to check |
| `tauri.conf.json` `width/height/minWidth/minHeight/center` | Desktop window size | rewrite: 853×480, no min size, if the Mac preview is kept; else drop. **Keep the window entry itself** (label `main`): Tauri creates the Android webview from it and `capabilities/default.json` targets `"main"` | Android ignores size | Medium if the whole `windows` array is deleted: no window is created |
| `src-tauri/build.rs` `rerun-if-changed=icons/icon.ico` | Re-embed the Windows resource | remove that line, keep `icon.png` and `tauri_build::build()` | | None |
| `src-tauri/icons/icon.ico`, `Square*Logo.png`, `StoreLogo.png` | Windows exe / Microsoft Store tiles | remove | Windows-only | Low |
| `src-tauri/icons/icon.png`, `32x32.png`, `128x128*.png`, `icon.icns` | Default window icon read by `generate_context!` on the host (and macOS dev dock icon) | **keep** until a commit proves they're unneeded with `cargo test` on the Mac and a CI APK build | `generate_context!` resolves an icon on the host target; a missing file can break the host compile | Medium: this is how removing desktop files breaks `cargo test` |
| `.gitignore` `build/`, `Thumbs.db` | build.ps1 output, Explorer thumbnails | optional: remove | Harmless | None |
| `src-tauri/gen/android/gradlew.bat` | Gradle wrapper for Windows hosts | **keep** | Part of the standard wrapper; nothing to do with the app's Windows support | None |

### Rust

| File / symbol | What it does | Action | Why | Risk |
|---|---|---|---|---|
| `selfupdate::apply`, `is_portable`, `finish_replace` | Download the new exe, swap or rename, relaunch with `--replaced` | remove | exe-only. Android links to the release page | Low |
| `selfupdate::LATEST_URL`, `is_newer`, the `latest.json` branch of `check()` | Windows release feed on Topher121 | remove; `check()` always uses `android_latest`. Keep `semver`, `parse_tag`, `android_is_newer` and their test | The only feed the fork publishes is the APK release | Low. `whats_new` already gets its notes through `check()`, i.e. through `android_latest` on the device, so nothing changes there |
| `check(client, url_override)` / `ADDONFORGE_UPDATE_URL` | Test override for the feed URL (ignored on Android today) | rewrite: make it override `ANDROID_RELEASES_API`, so the banner path can be tested against a local JSON on the Mac | Keeps a test hook without the exe feed | None |
| `Latest.size`, `Latest.filename` | exe size check, exe filename | `size` remove; `filename` keep (APK name, shown nowhere but harmless) | | None |
| `apply_self_update` command (lib.rs) + handler registration | IPC for the exe swap | remove (same commit as the UI branch that calls it) | | Low |
| `main.rs` `--replaced` path, `ADDONFORGE_JUST_UPDATED`, `windows_subsystem` attribute | Post-update cleanup on Windows | remove; `whats_new` drops the `just_updated` term (`existing = last != 0 \|\| has_history`) | Never set on Android | Low |
| `main.rs` itself | Desktop binary entry, `--cli` | keep (minimal) if the Mac preview or CLI is kept; Android enters through `run()` via `mobile_entry_point` | | Low |
| `wow::find_installs` (`#[cfg(not(target_os = "android"))]`), `ROOT_PATTERNS` | C:..Z: drive scan | remove. Make the Android scan the only `find_installs` with no `cfg`. On the Mac it finds nothing (`/storage` doesn't exist), which is correct | | Low |
| Android `find_installs` | Volume + Download scan | rewrite into `find_installs_in(volumes: &[PathBuf])` + a thin wrapper, and add a tempdir unit test (game root with `.build.info`, `Download/<root>/_classic_beta_`, a non-game folder) | Turns today's untested device-only code into host-tested code | Low; behaviour unchanged |
| `open_addons_folder`, `open_log` (lib.rs) | `explorer` / `explorer /select,` | remove both commands and their registrations, with the UI buttons in the same commit | Android already returns an error string and hides the buttons | Low |
| `state::AppState::dir()` non-Android branch (`dirs::config_dir`) | `%APPDATA%\AddonForge` / `~/Library/Application Support/AddonForge` | rewrite: host default = `std::env::temp_dir()/AddonForge-host` unless `ADDONFORGE_STATE_DIR` is set; **Android branch unchanged** | The host is dev-only now. A throwaway default removes the 2026-10-02 footgun (a test pointing a real profile at a temp folder) | Medium if the Android branch is touched: users would lose their settings. Don't touch it |
| `dirs` crate | config dir lookup | remove from `Cargo.toml` once state.rs stops using it | | None |
| `tauri-plugin-dialog` (Cargo.toml, `lib.rs` `.plugin(...)`, `dialog:default` capability) | Desktop folder / file pickers | remove, **after** the UI stops calling `T.dialog` (phase 6). Own commit: it changes the generated Gradle plugin includes | On Android, `touch.js` already replaces every `T.dialog` call with the Kotlin bridge (`pickFolder`, `pickFile`, `share`); the plugin's own Android pickers are never used | Medium: Android build plumbing; prove with a CI APK and a device launch |
| `tauri-plugin-opener` | Opens links | **keep** | `openUrl` drives every link and the "Get it" banner | High if removed |
| `Settings.platform` | Lets the UI branch on OS | remove **after** the UI no longer reads it (phase 7) | | High if done early (see ordering trap) |
| `Settings.svfix`, `wow::svfix_present` + its test | ForeverSVFix detection (a Windows exe tool) | remove. The owner already decided "never mentioned on Android" | Dead on Android | Low |
| `install::tests::removing_a_folder_never_follows_a_junction` | Proves `remove_dir_all` doesn't follow a junction into `WTF\SavedVariables` | **rewrite, don't remove:** `#[cfg(unix)]` version with `std::os::unix::fs::symlink`, same assertions. Runs on the Mac and covers Android (also unix) | Protection test. Must not be lost (studio rule) | Medium if simply deleted |
| `install::safety_scan` `MZ` check, `FORBIDDEN_EXT` (exe, dll, …) | Refuse zips with executables | **keep as is** | Still a protection: addon folders get copied between PCs and phones, and the rule is "addons are Lua, XML and media". Removing it weakens a protection | High if "cleaned up" |
| `install.rs` error text "a Windows executable in disguise" | | keep | Accurate description of an MZ file | None |
| `icons::resolve_texture` backslash handling | TOC `IconTexture` paths use `\` | keep | TOC files use backslashes on every platform | High if "fixed" |
| `remove_folders` rejecting `\` in names | Path safety | keep | Protection | High if relaxed |
| `diagnostics` `Exe:` line | `current_exe()` path | optional: drop the line (on Android it's the app_process path) | | None |
| Doc comments: `%APPDATA%` in state.rs, log.rs, icons.rs; `C:\...` in state.rs; `...\World of Warcraft\_classic_beta_` in wow.rs; `Interface\AddOns` in install.rs | | rewrite to the Android paths | | None |
| `cfg!(target_os = "android")` in `open_addons_folder`, `open_log`, `selfupdate::check` | Branches | disappear with the code above. Only `#[cfg(target_os = "android")]` in `state.rs` remains (needed: the host can't read `/proc/self/cmdline` as a package name) | | None |
| `--cli` (lib.rs `cli`, main.rs) | Headless JSON commands | **owner decision (Q3)**. Recommended: keep on the host, drop `selfupdate`; keep `selfcheck` (now exercises the Android feed), `resolve all` (catalogue health), `scan`/`check` against a copied AddOns folder | Only way to test sources/catalogue without a device | Low either way |
| `USER_AGENT`, `REPO_URL`, `SUPPORT_EMAIL` (lib.rs) | Point at Topher121 / the studio | **owner decision (Q1, Q2)** | Note: "Report on GitHub" files Android bugs on **upstream's** tracker today | Medium (user-facing) |
| `catalog::REMOTE_URL`, `STATS_URL` | Live catalogue + stats from Topher121 `main` | **owner decision (Q1)** | | Medium |
| `tauri.android.conf.json` identifier `io.github.grutamu.addonforge` | The installed app's id | **keep untouched** | Changing it is a different app to Android | Critical |
| `tauri.conf.json` `identifier: com.pocketforgestudios.addonforge` | Host-only (Android overrides it) | keep for this cleanup; branding question | | Low |

### UI

The approach: make the Thor layout **static** in `index.html` and
`style.css`, rather than built by `touch.js` moving desktop elements at
startup. Keep `touch.js` as a smaller **`ui/pad.js`** for the code that needs a
real device or a real DOM: gamepad mapping, spatial focus, Back order,
onboarding, resume re-check. Don't merge that code into `app.js`: anything above
the boot marker runs in the test's fake DOM, which has no
`MutationObserver`, `getComputedStyle`, `localStorage` or `document.body`.
Anything below it is never tested. A separate file keeps that boundary
explicit.

| File / symbol | What it does | Action | Why | Risk |
|---|---|---|---|---|
| `touch.js` gate (`if (!A && localStorage af-touch !== "1") return`) | Desktop opt-out | remove; the layout always applies. `A?.…` calls already no-op without the Android bridge, so the Mac preview still loads | | Low |
| `touch.js` DOM moves (detail, btn-check, show-ignored, last-checked, btn-update-all, gh-repo row, catalogue refresh, guide → pane) | Builds the Thor layout at runtime | rewrite as static markup in `index.html` (same ids, new places) | One layout, so nothing to move | Medium: layout regressions. Check with screenshots |
| `touch.js` `chips()` replacing `#installed-view` / `#browse-sort` `<select>` | Pad-friendly filters | rewrite: chip rows in HTML; `app.js` keeps `activeFilter` (already exists) and a new `sortMode` variable instead of reading `<select>.value`; delete the selects | | Low. `test-ui.js` drives `activeFilter` directly |
| `touch.js` `paneStats()` (`pane-total/updates/attention`, rail badge, chip counts, `pane-folder`) | Pane summary | rewrite into `renderInstalled()`, **reusing the ids `stat-total`, `stat-updates`, `stat-attention`** in the pane, so `test-ui.js` asserts on the same ids | The `.strip` that owned those ids goes away | Low if the ids are reused |
| `touch.js` guide rewrite (`Interface/AddOns`, launcher wording), packs hint, placeholders, `detail-close` "← Back" | Android copy | move the text into `index.html` | | None |
| `touch.js` `btn-check` override (`rescan(true)` then `checkUpdates()`) | Merged Rescan + Check | move into `app.js` as the only `btn-check` handler; remove `#btn-rescan` | | Low |
| `touch.js` featured/browse row click → `openDetail` | Rows open the pane | move into the `app.js` list handlers | | Low |
| `touch.js` gamepad / focus / Back / onboarding / resume / `afResult` picker replies / share | Thor input and native bridges | keep → `ui/pad.js`; move the pick / export / import / share-log handlers into `app.js` as the only handlers (they call `A?.…`) | Device code | Low (pure move) |
| `body.android` class, `.android-only` class, `body:not(.android) .android-only {display:none}` | Gating | remove once CSS is folded | | Medium, CSS specificity (below) |
| `.top .game-settings` button | "WoW: Forever · Installation settings" | remove | Hidden on Android | None |
| `.strip` (`stat-*`, `show-ignored`, `last-checked`, `#summary`) | Desktop count strip | remove the strip; the ids live on in the pane (`#summary` stays as the sr-only status) | | Low |
| `.thead.cols-installed`, `.thead.cols-browse` + `.cols-*` grid CSS | Table headers | remove | Hidden on Android | None |
| Row markup `.c-ver`, `.c-ver.available`, `.c-src`, `.flags`, `.c-cat` | Desktop columns | remove from the row templates (hidden on Android); keep the versions in the pane (`pkg-status`) | Smaller DOM per row | Low. Keep `note warn` and the escaped name, which `test-ui.js` asserts on |
| `<details class="more-actions">` "···" menu, its `toggle`/`scroll`/`Escape` handlers, the click-outside closer, `.more-actions`/`.action-menu` CSS | Desktop row menu | remove. Pin/Ignore/Remove/Reinstall live in `renderPackageActions()` | Hidden on Android | Medium: `test-ui.js` must be updated in the same commit (see phase 6) |
| `renderPackageActions()` early return `if (!isAndroid() …)` | Desktop guard | remove the guard | | Low |
| `#svfix-note`, `svfixReminder()` | ForeverSVFix reminder | remove (`svfixReminder(x)` → `x`) | | None |
| `.statusbar` (`#install-line`, `#footer-open-folder`) | Desktop footer | remove and the `loadSettings` lines that fill it | Hidden on Android | Low; the id check catches leftovers |
| `#btn-pick` desktop handler (`T.dialog.open`) | Desktop folder picker | remove the desktop handler; keep the button and the bridge handler ("Choose folder…") as the only one | | Low |
| `#btn-open-folder`, `#btn-open-log` | Explorer | remove. "Share the log" (in-flight work) covers getting the log off the device | | Low |
| `#btn-selfcheck` | "Check for an AddonForge update now" | remove (hidden on Android today). Optional: bring it back, since it works on Android | | None |
| `#btn-export` / `#btn-import` `T.dialog.save/open` handlers in `app.js` | Desktop file dialogs | remove; the share-sheet / file-picker handlers become the only ones and their labels ("Share my list…", "Import a list…") go into the HTML | | Low |
| Self-update banner: desktop text, "Update now"/"Skip this one", `apply_self_update` branch | exe swap UI | keep the Android text; "Get it" / "Not now" static in HTML; the button only opens `latest.url` | | Low |
| `isAndroid()` branches: `detail-note` text, `detail-close` focus, `detect()` message, `visibilitychange` re-detect | | collapse to the Android branch, delete `isAndroid` | | Low |
| `detect()` "Scanning drives…" | | rewrite: "Looking for your game folder…" | | None |
| `install-path` placeholder `C:\Program Files (x86)\…` | | `/storage/emulated/0/Download/WoW Forever/_classic_beta_` in HTML | | None |
| In-app copy: guide `Interface\AddOns`, "click an addon", Wago "Stored on this PC", `title=` tooltips that explain actions | | rewrite: `Interface/AddOns`, "select", "on this device"; the action explanations already show as visible text in the pane | UX plan §8 | None |
| `style.css` desktop rules overridden by touch.css, the `@media (max-width: 1000/860/700px)` table blocks, `body.android` lines at the end of style.css | | fold `touch.css` into `style.css` without the `body.android` prefix, delete what it overrides and the desktop media queries, drop the `<link>` to touch.css. Keep the Forge theme order: theme rules stay last | Note the Thor is ~833px wide, so `@media (max-width: 860px)` **applies on the device today** and touch.css wins only on specificity. Stripping the prefix lowers specificity, so delete those media queries in the same commit | Medium: visual regressions. Check with screenshots |
| `code`, `.diag` font `Consolas` | Windows font | `ui-monospace, monospace` | | None |

### Security gate (`scripts/security-check.mjs`)

| Check | Still meaningful on Android? | Action |
|---|---|---|
| CSP present and strict (`default-src 'self'`, no unsafe script, `connect-src` not open, `object-src 'none'`, `frame-ancestors 'none'`) | Yes: same Tauri CSP injection | keep |
| `dangerousDisableAssetCspModification`, `freezePrototype: false`, `devtools: true` | Yes | keep |
| Capability scan (`shell:`, `fs:`, `http:`, `process:`) | Yes | keep |
| No inline `<script>`, `style=""`, `on*=`, `eval` in `ui/` | Yes | **strengthen:** scan every `ui/*.html` and `ui/*.js` (today only index.html + app.js; touch.js / pad.js are unscanned) |
| `FORBIDDEN_FLAGS` in `additionalBrowserArgs` | Moot once the args are gone, but the loop costs nothing | keep; optionally fail if `additionalBrowserArgs` is present at all, since it does nothing on Android and could only be re-added to weaken something |
| Windows Defender / firewall / execution-policy regex over repo scripts | Moot once the `.ps1` files are gone | keep (cheap tripwire; studio rule: never remove a protection check) |
| *(new)* `MainActivity.kt` must not call `setWebContentsDebuggingEnabled(true)` | Yes: the CLAUDE.md "never commit it" rule, enforced instead of remembered | add |
| *(new)* `AndroidManifest.xml`: no `android:debuggable="true"`, no `android:usesCleartextTraffic="true"` | Yes | add |
| Header comment (`scripts/selfcheck.js`, `desktop/build.ps1`) and success message ("nothing touches Windows security settings") | | rewrite to say what is checked now |

Each change to the gate is only additive or a wording change. Show that each
new rule actually fires: add the forbidden thing in a scratch edit, run the
gate, see it fail, revert. Note the result in the commit message.

### Docs

| File / section | Action |
|---|---|
| `README.md` top: Windows download, Setup vs portable, SmartScreen, "single exe on WebView2", "Updates itself … swaps the exe", "another PC", Build (MSVC), icon section (`make-icons.ps1`, `icon.ico`), `docs/screenshot.png` (desktop) | rewrite around the Android section (already accurate): download = the fork's releases / Obtainium, a Thor screenshot (`adb exec-out screencap -p`), build = the existing Android steps. Keep the privacy bullets, Sources table, disclaimer, licence line and an explicit "fork of Topher121/AddonForge" credit |
| `README.md` "How it's built" | The upstream author describing themselves ("one person with a design background"). **Owner decision (Q2)**: keep it attributed to upstream, or write the fork's own |
| `README.md` Support (studio email, Ko-fi, "Made by Pocket Forge Studios") | **Owner decision (Q2)** |
| `CONTRIBUTING.md`: `Interface\AddOns`, Feature-my-addon link to Topher121, "Headless mode … addonforge.exe --cli" | slashes; link per Q1; CLI section per Q3 (e.g. `cargo run -- --cli resolve all`) |
| `CLAUDE.md` persona block (lines 1-8) | **Flag only, owner decision (Q5).** Don't edit as part of this cleanup |
| `CLAUDE.md` Project brief ("Windows addon manager"), Tech stack (WebView2, winget, MSVC, `addonforge.exe`, `npm run build`) | rewrite for Android + Mac host; fix the cargo PATH note |
| `CLAUDE.md` Code map (`%APPDATA%`, drive scan, "TABLE … ··· row menu"), What's-new (latest.json), v0.3.0 dev hooks (`dev-shot.ps1`), Forge theme (header/status bar) | rewrite the paths and layout description; keep the history short |
| `CLAUDE.md` Footprint (WebView2 measurements) | delete the measurements; **move the SECURITY paragraph** (studio rule, security-check, CSP) into its own section so it isn't deleted along with them |
| `CLAUDE.md` Installer, v0.2.0 self-update bullet, Headless mode, Studio hooks (build.ps1, PhoneApps, Topher121 PUBLIC, `Desktop\wow-addons`) | delete or rewrite (headless per Q3). Studio hooks are upstream's studio process. Keep a one-line pointer to upstream for history |
| `CLAUDE.md` CurseForge API section | **Flag**: that approval and the open support thread belong to Pocket Forge Studios, not the fork. The fork must not rely on it. Owner decision whether to keep the section as "upstream status, not ours" |
| `CLAUDE.md` Catalogue notices + ForeverSVFix, Launch + queue (r/wowforever, Mention Monitor, Office orders) | trim to what applies to the fork; the rest is upstream history |
| `CLAUDE.md` Android section | update: one layout (no "Windows is untouched", no `af-touch`), file names after the fold, `pad.js` |
| `docs/android-ux-plan.md` | Don't rewrite history. Add a status line saying principle 10 ("Don't fork the desktop UI … Windows is untouched") is superseded by this plan, and fix the stale "Get it opens Topher121/…/releases/latest" (the code uses the fork) |
| `.github/ISSUE_TEMPLATE/*.yml` | no Windows content found. Catalogue/feature forms depend on Q1 |
| `LICENSE` | **keep the existing notice verbatim** (MIT requires it). Optionally add a line for the fork's copyright (Q2) |

## 2. Phases

One small commit per step, on a branch (e.g. `cleanup/windows`), merged to
`android` when the phase is green. After **every** commit:

```
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
export ADDONFORGE_STATE_DIR=$(mktemp -d)       # never the real profile
(cd src-tauri && cargo test)
node scripts/test-ui.js
node scripts/security-check.mjs
```

On-device check, where a phase says so (Thor over `adb`, see CLAUDE.md):

```
npx tauri android build --apk --target aarch64 \
  --config '{"bundle":{"android":{"versionCode":<installed code>}}}'
adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release.apk
adb shell input gamepad keyevent KEYCODE_BUTTON_A   # etc.
adb exec-out screencap -p > shot-<tab>.png
```

Use the installed versionCode (not 3000, not above the next real release).
The **device run** referred to below: `scripts/android-smoke.sh <apk> <out>`
(walks every tab, fails on a crash, saves screenshots), then by hand the
`android-ux-plan.md` §11 path: gamepad only, folder already set → Update all →
open details → Pin, Unpin → Remove (Cancel) → Back through every layer to
exit. Then relaunch and check the game folder, pins and ignores survived
(proves the state path didn't move).

### Phase 0: reference set (no commit)
1. Run the three checks (done, see Baseline).
2. Build and install the current APK. Take Thor screenshots of: My addons
   (nothing selected / one selected), Find addons (featured strip, detail
   open), Starter packs (guide), Settings (top, Controller), onboarding (revoke
   file access with `appops set --uid … MANAGE_EXTERNAL_STORAGE deny`), the
   update banner if one is due, a confirm dialog. Keep them outside the repo.
   Later UI phases compare against these.

### Phase 1: CI and the protection test that would get lost
1. Port the junction test to `#[cfg(unix)]` with `std::os::unix::fs::symlink`
   (same three assertions). *Check:* `cargo test` on the Mac lists it
   (13 tests).
2. Rewrite `build.yml`: `macos-latest`, triggers `push: [android]` + PRs,
   same paths-ignore; steps `cargo test`, `node scripts/test-ui.js`,
   `node scripts/security-check.mjs`. macOS matches the owner's host and needs
   no webkit2gtk packages. Ubuntu with `libwebkit2gtk-4.1-dev` etc. is cheaper
   in Actions minutes but different from where the owner tests.
   *Check:* the PR shows the job green.
3. Fix `catalog.yml` triggers to `android` (still `main` today, so only PRs
   validate). Hold this if Q1 decides the catalogue stays upstream-only.

### Phase 2: harden the checks before anything is removed
1. `test-ui.js`: add a static check that every `$("…")` / `getElementById`
   id in `ui/*.js` exists in `index.html` (regex over the files; ids built
   at runtime are listed by hand). *Check:* passes now; deleting an id from
   a scratch copy makes it fail.
2. `security-check.mjs`: scan all `ui/*.js` + `ui/*.html`; add the
   MainActivity / manifest rules; rewrite comments. Keep the Windows checks
   for now. *Check:* passes; each new rule fails on a scratch edit.

### Phase 3: Windows build tooling
1. Move the `icon.svg → ui/icon.svg` copy into `make-android-icons.sh`.
   *Check:* run it; `git status` shows no change (icons identical).
2. Delete `build.ps1`, `scripts/make-icons.ps1`, `scripts/dev-shot.ps1`;
   change or remove `npm run build`. *Check:* three checks green;
   `git grep -n 'build.ps1\|make-icons.ps1\|dev-shot'` only hits docs that
   phase 10 rewrites.

### Phase 4: exe self-update
1. Rust: remove `apply`, `is_portable`, `finish_replace`, `LATEST_URL`,
   `is_newer`, `Latest.size`, the latest.json branch; `check()` always calls
   `android_latest`, and `url_override` now overrides the releases API URL.
   Remove `apply_self_update`, the `selfupdate` CLI command, `--replaced`,
   `ADDONFORGE_JUST_UPDATED`, `windows_subsystem`. UI: drop the non-Android
   banner text and the `apply_self_update` branch; "Get it" / "Not now" static.
   *Check:* three checks; `cargo run -- --cli selfcheck` on the Mac returns
   the fork's latest `android-v…` release with an `.apk` filename;
   `git grep -n apply_self_update` is empty.
   *Device:* install; Settings → Report a problem → Copy diagnostics: the log
   tail shows `self-update check: latest v…-b… (available: …)` with the
   right value. For the banner itself, run the Mac preview with
   `ADDONFORGE_UPDATE_URL` pointing at a local JSON with a higher version.

### Phase 5: discovery, state path, desktop-only commands
1. `wow.rs`: delete the drive scan and `ROOT_PATTERNS`; one `find_installs`;
   factor `find_installs_in(volumes)`; add the tempdir test.
   *Check:* `cargo test` +1 test. *Device:* Settings → Detect finds the game.
2. `state.rs`: host default dir → temp dir unless `ADDONFORGE_STATE_DIR`;
   **Android branch byte-for-byte unchanged**; drop `dirs`. Fix the
   `%APPDATA%` comments in state/log/icons/install.
   *Check:* `cargo test`; `git diff` on the `#[cfg(target_os = "android")]`
   block is empty. *Device:* install over the previous build, game folder and
   pins still there.
3. Remove `open_addons_folder`, `open_log` and, in the same commit, the
   `#btn-open-folder`, `#btn-open-log`, `#footer-open-folder` buttons and
   their handlers. *Check:* the id check passes; `git grep open_log` is empty.

### Phase 6: one UI path (behaviour change only, layout untouched)
1. Remove the `touch.js` gate and `af-touch`; delete every `isAndroid()`
   branch, keeping the Android side; remove `svfixReminder`, `#svfix-note`,
   the desktop `T.dialog` handlers (pick / export / import), `#btn-selfcheck`,
   the "···" row menu and its handlers, and the `renderPackageActions` guard.
   Rewrite `test-ui.js` in the same commit: drop the row-menu positioning
   assertions; add assertions that `renderPackageActions()` for a selected
   package renders Pin, Ignore, Remove (and Update when one is due), with the
   package name escaped. The remaining assertions (counts, filters, search,
   escaping, empty states, modal focus, detail cancellation) stay as they are.
   *Check:* three checks; the device run.

### Phase 7: Rust fields and the dialog plugin (after phase 6 is on `android`)
1. Remove `Settings.platform`, `Settings.svfix`, `wow::svfix_present` and its
   test. *Check:* `cargo test` (−1 test); `git grep -n "isAndroid\|platform"`
   in `ui/` is empty **before** this commit. *Device:* layout is still the
   Thor layout (catches the ordering trap).
2. Remove `tauri-plugin-dialog`: Cargo.toml, `.plugin(...)`, `dialog:default`.
   *Check:* `cargo test`, security gate (capability scan), and a **CI APK
   build**, not only a local one: CI starts clean, while a local
   `gen/android` can keep stale generated Gradle includes. *Device:* launch,
   links open (opener), Choose folder…, Share my list…, Import a list….

### Phase 8: static layout (markup + JS, CSS untouched)
1. Move the touch.js DOM moves into `index.html` (same ids); delete the header
   `.game-settings`, `.strip` (ids reused in the pane), both `.thead`s, the
   statusbar, `#btn-rescan`, the two `<select>`s (chips in HTML,
   `sortMode` in app.js); move `paneStats`, the guide copy, the `btn-check`
   handler and the row click handlers into `app.js`; rename what's left of
   `touch.js` to `ui/pad.js`. Trim the row templates to the columns the Thor
   shows. *Check:* three checks (`test-ui.js` passes unchanged: same ids);
   device run; screenshots match phase 0.

### Phase 9: fold the CSS
1. Merge `touch.css` into `style.css` without the `body.android` prefix;
   delete the desktop rules it overrides, the 1000/860/700px media queries,
   `.more-actions`/`.action-menu`, `.thead`/`.cols-*`, `.statusbar`,
   `.game-settings`, `.android-only`; drop the `<link>`; Forge theme rules
   stay last; Consolas → `ui-monospace`. *Check:* security gate (no inline
   styles); screenshots of every phase-0 state compared side by side; also
   check at the largest system font size (UX plan §11). The Mac preview at
   853×480 is the fast loop; the device is the sign-off.

### Phase 10: config and icons
1. `tauri.conf.json`: drop `additionalBrowserArgs`, NSIS, `bundle.windows`,
   `publisher`, `icon.ico`; window size → 853×480 or drop size keys (keep the
   `main` window entry). `build.rs`: drop the ico line. Delete `icon.ico`,
   `Square*Logo.png`, `StoreLogo.png`. *Check:* `cargo test` on the Mac (this
   is where a missing icon would break `generate_context!`), CI APK, security
   gate. Remove `icon.icns` / smaller PNGs only in a follow-up commit that
   proves the same three checks still pass.

### Phase 11: CLI (per Q3)
1. Trim or remove `--cli`. If kept: update the usage string, keep `resolve
   all`. *Check:* `cargo run -- --cli resolve all` on the Mac (with
   `ADDONFORGE_STATE_DIR`).

### Phase 12: docs
1. README, CONTRIBUTING, CLAUDE.md, the UX-plan status note, a Thor
   screenshot. *Check:* `git grep -n -i 'windows\|\.exe\|smartscreen\|webview2\|appdata\|powershell'`
   leaves only intended hits: upstream history, the MZ safety rule,
   `gradlew.bat`, the security gate's tripwires.

### Phase 13: upstream links and branding (only after the owner answers Q1/Q2)
Separate commits per decision (catalogue URL, stats, REPO_URL/issue links,
user agent, support email, Ko-fi, About text). *Check:* three checks; on the
device, Report on GitHub opens the intended tracker, About links open the
intended pages, the catalogue shows "latest from GitHub" in Settings.

### Final: release
`scripts/release-android.sh "…"` → CI builds, signs, publishes. On the Thor,
installing over the previous release must work (same signing key, higher
versionCode). Then: the What's-new note shows once, the banner is gone, the
device run passes. Obtainium picks up the release.

## 3. Open questions for the owner

**Q1. Catalogue, stats and upstream links.** Today the app fetches
`catalog/forever.json` and `catalog/stats.json` from
`raw.githubusercontent.com/Topher121/AddonForge/main`. The featured strip's
"ask to be featured" link, CONTRIBUTING, and "Report on GitHub" (`REPO_URL`)
all go to upstream too. **Android bug reports currently land on upstream's
tracker.**

- **A. Stay on upstream:** free catalogue updates. But the fork has no
  control: a schema change, rename or private repo upstream breaks the
  Android app, and authors would PR upstream for an app upstream doesn't
  ship.
- **B. Re-point everything to the fork** (`grutamu/AddonForge-android`,
  branch `android`): full control. The fork maintains the catalogue, runs
  `catalog.yml` on `android`, and either enables `stats.yml` (commit
  identity must change from "Pocket Forge Studios") or lets stats go stale.
- **C. Hybrid:** catalogue and stats from upstream, but issues/feature
  requests to the fork. Or a scheduled Action that copies upstream's
  catalogue into the fork, so the app always reads the fork. Fork control,
  upstream freshness, one more workflow to keep alive.

Also decide whether `stats.yml` stays disabled, gets enabled, or is deleted.
And note the bundled copy wins when it's newer by `updated`, so a
re-pointed remote must keep `updated` current.

**Q2. Branding and ownership.** The MIT notice ("Copyright (c) 2026 Pocket
Forge Studios") stays whatever happens, and upstream gets an explicit credit
line. To decide:
- Support email `PocketForgeStudios@proton.me` (lib.rs `SUPPORT_EMAIL`,
  About, README): keep, replace, or remove the Email button.
- Ko-fi "Support the dev" (About, README): it's the studio's; keep with
  attribution, replace, or remove.
- "AddonForge … by Pocket Forge Studios" (About), "Made by Pocket Forge
  Studios" (README), `Cargo.toml` `authors`, `package.json` description,
  `tauri.conf.json` host identifier `com.pocketforgestudios…`.
- The user agent `AddonForge/<ver> (+https://github.com/Topher121/AddonForge)`:
  sites see this. It should probably name the fork, but it's the owner's
  call.
- The app name "AddonForge" and the ember-and-anvil icon: upstream's brand.
  Keep (credited), or rename the fork.
- README "How it's built" is upstream's personal statement.
- A second copyright line for the fork's own changes.

**Q3. Headless `--cli`.** Keep on the Mac host (recommended, trimmed: no
`selfupdate`; `selfcheck`, `resolve all`, `scan`/`check` on a copied AddOns
folder), move `resolve all` into an `#[ignore]` network test, or remove it
with `main.rs` reduced to `run()`.

**Q4. Mac preview host.** Keep `npm run dev` (`tauri dev`) as a Thor-sized
853×480 window for layout work (recommended; it costs a window entry, the
host state-dir fallback and the PNG icons), or make the device the only way
to see the UI.

**Q5. Persona block** (`CLAUDE.md` lines 1-8, "you are AddonForge … studio-wide
convention"). It's the upstream owner's convention for their many chats. Keep,
change, or remove. This plan doesn't touch it.

**Q6. Upstream git remote.** Keep `upstream` for fetching only, with its push
URL set to something invalid (`git remote set-url --push upstream DISABLED`) so
nothing can be pushed by accident. Or remove it. Or ask GitHub to detach the
fork network. The upstream `v*-b*` tags in the fork stay either way.

**Q7 (small).** Bring back "Check for an AddonForge update now" (it works on
Android), or leave it out as today?

## 4. What not to touch

- **Signing:** `~/.android/addonforge-release.jks`, the Actions secrets
  `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`,
  `ANDROID_KEY_PASSWORD`, `src-tauri/gen/android/keystore.properties`
  (gitignored), and the signing / verify / cleanup steps in
  `android-release.yml`.
- **Versioning:** the `android-v<ver>-b<n>` tag scheme and its regex,
  `release-android.sh`'s counter, versionCode =
  `major*10^7 + minor*10^5 + build`, `ADDONFORGE_BUILD`, the tag-vs-
  `tauri.conf.json` version check. Don't delete or reuse upstream's `v*-b*`
  tags.
- **Identity:** `tauri.android.conf.json` (`io.github.grutamu.addonforge`,
  `minSdkVersion`), the Kotlin package and `MainActivity` / `Bridge`
  (`hasFileAccess`, `requestFileAccess`, `showKeyboard`, `exit`, plus the
  in-flight `pickFolder`, `pickFile`, `cacheDir`, `share`, `keepAwake`,
  `haptic`, `takeLaunchAction`, and `window.afPad` / `window.afResult` on the
  JS side), the proguard
  keep rule, the manifest permissions, `sensorLandscape`, theme colours.
- **The Android state path** in `state.rs` (`/data/data/<pkg>/files/AddonForge`):
  moving it loses every user's settings.
- **Protections:** zip `safety_scan` (extensions, `MZ`, symlinks, depth),
  `remove_folders` name checks, the 512 MB download cap, the CSP, the
  capability list (apart from removing `dialog:default`), and every existing
  security-gate rule. Under the studio rule the gate can only gain checks.
- `ui/` element ids that `test-ui.js` asserts on (`stat-total`, `stat-updates`,
  `stat-attention`, `btn-update-all`, `installed-list`, `installed-filter`,
  `show-ignored`, `modal-*`, `detail-*`) and the functions it calls
  (`renderInstalled`, `visiblePackages`, `pickFeatured`, `confirmModal`,
  `openDetail`, `closeDetail`, `entryUrl`).
- `tauri-plugin-opener`, `withGlobalTauri`, `crate-type` in Cargo.toml (cdylib
  for Android, rlib for tests/CLI), the release profile.
- `catalog/forever.json` / `stats.json` contents, `validate-catalog.js`,
  `build-stats.js`: data, not platform code. Their URLs wait for Q1.
- `src-tauri/gen/android/gradlew(.bat)` and the Gradle wrapper.
- `LICENSE` text.
