#!/usr/bin/env bash
# Build and package CoralCraft natively on a FreeBSD host:
#
#   $DIST/coralcraft-<version>-freebsd-<arch>.tar.gz   a /usr/local-style tree:
#       coralcraft-<version>-freebsd-<arch>/{bin, share/applications, share/icons, share/mime,
#       share/metainfo, share/doc/coralcraft}
#
# Usage: packaging/freebsd/package.sh [--skip-build]
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
LINUX="$ROOT/packaging/linux"
APP_ID=ai.storyteller.coralcraft

if [ "$(uname -s)" != "FreeBSD" ]; then
  echo "error: packaging/freebsd/package.sh must run natively on a FreeBSD host" >&2
  exit 2
fi

SKIP_BUILD=0
while [ $# -gt 0 ]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    -h | --help) sed -n '2,8p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

case "$(uname -m)" in
  amd64 | x86_64) ARCH=x86_64 ;;
  arm64 | aarch64) ARCH=aarch64 ;;
  *) echo "unsupported architecture $(uname -m)" >&2; exit 2 ;;
esac
BASENAME="coralcraft-$VERSION-freebsd-$ARCH"

echo "==> CoralCraft $VERSION for FreeBSD $ARCH"

if [ "$SKIP_BUILD" = 0 ]; then
  (cd "$ROOT" && cargo build --release --locked -p coralcraft -p coralcraft-cli)
fi
BIN="$CARGO_TARGET_DIR/release"
WORK="$CARGO_TARGET_DIR/freebsd-package"
STAGE="$WORK/$BASENAME"
rm -rf "$WORK"

ICONS_DIR="$CARGO_TARGET_DIR/package-icons"
python3 "$ROOT/packaging/icons.py" "$ICONS_DIR"

mkdir -p "$STAGE/bin" "$STAGE/share/applications" "$STAGE/share/mime/packages" "$STAGE/share/metainfo" \
  "$STAGE/share/icons" "$STAGE/share/doc/coralcraft"
install -m 755 "$BIN/coralcraft" "$BIN/coralcraft-cli" "$STAGE/bin/"
strip "$STAGE/bin/coralcraft" "$STAGE/bin/coralcraft-cli" 2>/dev/null || true
install -m 644 "$LINUX/$APP_ID.desktop" "$STAGE/share/applications/$APP_ID.desktop"
install -m 644 "$LINUX/$APP_ID.mime.xml" "$STAGE/share/mime/packages/$APP_ID.xml"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@DATE@/$CORALCRAFT_BUILD_DATE/g" \
  "$LINUX/$APP_ID.metainfo.xml.in" >"$STAGE/share/metainfo/$APP_ID.metainfo.xml"
cp -R "$ICONS_DIR/hicolor" "$STAGE/share/icons/"
copy_docs "$STAGE/share/doc/coralcraft"

tar -C "$WORK" -czf "$DIST/$BASENAME.tar.gz" "$BASENAME"
echo "wrote $DIST/$BASENAME.tar.gz"

"$STAGE/bin/coralcraft-cli" --version
ls -lh "$DIST/$BASENAME.tar.gz"
