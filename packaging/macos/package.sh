#!/usr/bin/env bash
# Build and package CoralCraft for macOS:
#   - Intel (x86_64-apple-darwin)
#   - Apple Silicon M-series (aarch64-apple-darwin)
#   - Universal2 (combined x86_64 + arm64 Fat Mach-O)
#
# Outputs in $DIST:
#   coralcraft-<version>-macos-x86_64.pkg         Installable macOS Flat Package (Intel)
#   coralcraft-<version>-macos-x86_64.dmg         macOS UDIF Disk Image (Intel)
#   coralcraft-<version>-macos-x86_64.app.zip     CoralCraft.app bundle archive (Intel)
#   coralcraft-<version>-macos-aarch64.pkg        Installable macOS Flat Package (Apple Silicon M-series)
#   coralcraft-<version>-macos-aarch64.dmg        macOS UDIF Disk Image (Apple Silicon M-series)
#   coralcraft-<version>-macos-aarch64.app.zip    CoralCraft.app bundle archive (Apple Silicon M-series)
#   coralcraft-<version>-macos-universal.dmg      Universal2 macOS UDIF Disk Image (Intel + Apple Silicon)
#   coralcraft-<version>-macos-universal.pkg      Universal2 Installable macOS Flat Package
#   coralcraft-<version>-macos-universal.app.zip  Universal2 CoralCraft.app bundle archive

set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/macos"

ARCH="both"
SKIP_BUILD=0
while [ $# -gt 0 ]; do
  case "$1" in
    --arch) ARCH="$2"; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

case "$ARCH" in
  both | universal) ARCH_LIST=(x86_64 aarch64) ;;
  x86_64 | intel) ARCH_LIST=(x86_64) ;;
  aarch64 | arm64 | m) ARCH_LIST=(aarch64) ;;
  *) echo "unsupported --arch $ARCH (expected x86_64, aarch64, or both)" >&2; exit 2 ;;
esac

SHORT_VERSION="${VERSION%%-*}"
ICONS_DIR="$CARGO_TARGET_DIR/package-icons"
python3 "$ROOT/packaging/icons.py" "$ICONS_DIR"

build_target() {
  local target="$1"
  if [ "$SKIP_BUILD" = 1 ]; then return 0; fi
  if [ "$(uname -s)" = "Darwin" ]; then
    (cd "$ROOT" && cargo build --release --locked -p coralcraft -p coralcraft-cli --target "$target")
  else
    PATH="/tmp/bin:$PATH" cargo zigbuild --release -p coralcraft -p coralcraft-cli --target "$target"
  fi
}

bundle_stage_to_outputs() {
  local root_stage="$1"
  local tag="$2"
  local app="$root_stage/CoralCraft.app"

  cp "$ICONS_DIR/CoralCraft.icns" "$app/Contents/Resources/CoralCraft.icns"
  sed -e "s/@VERSION@/$VERSION/g" \
      -e "s/@SHORT_VERSION@/$SHORT_VERSION/g" \
      -e "s/@BUILD_SHA@/$CORALCRAFT_BUILD_SHA/g" \
      "$HERE/Info.plist.in" >"$app/Contents/Info.plist"
  printf 'APPL????' >"$app/Contents/PkgInfo"
  copy_docs "$app/Contents/Resources"

  if [ "$(uname -s)" = "Darwin" ] && command -v codesign >/dev/null 2>&1; then
    codesign --force --sign "${MACOS_SIGN_IDENTITY:--}" --timestamp=none "$app/Contents/MacOS/coralcraft-cli"
    codesign --force --sign "${MACOS_SIGN_IDENTITY:--}" --timestamp=none "$app/Contents/MacOS/CoralCraft"
    codesign --force --sign "${MACOS_SIGN_IDENTITY:--}" --timestamp=none "$app"
  fi

  local out_pkg="$DIST/coralcraft-$VERSION-macos-$tag.pkg"
  local out_dmg="$DIST/coralcraft-$VERSION-macos-$tag.dmg"
  local out_zip="$DIST/coralcraft-$VERSION-macos-$tag.app.zip"
  rm -f "$out_pkg" "$out_dmg" "$out_zip"

  python3 "$HERE/build_flat_pkg.py" "$root_stage" "$SHORT_VERSION" "$out_pkg"
  python3 "$HERE/build_dmg.py" "$root_stage" "CoralCraft" "$out_dmg"

  (cd "$root_stage" && python3 -c '
import os, sys, zipfile
out_zip = sys.argv[1]
with zipfile.ZipFile(out_zip, "w", zipfile.ZIP_DEFLATED) as zf:
    for base, dirs, files in os.walk("CoralCraft.app"):
        for fn in files:
            p = os.path.join(base, fn)
            info = zipfile.ZipInfo(p)
            st = os.stat(p)
            info.external_attr = (st.st_mode & 0xFFFF) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            with open(p, "rb") as f:
                zf.writestr(info, f.read())
' "$out_zip")

  echo "wrote $out_pkg"
  echo "wrote $out_dmg"
  echo "wrote $out_zip"
}

package_one_arch() {
  local arch="$1"
  local target="${arch}-apple-darwin"
  echo "==> Packaging CoralCraft $VERSION for macOS ($arch / $target)"
  build_target "$target"

  local work="$CARGO_TARGET_DIR/macos-package-$arch"
  local root_stage="$work/root"
  local app="$root_stage/CoralCraft.app"
  rm -rf "$work"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

  cp "$CARGO_TARGET_DIR/$target/release/coralcraft" "$app/Contents/MacOS/CoralCraft"
  cp "$CARGO_TARGET_DIR/$target/release/coralcraft-cli" "$app/Contents/MacOS/coralcraft-cli"
  chmod 755 "$app/Contents/MacOS/CoralCraft" "$app/Contents/MacOS/coralcraft-cli"

  bundle_stage_to_outputs "$root_stage" "$arch"
}

for a in "${ARCH_LIST[@]}"; do
  package_one_arch "$a"
done

if [ "${#ARCH_LIST[@]}" -eq 2 ]; then
  echo "==> Packaging CoralCraft $VERSION for macOS Universal2 (x86_64 + aarch64)"
  WORK_UNIV="$CARGO_TARGET_DIR/macos-package-universal"
  ROOT_UNIV="$WORK_UNIV/root"
  APP_UNIV="$ROOT_UNIV/CoralCraft.app"
  rm -rf "$WORK_UNIV"
  mkdir -p "$APP_UNIV/Contents/MacOS" "$APP_UNIV/Contents/Resources"

  python3 "$HERE/make_universal_macho.py" \
    "$CARGO_TARGET_DIR/x86_64-apple-darwin/release/coralcraft" \
    "$CARGO_TARGET_DIR/aarch64-apple-darwin/release/coralcraft" \
    "$APP_UNIV/Contents/MacOS/CoralCraft"
  python3 "$HERE/make_universal_macho.py" \
    "$CARGO_TARGET_DIR/x86_64-apple-darwin/release/coralcraft-cli" \
    "$CARGO_TARGET_DIR/aarch64-apple-darwin/release/coralcraft-cli" \
    "$APP_UNIV/Contents/MacOS/coralcraft-cli"
  chmod 755 "$APP_UNIV/Contents/MacOS/CoralCraft" "$APP_UNIV/Contents/MacOS/coralcraft-cli"

  bundle_stage_to_outputs "$ROOT_UNIV" "universal"
fi

ls -lh "$DIST"/coralcraft-"$VERSION"-macos-*
