#!/usr/bin/env bash
# Usage: scripts/android-smoke.sh [path/to/app.apk] [out-dir]
# Never presses A on an addon action, so addons are left untouched.
set -euo pipefail
apk=${1:-}
out=${2:-${TMPDIR:-/tmp}/addonforge-smoke}
pkg=io.github.grutamu.addonforge
mkdir -p "$out"

adb get-state >/dev/null || { echo "No device attached." >&2; exit 1; }
if [ -n "$apk" ]; then adb install -r "$apk" >/dev/null && echo "installed $apk"; fi

# Top screen.
display=$(adb shell dumpsys SurfaceFlinger --display-id 2>/dev/null | sed -n 's/^Display \([0-9]*\).*/\1/p' | head -1)
shot() { adb exec-out screencap -p ${display:+-d "$display"} > "$out/$1.png"; }
pad() { adb shell input gamepad keyevent "KEYCODE_$1"; sleep "${2:-0.5}"; }
alive() { adb shell pidof "$pkg" >/dev/null || { echo "FAIL: AddonForge is not running after $1" >&2; adb logcat -d -b crash | tail -30 >&2; exit 1; }; }

adb shell input keyevent KEYCODE_WAKEUP
adb shell am force-stop "$pkg"
adb logcat -c
adb shell monkey -p "$pkg" -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1
sleep 7
alive "launch"
shot 1-launch

pad DPAD_DOWN; pad DPAD_DOWN; pad BUTTON_A 1.5   # open the second addon in the pane
alive "opening an addon"; shot 2-details
pad BUTTON_B                                      # close it
for tab in 3-find 4-settings; do pad BUTTON_R1 1.5; alive "$tab"; shot "$tab"; done
pad BUTTON_R1 1; pad BUTTON_Y 1; pad BUTTON_B; pad BUTTON_B   # back to My addons, search, back out
alive "search and back"; shot 5-back-home

if adb logcat -d -b crash | grep -q "$pkg"; then
  echo "FAIL: crash logged" >&2; adb logcat -d -b crash | tail -30 >&2; exit 1
fi
echo "OK: launched, walked every tab with the pad, no crash. Screenshots in $out"
