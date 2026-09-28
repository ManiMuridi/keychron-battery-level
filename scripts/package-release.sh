#!/bin/bash
set -euo pipefail

project_dir="$(cd "$(dirname "$0")/.." && pwd)"
cd "$project_dir"
if ! command -v cargo >/dev/null 2>&1 && [ -f "$HOME/.cargo/env" ]; then
    . "$HOME/.cargo/env"
fi
./scripts/build-app.sh --universal
app_version="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["version"])')"
asset_name="Keychron-Battery-v${app_version}-universal"
staging_dir="$(mktemp -d "${TMPDIR:-/tmp}/keychron-release.XXXXXX")"
trap 'rm -rf "$staging_dir"' EXIT
ditto 'dist/Keychron Battery.app' "$staging_dir/Keychron Battery.app"
ln -s /Applications "$staging_dir/Applications"
hdiutil create -volname 'Keychron Battery' -srcfolder "$staging_dir" \
    -ov -format UDZO "dist/$asset_name.dmg"
ditto -c -k --sequesterRsrc --keepParent 'dist/Keychron Battery.app' "dist/$asset_name.zip"
cd dist
shasum -a 256 "$asset_name.dmg" "$asset_name.zip" > SHA256SUMS.txt
echo "Release files are in $project_dir/dist"
