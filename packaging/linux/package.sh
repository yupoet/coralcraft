#!/usr/bin/env bash
# Build and package CoralCraft for Linux GUI (x86_64):
#   - coralcraft-<version>-linux-x86_64.deb       Debian/Ubuntu/Mint/Pop!_OS package (dpkg-deb)
#   - coralcraft-<version>-linux-x86_64.rpm       Fedora/RHEL/openSUSE RPM package
#   - coralcraft-<version>-linux-x86_64.flatpak   Flatpak single-file bundle
#   - coralcraft-<version>-linux-x86_64.AppImage  Single-file portable executable Linux GUI bundle
#   - coralcraft-<version>-linux-x86_64.tar.gz    FHS archive with install.sh

set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/linux"
APP_ID="ai.storyteller.coralcraft"

SKIP_BUILD=0
while [ $# -gt 0 ]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

ARCH="x86_64"
TARGET="x86_64-unknown-linux-gnu"
DEB_ARCH="amd64"
BASENAME="coralcraft-$VERSION-linux-$ARCH"

ICONS_DIR="$CARGO_TARGET_DIR/package-icons"
python3 "$ROOT/packaging/icons.py" "$ICONS_DIR"

if [ "$SKIP_BUILD" = 0 ]; then
  (cd "$ROOT" && cargo build --release -p coralcraft -p coralcraft-cli --target "$TARGET")
fi

BIN="$CARGO_TARGET_DIR/$TARGET/release"
WORK="$CARGO_TARGET_DIR/linux-package"
STAGE="$WORK/deb-root"
rm -rf "$WORK"
mkdir -p "$STAGE"

# 1. Stage standard FHS tree (/usr/bin, /usr/share/applications, /usr/share/mime, /usr/share/icons)
install -Dm755 "$BIN/coralcraft" "$STAGE/usr/bin/coralcraft"
install -Dm755 "$BIN/coralcraft-cli" "$STAGE/usr/bin/coralcraft-cli"
strip "$STAGE/usr/bin/coralcraft" "$STAGE/usr/bin/coralcraft-cli" 2>/dev/null || true

install -Dm644 "$HERE/$APP_ID.desktop" "$STAGE/usr/share/applications/$APP_ID.desktop"
install -Dm644 "$HERE/$APP_ID.mime.xml" "$STAGE/usr/share/mime/packages/$APP_ID.xml"
mkdir -p "$STAGE/usr/share/icons"
cp -R "$ICONS_DIR/hicolor" "$STAGE/usr/share/icons/"
copy_docs "$STAGE/usr/share/doc/coralcraft"

# 2. Build Debian/Ubuntu .deb package
mkdir -p "$STAGE/DEBIAN"
INSTALLED_KB="$(du -sk "$STAGE/usr" | cut -f1)"
cat >"$STAGE/DEBIAN/control" <<EOF
Package: coralcraft
Version: $VERSION
Section: graphics
Priority: optional
Architecture: $DEB_ARCH
Installed-Size: $INSTALLED_KB
Maintainer: CoralCraft Maintainers <maintainers@coralcraft.org>
Homepage: https://github.com/yupoet/coralcraft
Description: CoralCraft Vector Illustration and Multi-Page Layout Studio
 Clean-room, Rust-native vector illustration and multi-page layout application
 supporting .coralcraft, .cdr, .cmx, SVG 1.1, PDF 1.4, EPSF-3.0, and DXF,
 plus a 240-tool Model Context Protocol (MCP) server.
EOF

cat >"$STAGE/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q /usr/share/applications || true
fi
if command -v update-mime-database >/dev/null 2>&1; then
  update-mime-database /usr/share/mime || true
fi
exit 0
EOF
chmod 755 "$STAGE/DEBIAN/postinst"

OUT_DEB="$DIST/$BASENAME.deb"
rm -f "$OUT_DEB"
dpkg-deb --build --root-owner-group "$STAGE" "$OUT_DEB" >/dev/null
echo "wrote $OUT_DEB"

# 3. Build portable .tar.gz with install.sh
TAR_STAGE="$WORK/$BASENAME"
mkdir -p "$TAR_STAGE"
cp -R "$STAGE/usr/"* "$TAR_STAGE/"
cat >"$TAR_STAGE/install.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
PREFIX="${PREFIX:-/usr/local}"
HERE="$(cd "$(dirname "$0")" && pwd)"
install -Dm755 "$HERE/bin/coralcraft" "$PREFIX/bin/coralcraft"
install -Dm755 "$HERE/bin/coralcraft-cli" "$PREFIX/bin/coralcraft-cli"
install -Dm644 "$HERE/share/applications/ai.storyteller.coralcraft.desktop" "$PREFIX/share/applications/ai.storyteller.coralcraft.desktop"
install -Dm644 "$HERE/share/mime/packages/ai.storyteller.coralcraft.xml" "$PREFIX/share/mime/packages/ai.storyteller.coralcraft.xml"
mkdir -p "$PREFIX/share/icons"
cp -R "$HERE/share/icons/hicolor" "$PREFIX/share/icons/"
echo "Installed CoralCraft to $PREFIX/bin/coralcraft"
EOF
chmod 755 "$TAR_STAGE/install.sh"

OUT_TAR="$DIST/$BASENAME.tar.gz"
rm -f "$OUT_TAR"
tar -C "$WORK" -czf "$OUT_TAR" "$BASENAME"
echo "wrote $OUT_TAR"

# 4. Build self-contained single-file .AppImage bundle (FUSE-free self-extracting AppDir runner)
APPDIR="$WORK/CoralCraft.AppDir"
mkdir -p "$APPDIR"
cp -R "$STAGE/usr" "$APPDIR/usr"
cp "$HERE/$APP_ID.desktop" "$APPDIR/$APP_ID.desktop"
cp "$ICONS_DIR/hicolor/256x256/apps/$APP_ID.png" "$APPDIR/$APP_ID.png"
cat >"$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/coralcraft" "$@"
EOF
chmod 755 "$APPDIR/AppRun"

APPDIR_TAR="$WORK/appdir.tar.gz"
tar -C "$APPDIR" -czf "$APPDIR_TAR" .
OUT_APPIMAGE="$DIST/$BASENAME.AppImage"
cat >"$OUT_APPIMAGE" <<'EOF'
#!/usr/bin/env bash
# CoralCraft Self-Extracting FUSE-Free AppImage Bundle (x86_64-linux)
set -euo pipefail
if [ "${1:-}" = "--appimage-extract" ]; then
  mkdir -p squashfs-root
  ARCHIVE_LINE="$(awk '/^__APPIMAGE_ARCHIVE_BELOW__$/ {print NR + 1; exit 0; }' "$0")"
  tail -n +"$ARCHIVE_LINE" "$0" | tar -xz -C squashfs-root
  echo "Extracted to squashfs-root/"
  exit 0
fi
TMPDIR_RUN="$(mktemp -d "${TMPDIR:-/tmp}/coralcraft-appimage.XXXXXX")"
trap 'rm -rf "$TMPDIR_RUN"' EXIT
ARCHIVE_LINE="$(awk '/^__APPIMAGE_ARCHIVE_BELOW__$/ {print NR + 1; exit 0; }' "$0")"
tail -n +"$ARCHIVE_LINE" "$0" | tar -xz -C "$TMPDIR_RUN"
exec "$TMPDIR_RUN/AppRun" "$@"
__APPIMAGE_ARCHIVE_BELOW__
EOF
cat "$APPDIR_TAR" >>"$OUT_APPIMAGE"
chmod 755 "$OUT_APPIMAGE"
echo "wrote $OUT_APPIMAGE"

# 5. Build Fedora/RHEL/openSUSE .rpm and Flatpak .flatpak bundle
OUT_RPM="$DIST/$BASENAME.rpm"
OUT_FLATPAK="$DIST/$BASENAME.flatpak"
rm -f "$OUT_RPM" "$OUT_FLATPAK"
python3 "$HERE/build_rpm_flatpak.py" "$STAGE" "$VERSION" "$OUT_RPM" "$OUT_FLATPAK"
echo "wrote $OUT_RPM"
echo "wrote $OUT_FLATPAK"

ls -lh "$OUT_DEB" "$OUT_RPM" "$OUT_FLATPAK" "$OUT_TAR" "$OUT_APPIMAGE"
