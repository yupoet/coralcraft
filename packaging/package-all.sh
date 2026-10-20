#!/usr/bin/env bash
# Build the release packages that this host can produce natively:
#   macOS host  -> packaging/macos/package.sh (universal .app.zip + CLI zip)
#   Linux host  -> packaging/linux/package.sh (.deb + .tar.gz, glibc 2.17 baseline)
#                  packaging/windows/package.sh (portable .zip, cross-built with cargo-zigbuild)
# Official releases are built by .github/workflows/release.yml on native runners.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
. "$HERE/env.sh"

case "$(uname -s)" in
  Darwin) "$HERE/macos/package.sh" --arch universal "$@" ;;
  Linux)
    "$HERE/linux/package.sh" "$@"
    "$HERE/windows/package.sh" "$@"
    ;;
  *) echo "error: unsupported packaging host $(uname -s)" >&2; exit 2 ;;
esac

(
  cd "$DIST"
  rm -f SHA256SUMS.txt
  for f in coralcraft-*; do
    [ -f "$f" ] && printf '%s  %s\n' "$(sha256_file "$f")" "$f"
  done >SHA256SUMS.txt
)
ls -lh "$DIST"
