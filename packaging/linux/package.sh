#!/usr/bin/env bash
# Build and package CoralCraft for Linux (<arch> is x86_64 or aarch64):
#   - $DIST/coralcraft-<version>-linux-<arch>.deb       Debian/Ubuntu package (nfpm or dpkg-deb)
#   - $DIST/coralcraft-<version>-linux-<arch>.rpm       Fedora/openSUSE/RHEL package (nfpm)
#   - $DIST/coralcraft-<version>-linux-<arch>.AppImage  AppImage (appimagetool)
#   - $DIST/coralcraft-<version>-linux-<arch>.tar.gz    FHS archive with install.sh
#
# Usage: packaging/linux/package.sh [--skip-build] [--formats "deb tar rpm appimage"]
#
# Binaries target a glibc 2.17 baseline via `cargo zigbuild` when available so they run on older
# distributions (see .github/workflows/ci.yml linux-package-smoke).

set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/linux"
APP_ID="ai.storyteller.coralcraft"

SKIP_BUILD=0
FORMATS="deb tar"
while [ $# -gt 0 ]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    --formats) FORMATS="$2"; shift 2 ;;
    -h | --help) sed -n '2,11p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

ARCH="${CROSS_ARCH:-$(uname -m)}"
case "$ARCH" in
  x86_64) DEB_ARCH="amd64"; TARGET="x86_64-unknown-linux-gnu" ;;
  aarch64 | arm64) ARCH="aarch64"; DEB_ARCH="arm64"; TARGET="aarch64-unknown-linux-gnu" ;;
  *) echo "unsupported architecture $ARCH" >&2; exit 2 ;;
esac
export CORALCRAFT_MAINTAINER="${CORALCRAFT_MAINTAINER:-CoralCraft Maintainers <maintainers@coralcraft.org>}"
BASENAME="coralcraft-$VERSION-linux-$ARCH"

ICONS_DIR="$CARGO_TARGET_DIR/package-icons"
python3 "$ROOT/packaging/icons.py" "$ICONS_DIR"

if [ "$SKIP_BUILD" = 0 ]; then
  if command -v cargo-zigbuild >/dev/null 2>&1; then
    (cd "$ROOT" && cargo zigbuild --release --locked -p coralcraft -p coralcraft-cli --target "$TARGET.2.17")
    BIN="$CARGO_TARGET_DIR/$TARGET/release"
  else
    (cd "$ROOT" && cargo build --release --locked -p coralcraft -p coralcraft-cli)
    BIN="$CARGO_TARGET_DIR/release"
  fi
else
  if [ -d "$CARGO_TARGET_DIR/$TARGET/release" ]; then
    BIN="$CARGO_TARGET_DIR/$TARGET/release"
  else
    BIN="$CARGO_TARGET_DIR/release"
  fi
fi

WORK="$CARGO_TARGET_DIR/linux-package"
STAGE="$WORK/root"
rm -rf "$WORK"
mkdir -p "$STAGE"

# 1. Stage standard FHS tree (/usr/bin, /usr/share/applications, /usr/share/mime, /usr/share/metainfo, /usr/share/icons)
install -Dm755 "$BIN/coralcraft" "$STAGE/usr/bin/coralcraft"
install -Dm755 "$BIN/coralcraft-cli" "$STAGE/usr/bin/coralcraft-cli"
strip "$STAGE/usr/bin/coralcraft" "$STAGE/usr/bin/coralcraft-cli" 2>/dev/null || true

install -Dm644 "$HERE/$APP_ID.desktop" "$STAGE/usr/share/applications/$APP_ID.desktop"
install -Dm644 "$HERE/$APP_ID.mime.xml" "$STAGE/usr/share/mime/packages/$APP_ID.xml"
mkdir -p "$STAGE/usr/share/metainfo"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@DATE@/$CORALCRAFT_BUILD_DATE/g" \
  "$HERE/$APP_ID.metainfo.xml.in" >"$STAGE/usr/share/metainfo/$APP_ID.metainfo.xml"
mkdir -p "$STAGE/usr/share/icons"
cp -R "$ICONS_DIR/hicolor" "$STAGE/usr/share/icons/"
copy_docs "$STAGE/usr/share/doc/coralcraft"

has() { case " $FORMATS " in *" $1 "*) return 0 ;; *) return 1 ;; esac; }

# 2. Build portable .tar.gz with install.sh
if has tar; then
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
install -Dm644 "$HERE/share/metainfo/ai.storyteller.coralcraft.metainfo.xml" "$PREFIX/share/metainfo/ai.storyteller.coralcraft.metainfo.xml"
mkdir -p "$PREFIX/share/icons"
cp -R "$HERE/share/icons/hicolor" "$PREFIX/share/icons/"
echo "Installed CoralCraft to $PREFIX/bin/coralcraft"
EOF
  chmod 755 "$TAR_STAGE/install.sh"

  OUT_TAR="$DIST/$BASENAME.tar.gz"
  rm -f "$OUT_TAR"
  tar -C "$WORK" -czf "$OUT_TAR" "$BASENAME"
  echo "wrote $OUT_TAR"
fi

# 3. Build .deb and/or .rpm (nfpm when available; dpkg-deb fallback for .deb)
if has deb || has rpm; then
  if command -v nfpm >/dev/null 2>&1; then
    export VERSION
    export NFPM_ARCH="$DEB_ARCH"
    sed "s|\${STAGE}|$STAGE|g" "$HERE/nfpm.yaml" >"$WORK/nfpm.yaml"
    for fmt in deb rpm; do
      if has "$fmt"; then
        rm -f "$DIST/$BASENAME.$fmt"
        (cd "$ROOT" && nfpm package -f "$WORK/nfpm.yaml" -p "$fmt" -t "$DIST/$BASENAME.$fmt")
        echo "wrote $DIST/$BASENAME.$fmt"
      fi
    done
  else
    if has rpm; then
      echo "error: nfpm not found (required for .rpm: https://nfpm.goreleaser.com/install/)" >&2
      exit 1
    fi
    if has deb; then
      DEB_STAGE="$WORK/deb-root"
      cp -R "$STAGE" "$DEB_STAGE"
      mkdir -p "$DEB_STAGE/DEBIAN"
      INSTALLED_KB="$(du -sk "$DEB_STAGE/usr" | cut -f1)"
      cat >"$DEB_STAGE/DEBIAN/control" <<EOF
Package: coralcraft
Version: $VERSION
Section: graphics
Priority: optional
Architecture: $DEB_ARCH
Installed-Size: $INSTALLED_KB
Depends: libc6 (>= 2.17), libgcc-s1
Maintainer: $CORALCRAFT_MAINTAINER
Homepage: https://github.com/yupoet/coralcraft
Description: CoralCraft Vector Illustration and Multi-Page Layout Studio
 Clean-room, Rust-native vector illustration and multi-page layout application
 supporting .coralcraft, .cdr, .cmx, SVG 1.1, PDF 1.4, EPSF-3.0, and DXF,
 plus a 244-tool Model Context Protocol (MCP) server.
EOF
      cp "$HERE/postinst.sh" "$DEB_STAGE/DEBIAN/postinst"
      chmod 755 "$DEB_STAGE/DEBIAN/postinst"
      OUT_DEB="$DIST/$BASENAME.deb"
      rm -f "$OUT_DEB"
      dpkg-deb -Zxz --build --root-owner-group "$DEB_STAGE" "$OUT_DEB" >/dev/null
      echo "wrote $OUT_DEB"
    fi
  fi
fi

# 4. Build AppImage (appimagetool)
if has appimage; then
  APPDIR="$WORK/CoralCraft.AppDir"
  cp -R "$STAGE" "$APPDIR"
  rm -rf "$APPDIR/usr/share/doc"
  ln -s usr/bin/coralcraft "$APPDIR/AppRun"
  cp "$HERE/$APP_ID.desktop" "$APPDIR/$APP_ID.desktop"
  cp "$ICONS_DIR/hicolor/256x256/apps/$APP_ID.png" "$APPDIR/$APP_ID.png"
  ln -s "$APP_ID.png" "$APPDIR/.DirIcon"

  TOOL="${APPIMAGETOOL:-$(command -v appimagetool || true)}"
  if [ -z "$TOOL" ]; then
    TOOL="$CARGO_TARGET_DIR/appimagetool-$ARCH.AppImage"
    if [ ! -x "$TOOL" ]; then
      curl -fsSL -o "$TOOL" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage"
      chmod +x "$TOOL"
    fi
  fi
  OUT="$(cd "$DIST" && pwd)/$BASENAME.AppImage"
  APPDIR="$(cd "$APPDIR" && pwd)"
  TOOL="$(cd "$(dirname "$TOOL")" && pwd)/$(basename "$TOOL")"
  rm -f "$OUT" "$OUT.zsync"
  (cd "$DIST" && ARCH="$ARCH" APPIMAGE_EXTRACT_AND_RUN=1 "$TOOL" --no-appstream "$APPDIR" "$OUT")
  echo "wrote $OUT"
fi

"$STAGE/usr/bin/coralcraft-cli" --version
ls -lh "$DIST"/coralcraft-"$VERSION"-linux-*
