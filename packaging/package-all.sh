#!/usr/bin/env bash
# Master cross-platform packaging script for CoralCraft:
#   1. macOS Intel (x86_64-apple-darwin)         -> .pkg + .dmg + .app.zip
#   2. macOS Apple Silicon (aarch64-apple-darwin)-> .pkg + .dmg + .app.zip
#   3. macOS Universal2 (x86_64 + aarch64)       -> .pkg + .dmg + .app.zip
#   4. Windows 64-bit (x64 & arm64)              -> -setup.exe + -portable.zip
#   5. Linux GUI (x86_64-unknown-linux-gnu)      -> .AppImage + .flatpak + .deb + .rpm + .tar.gz
#   6. FreeBSD (x86_64)                          -> .tar.gz
#   7. Web (WebAssembly / wasm32-unknown-unknown)-> .zip + .tar.gz
#   8. SHA256SUMS.txt manifest

set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
. "$HERE/env.sh"

chmod +x \
  "$HERE/macos/package.sh" \
  "$HERE/linux/package.sh" \
  "$HERE/windows/package.sh" \
  "$HERE/freebsd/package.sh" \
  "$HERE/web/package.sh"

echo "================================================================"
echo " Building CoralCraft v$VERSION Packages for All Platforms"
echo "================================================================"

"$HERE/linux/package.sh" "$@"
"$HERE/windows/package.sh" "$@"
"$HERE/macos/package.sh" --arch both "$@"
"$HERE/freebsd/package.sh" "$@"
"$HERE/web/package.sh" "$@"

(
  cd "$DIST"
  rm -f SHA256SUMS.txt
  sha256sum coralcraft-"$VERSION"-* >SHA256SUMS.txt
)

echo ""
echo "================================================================"
echo " All Release Packages Built Successfully in $DIST:"
echo "================================================================"
ls -lh "$DIST"
echo ""
cat "$DIST/SHA256SUMS.txt"
