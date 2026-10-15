#!/usr/bin/env bash
# Tag the next Android release. The "Android release" workflow then builds,
# signs and publishes AddonForge-android-v<version>-b<build>.apk.
#
#   scripts/release-android.sh "What changed (shown on the release page)"
#
# Android builds have their own counter (android-v*-b*), separate from the
# Windows tags (v*-b*) that came with the fork.
set -euo pipefail
cd "$(dirname "$0")/.."

notes=${1:?usage: scripts/release-android.sh "release notes"}
[ -z "$(git status --porcelain)" ] || { echo "Commit or stash your changes first." >&2; exit 1; }

ver=$(node -p "require('./src-tauri/tauri.conf.json').version")
git fetch -q --tags origin
last=$(git tag -l 'android-v*-b*' | sed 's/.*-b//' | sort -n | tail -1)
tag="android-v$ver-b$(( ${last:-0} + 1 ))"

git push -q origin HEAD
git tag -a "$tag" -m "$notes"
git push -q origin "$tag"
echo "Pushed $tag. Follow the build with: gh run watch -R grutamu/AddonForge-android"
