#!/usr/bin/env bash
# Package an already assembled native app alongside its release CLI on macOS.
set -euo pipefail

if [[ $# != 4 ]]; then
  echo "Usage: $0 path/to/anycube path/to/anycube.app arm64|x86_64 output-directory" >&2
  exit 2
fi
cli=$1
bundle=$2
arch=$3
case "$arch" in arm64|x86_64) ;; *) echo "Unsupported architecture: $arch" >&2; exit 2 ;; esac
if [[ ${cli##*/} != anycube || ! -f "$cli" || ! -x "$cli" || ${bundle##*/} != anycube.app || ! -d "$bundle" ]]; then
  echo "Expected an executable named anycube and an assembled anycube.app directory" >&2
  exit 1
fi
cli_dir=$(cd "$(dirname "$cli")" && pwd -P)
bundle_dir=$(cd "$bundle" && pwd -P)
plutil -lint "$bundle_dir/Contents/Info.plist"
codesign --verify --deep --strict "$bundle_dir"
for binary in "$cli_dir/anycube" "$bundle_dir/Contents/MacOS/anycube" "$bundle_dir/Contents/MacOS/anycube-server"; do
  lipo -verify_arch "$arch" "$binary"
done

mkdir -p "$4"
out=$(cd "$4" && pwd -P)
if [[ "$out/" == "$bundle_dir/"* ]]; then
  echo "Choose an output directory outside the bundle" >&2
  exit 1
fi
archive="$out/anycube-macos-$arch-with-app.tar.gz"
tmp=$(mktemp "$out/.anycube-archive.XXXXXX")
trap 'rm -f "$tmp"' EXIT
tar -czf "$tmp" -C "$cli_dir" anycube -C "$(dirname "$bundle_dir")" anycube.app
mv -f "$tmp" "$archive"
echo "$archive"
