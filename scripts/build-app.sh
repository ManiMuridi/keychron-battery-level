#!/bin/bash
set -euo pipefail

project_dir="$(cd "$(dirname "$0")/.." && pwd)"
cd "$project_dir"
if ! command -v cargo >/dev/null 2>&1; then
    if [ -f "$HOME/.cargo/env" ]; then
        . "$HOME/.cargo/env"
    else
        echo 'Install Rust from https://rustup.rs before building.' >&2
        exit 1
    fi
fi

export MACOSX_DEPLOYMENT_TARGET=13.0
case "${1:-}" in
    '')
        cargo build --release --locked
        ;;
    --universal)
        cargo build --release --locked --target aarch64-apple-darwin
        cargo build --release --locked --target x86_64-apple-darwin
        ;;
    *)
        echo "Usage: $0 [--universal]" >&2
        exit 2
        ;;
esac
app_dir="$project_dir/dist/Keychron Battery.app"
mkdir -p "$app_dir/Contents/MacOS" "$app_dir/Contents/Resources"
if [ "${1:-}" = --universal ]; then
    lipo -create target/aarch64-apple-darwin/release/keychron-battery \
        target/x86_64-apple-darwin/release/keychron-battery \
        -output "$app_dir/Contents/MacOS/keychron-battery"
else
    cp target/release/keychron-battery "$app_dir/Contents/MacOS/keychron-battery"
fi
cp resources/Info.plist "$app_dir/Contents/Info.plist"
app_version="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["version"])')"
plutil -replace CFBundleShortVersionString -string "$app_version" "$app_dir/Contents/Info.plist"
plutil -replace CFBundleVersion -string "$app_version" "$app_dir/Contents/Info.plist"
cp LICENSE THIRD_PARTY_NOTICES.md "$app_dir/Contents/Resources/"
plutil -lint "$app_dir/Contents/Info.plist"
codesign --force --sign - --identifier dev.mani.keychron-battery "$app_dir"
codesign --verify --strict "$app_dir"
echo "Built: $app_dir"
