# AddonForge on Android (AYN Thor): UX research and plan

Status: P0 built 2026-10-14 (ui/touch.css, ui/touch.js, MainActivity) and
tested on a Thor; see the Android section of CLAUDE.md. Later on 2026-10-14
the Windows code was removed: this layout is now the only one (static in
ui/index.html, styles in ui/style.css, input in ui/pad.js), so principle 10
below ("Don't fork the desktop UI … Windows is untouched") no longer applies. Deviations from
this plan: Starter packs and Settings keep a single list (the pane shows the
guide / nothing), the "Show ignored" switch and Check live in the pane home
rather than a ⋮ menu, and START runs Check for updates. P1/P2 are open.

**Target: the AYN Thor, landscape, top screen.** No portrait layout.

## 1. The device and the user

- **AYN Thor**: Android 13, Snapdragon 8 Gen 2. Top screen 6" OLED
  1920x1080 (16:9). Bottom screen 3.92" touch OLED 1240x1080 (31:27).
  Built-in gamepad with D-pad, ABXY, sticks and shoulders.
- In CSS pixels the top screen is roughly **730-960 wide x 410-540 tall**,
  depending on the Android display-size setting. Measure
  `innerWidth`/`innerHeight` on the device once and design for the default.
  The plan below assumes ~850x480.
- WoW runs through the AYN Thor WoW Launcher (a fork of jaredgei's *WoW
  Forever for Android*). The game folder is wherever the user copied `Data`
  and `.build.info`; `wow::find_installs` already looks there.

What follows from that:

- **Height is the scarce axis.** About 480px tall. The current desktop chrome
  stacks header 36 + tabs 26 + toolbar ~56 + strip ~30 + table header ~34 +
  status bar ~32, about 210px. That's almost half the screen before the first
  addon row.
- **Hands are on the controller.** The thumbs rest on the D-pad and buttons
  and reach the screen only when needed. Touch and gamepad both have to
  work, and neither may depend on hover.
- **Sessions are short and happen right before playing.** Open, see
  "3 updates", Update all, back to the launcher, Play. Build the UI around
  that one-minute loop. Catalogue browsing comes second.
- **Typing is expensive.** The on-screen keyboard covers most of a landscape
  screen. Paths and `owner/repo` strings are the clunkiest thing we could ask
  for.

## 2. What feels clunky in the current UI on the Thor

Specific to this codebase, roughly worst first:

1. **Back exits the app.** Tauri's default Android back is "webview history
   back, else finish". We're a single page with no history, so back (or B on
   the pad) with the details modal or a row menu open closes the app.
2. **Vertical chrome** eats nearly half the height (§1). The header, tab
   strip and status bar are all horizontal bands.
3. **The installed list is a 6-column table.** At these widths the existing
   media queries squeeze it to 64px columns, and below 700px it scrolls
   sideways. Small, dense, and hard to hit.
4. **The row menu (`<details>` + fixed `.action-menu`)** is a desktop
   popover: a ~24px "···" target, positioned by JS, closes on scroll.
5. **Explanations live in `title=` tooltips** (Pin, Ignore, Remove, Rescan,
   Installation settings). There's no hover on touch or gamepad, so they
   disappear.
6. **Type is 11-13px**, with 11px secondary lines, on a ~6" screen held at
   arm's length.
7. **Targets are 24-36px.** Android's minimum is 48dp. The current
   `body.android` rule (36px) helps but isn't enough.
8. **There's no gamepad story.** Focus rings are 1px, there's no list
   navigation, and some actions exist only behind the hover/tooltip/menu
   combination.
9. **First run fires the "All files access" system screen with no
   explanation** (`MainActivity.onCreate`). The user comes back to an app
   that drops them into a text field asking for a path, with no picker.
10. **Desktop vocabulary**: Rescan, Open AddOns folder, Browse…,
    `Interface\AddOns` with backslashes, a status bar, a ForeverSVFix note
    (a Windows exe), and the guide's "type /reload in chat".
11. **The theme has no window background** (`MaterialComponents.DayNight`),
    so there's a white flash before the dark webview paints. The manifest
    still carries the template's `LEANBACK_LAUNCHER` (Android TV).

## 3. Design principles

1. **One landscape layout, built for ~850x480.** Navigation on the left
   edge, content in the middle, details on the right. No horizontal bands
   except one compact toolbar.
2. **One primary action per screen, under the right thumb.** My addons →
   Update all. Details → Install / Update. Starter pack → Install pack. Put
   it at the bottom right, never in a top toolbar.
3. **Rows are things you select, not table cells.** Selecting a row (tap, or
   D-pad + A) shows it in the details pane. Only Update/Install is ever inline.
4. **Every tooltip becomes visible text,** either in the details pane or in
   the action list.
5. **Reversible means Undo, irreversible means Confirm.** Pin / Ignore use a
   snackbar with Undo. Remove keeps the confirm dialog that lists folders.
6. **Back (system back and B) always closes one layer.**
7. **Every action is reachable with the D-pad and A,** and every tap target
   is ≥ 48px. Nothing needs hover or long-press only.
8. **No typing on the happy path.** Detect the folder, use chips and lists,
   use the system folder picker.
9. **Keep the Forge identity**: brass/charcoal palette, serif wordmark,
   single ember accent. Borrow Android *structure* (rail, list-detail,
   snackbar), not Material *looks*.
10. **Don't fork the desktop UI.** Put Thor rules in a new `ui/touch.css`
    loaded after `style.css`, scoped by `body.android`, and keep every
    element id so `scripts/test-ui.js` stays green. Windows is untouched.

## 4. Layout

```
 ~850 x 480
┌─────┬────────────────────────────────┬───────────────────────────┐
│  ▲  │ (All 24) (Updates 3) (Needs 1) │ [icon] BigWigs            │
│     │                          🔍  ⋮ │ GitHub · 48k · 3 days ago │
│ ⬇ 3 ├────────────────────────────────┤ v425.5 → v426.0           │
│ My  │▌[ic] BigWigs            Update │ [screenshot]              │
│     │ [ic] Questie      ✓ Up to date │ README excerpt…           │
│ 🔍  │ [ic] Details         Link only │                           │
│ Find│ [ic] SVShim     ⚠ No longer…   │ Pin · Ignore · Remove     │
│ ▦   │ [ic] Plater       ✓ Up to date │                           │
│Packs│            …                   │ ┌───────────────────────┐ │
│ ⚙   │                                │ │ Update all (3)    (X) │ │
│     │                                │ └───────────────────────┘ │
└─────┴────────────────────────────────┴───────────────────────────┘
 rail     list ~52%, 64px rows             details pane ~40%
 72px
```

- **Navigation rail** (72px) on the left: the ember mark at the top, then My
  addons (with an update-count badge), Find, Packs and Settings, icons with a
  short label. It replaces the 36px header, the 26px tab strip, *and* the
  "WoW: Forever · Installation settings" link.
- **Toolbar** (one 56px row): filter chips with counts on the left, search
  and overflow (⋮) icons on the right. Search expands in place over the chips.
- **List**: 64px rows, 40px icons, name 16px, one supporting line 14px.
  Selected row gets an ember bar on the left.
- **Details pane** (always visible): the selected addon. With nothing
  selected, it shows the summary the `.strip` and status bar show today
  ("24 addons · 3 updates", "Checked at 14:02", game folder status), and the
  primary button. That means the status bar can go.
- **Primary button** at the bottom of the details pane (bottom-right of the
  screen, under the right thumb), with an optional pad-button hint.
- **Snackbar** bottom-centre over the list, 14px, 4s, optional Undo.

Every tab uses the same three-column shell. Only the list and the pane
contents change.

## 5. Screen by screen

### First run (new; replaces the cold permission jump)

Full-width cards, no rail yet:

1. **Welcome + permission**: one sentence about what AddonForge does, why it
   needs "All files access" (your addons live in the game folder, which
   another app created), and a **Allow file access** button. The button fires
   the settings intent; move it out of `MainActivity.onCreate` into a command.
2. **Finding your game**: run `find_installs` when the user comes back (the
   `visibilitychange` hook is already there). One result: a large card with
   **Use this**. Several: a list to pick from. None: **Choose folder** opens
   the system folder picker (`ACTION_OPEN_DOCUMENT_TREE`). Convert the tree
   URI to a path (`primary:WoW` → `/storage/emulated/0/WoW`, `XXXX-XXXX:WoW` →
   `/storage/XXXX-XXXX/WoW`). Keep a small "Type a path instead" link.
3. **Done**: nothing installed → Starter packs with the guide open.
   Otherwise → My addons, check for updates.

### My addons

- The filter `<select>` becomes **chips with counts**: All · Updates · Needs
  attention. "Show ignored" moves to ⋮.
- Rescan and Check for updates merge into one action, in ⋮ and on a
  shoulder button (see §6). **Also auto-check on resume** if the last check is
  more than 10 minutes old: that covers the launcher-to-AddonForge round trip.
- Row: icon, name, supporting line (`v425.5 → v426.0` / `✓ Up to date ·
  GitHub` / amber notice / `Needs BigWigs_Core`), and a trailing **Update**
  only when `status === "update"` and not pinned (same rule as now).
- **Selecting a row fills the details pane.** The pane holds what the "···"
  menu holds today, as visible buttons with their explanations:
  ```
  Reinstall
  Pin version     Hold at v425.5; Update all skips it
  Ignore          Hide it and stop checking for updates
  Remove          Deletes its folders; settings in WTF are kept   (red)
  ```
  The "···" `<details>` menu is hidden on Android; the pane replaces it.
- **Update all** is the pane's primary button whenever updates > 0, even
  while a single addon is selected. Show it pinned at the bottom of the pane
  above the selected addon's own Update. It runs sequentially with per-row
  progress and "Updating 2 of 3…".
- Missing dependencies show in the pane with an **Install** button each.

### Find addons

- **Featured**: a single row of up to four cards above the list. They scroll
  away with the list so they don't cost height permanently.
- Toolbar: search plus a sort chip ("Most downloaded ▾"). Refresh catalogue
  moves to ⋮.
- Rows: icon, name, `category · 48k · updated 3 days ago`, trailing
  **Install** / `Installed ✓` / `Link only` / `Needs Wago key`.
- The details pane shows the README excerpt, screenshot and Install, which is
  what the modal shows today. It no longer covers the list.
- **Install from GitHub** moves to ⋮ → "Add from a GitHub link…" (a dialog
  with a **Paste** button). Later, an Android share target ("Share →
  AddonForge" from the browser) means nobody types a repo name.

### Starter packs

- The list shows the packs (name + one-line description + "4 of 6
  installed"). The pane shows the selected pack's addons as 48px rows with
  icons, plus **Install pack** at the bottom.
- The "New to addons?" guide is the first list entry, and its text shows in
  the pane.

### Settings

- The list holds the categories: Game folder, Updates, **Controller**, Wago
  key, Addon list, Report a problem, About. The pane holds the selected
  category's controls. That's the same shell again, and the desktop
  label-column grid maps onto it.
- Controller: "Button layout: Nintendo / Xbox" with a picture of the four
  face buttons showing which one confirms. Store it in `state.json`.
- Switches instead of checkboxes. The Wago key gets a **Paste** button.
- Export/import: the fixed `Download/addonforge-list.json` is fine for now;
  show the path in the pane, not only in a toast. Later: system share sheet /
  file picker (needs native `content://` handling).
- Keep hidden on Android, as the other agent has started doing: Browse…,
  Open AddOns folder, Open the log, the self-update check, the status bar.

### Dialogs

- Confirm dialog (Remove, Not-for-Forever, Import plan): centred, max 480px
  wide, 48px buttons, destructive button red, **B / back = Cancel**, and focus
  starts on the safe button.

## 6. Gamepad (required, P0)

The Thor's pad is its main input. Every screen must be fully usable without
touching it.

**Default layout is Nintendo style**, matching the labels printed on the
Thor: **A (right) confirms, B (bottom) goes back.** Settings → Controller
has a "Button layout" switch, *Nintendo (A right confirms)* / *Xbox (A bottom
confirms)*. It swaps A↔B and X↔Y in the mapping and in the on-screen glyphs.

| Button (Nintendo default) | Position | Action |
|---|---|---|
| D-pad up/down | | Move through the list (roving focus; the details pane follows) |
| D-pad left/right | | Move between rail ↔ list ↔ pane |
| A | right | Activate the focused item (Update / Install / open) |
| B | bottom | Back (close dialog → leave pane → My addons → exit) |
| X | top | The screen's primary action (Update all / Install pack) |
| Y | left | Search |
| L / R | shoulders | Previous / next tab |
| + (Start) | | ⋮ menu (Check for updates, Show ignored…) |

- **Do this first on the device**: a hidden debug page that logs `key`,
  `code` and `keyCode` for every button (keydown, plus
  `navigator.getGamepads()`). The Thor's firmware may report buttons by
  *label* or by *position*, and may already swap them in its own controller
  settings. The mapping above is about position, so we need to know which
  Android keycode each physical button sends.
- **Watch the system fallbacks.** Android's default key map turns an
  *unconsumed* `BUTTON_B` into Back and `BUTTON_A` into D-pad centre. With a
  Nintendo layout that can make "confirm" also fire Back. Every pad key we
  handle must be consumed (`preventDefault`), and we need to check that the
  WebView really stops the fallback. If it doesn't, forward keys from
  `MainActivity.dispatchKeyEvent` instead.
- D-pad arrives as arrow keys; the rest may need the Gamepad API or a
  native key forwarder. Either way the table above is the contract.
- **Focus ring**: 2-3px ember outline + glow on `:focus-visible`, plus the
  selected-row bar. It has to be visible from arm's length. Focus is never
  lost: after a re-render (e.g. an update finishing), it goes back to the
  same row by key.
- **Glyph hints** on the primary buttons and dialogs ("Update all Ⓧ",
  "Ⓑ Cancel  Ⓐ Remove"), drawn from the chosen layout. Show them once a pad
  key has been pressed and hide them after a touch.
- Every action is a real `<button>` (mostly true today). Nothing exists only
  behind long-press or hover.

## 7. Android plumbing the UX depends on

| Need | How | Notes |
|---|---|---|
| Back | `window.__TAURI__.app.onBackButtonPress(...)` (API 2.9+, project is on tauri 2.11.6). Check whether `core:default` grants `core:app:allow-register-listener`; add it to the capability if not. | Order: dialog → search → pane focus → My addons → exit. |
| System bars | **Immersive by default**: hide the status and navigation bars (`WindowInsetsControllerCompat.hide(systemBars())`, behaviour `BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE`), like the launcher does. That gives back ~24-48px. Still add `viewport-fit=cover` + `env(safe-area-inset-*)` padding so a swiped-in bar never covers the rail. | Tauri enables edge-to-edge regardless of Android version. WebView passes insets from M136 (fullscreen) / M144 (all); 0 on older WebViews. Re-apply hide in `onWindowFocusChanged`, or the bars come back after the permission screen / keyboard. |
| Keyboard | Keep it out of the way: search fields only. Don't re-render on `resize`/`visualViewport` changes. | Chromium warns that re-rendering on IME resize can blur the input and dismiss the keyboard. Landscape IME covers most of the screen, which is fine for a search box. |
| No white flash | `android:windowBackground` and system bar colours `#191919` in `themes.xml`. | Cheap and very noticeable. |
| Orientation | `android:screenOrientation="sensorLandscape"` on the activity. | Stops an accidental portrait layout. |
| Leanback | Remove `LEANBACK_LAUNCHER` and the `leanback` uses-feature. | |
| Folder picker | Small Kotlin plugin: `ACTION_OPEN_DOCUMENT_TREE` → tree URI → path. | The same plugin could later do share-sheet export / picker import. |
| Font scale | Size text in `rem`; test at the largest system font size. | WebView `textZoom` follows the system font scale by default. |

## 8. Copy changes for Android

- Use `Interface/AddOns` (forward slashes); say "game folder", not "drive".
- Guide → "Turning them on": the AddOns button is on the character-select
  screen; to load addons mid-session, use the launcher pad's keyboard to send
  `/reload` (the Thor launcher's keyboard has `/` on its main page).
- ForeverSVFix: **hidden on Android**: no amber note, no "run Apply /
  Refresh" suffix on install/update toasts (`settings.svfix` forced false or
  ignored when `isAndroid()`).
- Drop "Rescan" and "Open AddOns folder" (or replace the latter with "Copy
  folder path").
- Say "tap" or neutral verbs ("open", "choose"), never "click".

## 9. Phasing

**P0: before anyone outside the studio gets an APK**
1. Back handling, landscape lock, immersive mode, dark window background,
   drop leanback.
2. `ui/touch.css`: the rail + list + details-pane shell, 48px targets,
   16/14px type, list rows replacing the tables, no app status bar.
3. Details pane replaces both the details modal and the "···" row menu.
4. Gamepad: key-logging check on the device, then the full §6 mapping
   (Nintendo default + Xbox option), focus ring, glyph hints.
5. Onboarding: permission explainer → auto-detect → pick.
6. Android copy pass (§8), ForeverSVFix hidden.
7. Update banner (below).

**Update banner (Android)**: the exe self-update stays off. Instead, on
launch, compare against the latest GitHub release. Either add an `android`
block (`version`, `build`, `apk` name) to `latest.json`, or look for an
`.apk` asset on `releases/latest`. If newer, show **one line at the top of
the list column**: "AddonForge v0.4.0 is out · **Get it** · Not now".
**Get it** opens the fork's release page (`grutamu/AddonForge-android`)
in the browser. It must be reachable by D-pad, and "Not now" remembers the
build like "Skip this one" does today. No in-app installer.

**P1: makes it feel native**
- Filter chips, Update all in the pane with progress, auto-check on resume.
- Snackbar with Undo.
- System folder picker (Kotlin plugin).

**P2: nice to have**
- Share target for GitHub links; share-sheet export / picker import.
- Running on the Thor's **bottom screen** (1240x1080, nearly square) while
  the game is on top. The three-column shell would need to drop the pane
  into a dialog under ~700px. Only worth it if people actually use it that
  way.

## 10. Decided (owner, 2026-10-14)

1. **Gamepad navigation is required** (P0). The default layout is Nintendo
   style (A right = confirm, B bottom = back), as printed on the hardware,
   with a Settings option to switch to Xbox style.
2. **ForeverSVFix**: hidden on Android.
3. **No Play button** into the launcher.
4. **App updates**: a banner linking to the GitHub release page; no in-app
   installer.
5. **Android status bar**: hidden (immersive) to gain height; the user can
   swipe it in.

## 11. How to check it

- Desktop Chrome device mode at **853x480** (and 731x411, 960x540 for the
  display-size extremes), with touch emulation.
- On the Thor: the largest system font size; a gamepad-only run (folder
  setup → Update all → details → Remove → back to exit); the keyboard open
  in search; back from every layer.
