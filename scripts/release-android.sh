#!/usr/bin/env bash
# Tag today's Android release. The "Android release" workflow then builds,
# signs and publishes AddonForge-android-<name>.apk.
#
#   scripts/release-android.sh "What changed (shown on the release page)"
#
# Releases are named by date, not x.y.z: android-2026.10.14, then
# android-2026.10.14-2, -3 … for more releases the same day. The name has no
# zero padding because it is also the Android versionName (must be semver).
set -euo pipefail
cd "$(dirname "$0")/.."

notes=${1:?usage: scripts/release-android.sh "release notes"}
[ -z "$(git status --porcelain)" ] || { echo "Commit or stash your changes first." >&2; exit 1; }

git fetch -q --tags origin
today=$(date +%Y.%m.%d | awk -F. '{ printf "%d.%d.%d", $1, $2, $3 }')
tag="android-$today"
n=1
while git rev-parse -q --verify "refs/tags/$tag" >/dev/null; do
  n=$((n + 1))
  tag="android-$today-$n"
done
[ "$n" -le 99 ] || { echo "99 releases in one day is enough." >&2; exit 1; }

git push -q origin HEAD
git tag -a "$tag" -m "$notes"
git push -q origin "$tag"
echo "Pushed $tag. Follow the build with: gh run watch -R grutamu/AddonForge-android"
