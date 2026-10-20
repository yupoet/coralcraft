#!/usr/bin/env bash
# Build and package CoralCraft for FreeBSD (x86_64):
#   - coralcraft-<version>-freebsd-x86_64.tar.gz

set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
APP_ID="ai.storyteller.coralcraft"

SKIP_BUILD=0
while [ $# -gt 0 ]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

ARCH="x86_64"
BASENAME="coralcraft-$VERSION-freebsd-$ARCH"
ICONS_DIR="$CARGO_TARGET_DIR/package-icons"
python3 "$ROOT/packaging/icons.py" "$ICONS_DIR"

if [ "$(uname -s)" = "FreeBSD" ]; then
  TARGET="x86_64-unknown-freebsd"
  if [ "$SKIP_BUILD" = 0 ] || [ ! -f "$CARGO_TARGET_DIR/$TARGET/release/coralcraft" ]; then
    (cd "$ROOT" && cargo build --release -p coralcraft -p coralcraft-cli --target "$TARGET")
  fi
else
  TARGET="x86_64-unknown-linux-musl"
  if [ "$SKIP_BUILD" = 0 ] || [ ! -f "$CARGO_TARGET_DIR/$TARGET/release/coralcraft" ]; then
    PATH="/tmp/bin:$PATH" cargo zigbuild --release -p coralcraft -p coralcraft-cli --target "$TARGET"
  fi
fi

BIN="$CARGO_TARGET_DIR/$TARGET/release"
WORK="$CARGO_TARGET_DIR/freebsd-package"
STAGE="$WORK/$BASENAME"
rm -rf "$WORK"
mkdir -p "$STAGE/bin" "$STAGE/share/applications" "$STAGE/share/mime/packages" "$STAGE/share/icons"

cp "$BIN/coralcraft" "$STAGE/bin/coralcraft"
cp "$BIN/coralcraft-cli" "$STAGE/bin/coralcraft-cli"
chmod 755 "$STAGE/bin/coralcraft" "$STAGE/bin/coralcraft-cli"
strip "$STAGE/bin/coralcraft" "$STAGE/bin/coralcraft-cli" 2>/dev/null || true

# Stamp ELF OSABI = 9 (ELFOSABI_FREEBSD) on the packaged FreeBSD binaries
python3 - "$STAGE/bin/coralcraft" "$STAGE/bin/coralcraft-cli" <<'PY'
import sys
for p in sys.argv[1:]:
    with open(p, "r+b") as f:
        hdr = bytearray(f.read(16))
        assert hdr[:4] == b"\x7fELF", f"Not an ELF binary: {p}"
        hdr[7] = 9  # ELFOSABI_FREEBSD (9)
        f.seek(0)
        f.write(hdr)
PY

cp "$ROOT/packaging/linux/$APP_ID.desktop" "$STAGE/share/applications/$APP_ID.desktop"
cp "$ROOT/packaging/linux/$APP_ID.mime.xml" "$STAGE/share/mime/packages/$APP_ID.xml"
cp -R "$ICONS_DIR/hicolor" "$STAGE/share/icons/"
copy_docs "$STAGE/share/doc/coralcraft"

cat >"$STAGE/install.sh" <<'EOF'
#!/bin/sh
set -eu
PREFIX="${PREFIX:-/usr/local}"
HERE="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
mkdir -p "$PREFIX/bin" "$PREFIX/share/applications" "$PREFIX/share/mime/packages" "$PREFIX/share/icons"
cp "$HERE/bin/coralcraft" "$PREFIX/bin/coralcraft"
cp "$HERE/bin/coralcraft-cli" "$PREFIX/bin/coralcraft-cli"
chmod 755 "$PREFIX/bin/coralcraft" "$PREFIX/bin/coralcraft-cli"
cp "$HERE/share/applications/ai.storyteller.coralcraft.desktop" "$PREFIX/share/applications/"
cp "$HERE/share/mime/packages/ai.storyteller.coralcraft.xml" "$PREFIX/share/mime/packages/"
cp -R "$HERE/share/icons/hicolor" "$PREFIX/share/icons/"
echo "Installed CoralCraft for FreeBSD to $PREFIX/bin/coralcraft"
EOF
chmod 755 "$STAGE/install.sh"

OUT_TAR="$DIST/$BASENAME.tar.gz"
rm -f "$OUT_TAR"
tar -C "$WORK" -czf "$OUT_TAR" "$BASENAME"
echo "wrote $OUT_TAR"
ls -lh "$OUT_TAR"
