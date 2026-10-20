#!/usr/bin/env bash
# Build and sign the macOS release artifacts on a macOS host (modeled on refs/vectorcraft):
#
#   $DIST/coralcraft-<version>-macos-<arch>.app.zip      CoralCraft.app (ditto archive)
#   $DIST/coralcraft-cli-<version>-macos-<arch>.zip      the headless CLI (includes `coralcraft-cli mcp`)
#
# Usage: packaging/macos/package.sh [--arch universal|aarch64|x86_64] [--skip-build]
#
# Signing (env):
#   MACOS_SIGN_IDENTITY   codesign identity. Default "-" = ad-hoc: the app runs on the build machine
#                         and on other Macs only after removing the quarantine attribute
#                         (`xattr -dr com.apple.quarantine CoralCraft.app`). Not notarized.
#   MACOS_KEYCHAIN        keychain holding the identity (optional)
#
# This script must run on macOS: it needs lipo, codesign, plutil and ditto. Binary package formats
# are never hand-rolled on other hosts (see docs/ai-memory/04-known-issues.md #10).
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/macos"

if [ "$(uname -s)" != "Darwin" ]; then
  echo "error: packaging/macos/package.sh must run on macOS (lipo/codesign/ditto required)" >&2
  exit 2
fi

ARCH=universal
SKIP_BUILD=0
while [ $# -gt 0 ]; do
  case "$1" in
    --arch) ARCH="$2"; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    -h | --help) sed -n '2,16p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$ARCH" in
  universal) TARGETS=(aarch64-apple-darwin x86_64-apple-darwin) ;;
  aarch64) TARGETS=(aarch64-apple-darwin) ;;
  x86_64) TARGETS=(x86_64-apple-darwin) ;;
  *) echo "unknown --arch $ARCH" >&2; exit 2 ;;
esac

# Keep in sync with LSMinimumSystemVersion in Info.plist.in.
export MACOSX_DEPLOYMENT_TARGET=11.0
IDENTITY="${MACOS_SIGN_IDENTITY:--}"
SHORT_VERSION="${VERSION%%-*}"
WORK="$CARGO_TARGET_DIR/macos-package"
APP="$WORK/CoralCraft.app"
APP_ZIP="$DIST/coralcraft-$VERSION-macos-$ARCH.app.zip"
CLI_ZIP="$DIST/coralcraft-cli-$VERSION-macos-$ARCH.zip"

if [ "$IDENTITY" = "-" ]; then
  warn "macOS: ad-hoc signing (no MACOS_SIGN_IDENTITY); artifacts are not notarized"
fi
echo "==> CoralCraft $VERSION for macOS ($ARCH), identity: $IDENTITY"

# ---- build -------------------------------------------------------------------------------------
if [ "$SKIP_BUILD" = 0 ]; then
  args=()
  for t in "${TARGETS[@]}"; do args+=(--target "$t"); done
  (cd "$ROOT" && cargo build --release --locked -p coralcraft -p coralcraft-cli "${args[@]}")
fi

rm -rf "$WORK"
mkdir -p "$WORK/bin"
for bin in coralcraft coralcraft-cli; do
  inputs=()
  for t in "${TARGETS[@]}"; do inputs+=("$CARGO_TARGET_DIR/$t/release/$bin"); done
  lipo -create -output "$WORK/bin/$bin" "${inputs[@]}"
  lipo -info "$WORK/bin/$bin"
done

sign() {
  local ts=(--timestamp)
  [ "$IDENTITY" = "-" ] && ts=(--timestamp=none)
  local kc=()
  [ -n "${MACOS_KEYCHAIN:-}" ] && kc=(--keychain "$MACOS_KEYCHAIN")
  codesign --force --sign "$IDENTITY" ${kc[@]+"${kc[@]}"} "${ts[@]}" "$@"
}

# ---- CoralCraft.app ----------------------------------------------------------------------------
echo "==> assembling $APP"
ICONS_DIR="$WORK/icons"
python3 "$ROOT/packaging/icons.py" "$ICONS_DIR"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$WORK/bin/coralcraft" "$APP/Contents/MacOS/CoralCraft"
cp "$ICONS_DIR/CoralCraft.icns" "$APP/Contents/Resources/CoralCraft.icns"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@SHORT_VERSION@/$SHORT_VERSION/g" \
  -e "s/@BUILD_SHA@/${CORALCRAFT_BUILD_SHA:-unknown}/g" \
  "$HERE/Info.plist.in" >"$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"
printf 'APPL????' >"$APP/Contents/PkgInfo"
copy_docs "$APP/Contents/Resources"

# Sign inside-out: the main executable first, then the bundle (no --deep on the final signature).
sign --options runtime --entitlements "$HERE/entitlements.plist" "$APP/Contents/MacOS/CoralCraft"
sign --options runtime --entitlements "$HERE/entitlements.plist" "$APP"
codesign --verify --strict --deep --verbose=2 "$APP"

rm -f "$APP_ZIP"
ditto -c -k --keepParent "$APP" "$APP_ZIP"

# ---- CLI ---------------------------------------------------------------------------------------
echo "==> building $CLI_ZIP"
CLI_DIR="$WORK/coralcraft-cli-$VERSION-macos-$ARCH"
mkdir -p "$CLI_DIR"
cp "$WORK/bin/coralcraft-cli" "$CLI_DIR/"
copy_docs "$CLI_DIR"
sign --options runtime "$CLI_DIR/coralcraft-cli"
codesign --verify --strict --verbose=2 "$CLI_DIR/coralcraft-cli"
rm -f "$CLI_ZIP"
ditto -c -k --keepParent "$CLI_DIR" "$CLI_ZIP"

# ---- smoke test --------------------------------------------------------------------------------
"$WORK/bin/coralcraft-cli" --version
"$APP/Contents/MacOS/CoralCraft" --version
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | "$WORK/bin/coralcraft-cli" mcp | grep -q '"tools"'
echo "==> done"
ls -lh "$APP_ZIP" "$CLI_ZIP"
