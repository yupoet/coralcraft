#!/usr/bin/env bash
# Build the browser WebAssembly version of CoralCraft (`coralcraft-ui-egui` + `coralcraft-engine`
# compiled for `wasm32-unknown-unknown`) and package it as a static web archive:
#
#   $DIST/coralcraft-web-$VERSION.zip
#
# Usage: packaging/web/package.sh [--skip-build]
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/web"

SKIP_BUILD=0
while [ $# -gt 0 ]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    -h | --help) sed -n '2,8p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [ "$SKIP_BUILD" = 0 ]; then
  (cd "$ROOT" && cargo build --release --locked -p coralcraft-ui-egui --target wasm32-unknown-unknown)
fi

WASM_RLS="$CARGO_TARGET_DIR/wasm32-unknown-unknown/release/libcoralcraft_ui_egui.rlib"
if [ ! -f "$WASM_RLS" ] && [ -f "$ROOT/apps/coralcraft-web/index.html" ] && command -v trunk >/dev/null 2>&1; then
  (cd "$ROOT/apps/coralcraft-web" && trunk build --release)
fi

NAME="coralcraft-web-$VERSION"
WORK="$CARGO_TARGET_DIR/web-package"
rm -rf "$WORK"
mkdir -p "$WORK/$NAME"
cp "$HERE/_headers" "$HERE/.htaccess" "$WORK/$NAME/"
cp "$HERE/README.md" "$WORK/$NAME/HOSTING.md"
copy_docs "$WORK/$NAME"
if [ -d "$ROOT/dist/web" ]; then
  cp -R "$ROOT/dist/web/." "$WORK/$NAME/"
fi
rm -f "$DIST/$NAME.zip"
python3 - "$WORK" "$NAME" "$DIST/$NAME.zip" <<'PY'
import os, sys, zipfile
work_dir, folder_name, out_zip = sys.argv[1], sys.argv[2], sys.argv[3]
root_dir = os.path.join(work_dir, folder_name)
with zipfile.ZipFile(out_zip, "w", zipfile.ZIP_DEFLATED) as zf:
    for base, _, files in os.walk(root_dir):
        for fn in files:
            full = os.path.join(base, fn)
            rel = os.path.relpath(full, work_dir)
            zf.write(full, rel)
PY
echo "wrote $DIST/$NAME.zip"
ls -lh "$DIST/$NAME.zip"
