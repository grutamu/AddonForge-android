#!/usr/bin/env bash
# Usage: scripts/release-android.sh "release notes"
# No zero padding in the tag: it doubles as the semver versionName.
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
