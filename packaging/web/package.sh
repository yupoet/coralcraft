#!/usr/bin/env bash
# Build and package CoralCraft for Web (WebAssembly / wasm32-unknown-unknown):
#   - coralcraft-<version>-web-wasm32.zip
#   - coralcraft-<version>-web-wasm32.tar.gz

set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"

SKIP_BUILD=0
while [ $# -gt 0 ]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

TARGET="wasm32-unknown-unknown"
BASENAME="coralcraft-$VERSION-web-wasm32"
ICONS_DIR="$CARGO_TARGET_DIR/package-icons"
python3 "$ROOT/packaging/icons.py" "$ICONS_DIR"

echo "==> Packaging CoralCraft $VERSION for Web (WebAssembly / $TARGET)"
if [ "$SKIP_BUILD" = 0 ] || [ ! -f "$CARGO_TARGET_DIR/$TARGET/release/coralcraft.wasm" ]; then
  (cd "$ROOT" && cargo build --release --target "$TARGET" -p coralcraft -p coralcraft-cli)
fi

WORK="$CARGO_TARGET_DIR/web-package"
STAGE="$WORK/$BASENAME"
rm -rf "$WORK"
mkdir -p "$STAGE"

cp "$CARGO_TARGET_DIR/$TARGET/release/coralcraft.wasm" "$STAGE/coralcraft.wasm"
cp "$CARGO_TARGET_DIR/$TARGET/release/coralcraft-cli.wasm" "$STAGE/coralcraft-cli.wasm"
cp "$ICONS_DIR/hicolor/256x256/apps/ai.storyteller.coralcraft.png" "$STAGE/coralcraft-icon.png"
copy_docs "$STAGE"

cat >"$STAGE/index.html" <<'EOF'
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>CoralCraft Studio (WebAssembly)</title>
<link rel="icon" type="image/png" href="coralcraft-icon.png">
<style>
  html, body {
    margin: 0; padding: 0; width: 100%; height: 100%;
    background: #1e1f22; color: #e6e7eb;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    display: flex; flex-direction: column; align-items: center; justify-content: center;
  }
  .card {
    background: #282a2e; border: 1px solid #3c3f45; border-radius: 10px;
    padding: 28px 36px; max-width: 680px; box-shadow: 0 12px 32px rgba(0,0,0,0.45);
  }
  h1 { margin: 0 0 8px; font-size: 22px; display: flex; align-items: center; gap: 12px; }
  code { background: #18191c; padding: 2px 6px; border-radius: 4px; color: #79c0ff; }
  #status { margin-top: 16px; padding: 12px; border-radius: 6px; background: #18191c; font-family: monospace; }
</style>
</head>
<body>
<div class="card">
  <h1><img src="coralcraft-icon.png" width="36" height="36" alt="CoralCraft"> CoralCraft WebAssembly Bundle</h1>
  <p>Pure-Rust vector illustration, multipage layout &amp; 240-tool MCP engine compiled to <code>wasm32-unknown-unknown</code>.</p>
  <ul>
    <li><code>coralcraft.wasm</code> — Desktop Studio WebAssembly module</li>
    <li><code>coralcraft-cli.wasm</code> — Full CLI &amp; 240-tool MCP engine WebAssembly module</li>
  </ul>
  <div id="status">Loading WebAssembly modules...</div>
</div>
<script>
(async () => {
  const statusEl = document.getElementById('status');
  try {
    const [appResp, cliResp] = await Promise.all([
      fetch('coralcraft.wasm'),
      fetch('coralcraft-cli.wasm')
    ]);
    const [appBuf, cliBuf] = await Promise.all([appResp.arrayBuffer(), cliResp.arrayBuffer()]);
    const [appMod, cliMod] = await Promise.all([
      WebAssembly.compile(appBuf),
      WebAssembly.compile(cliBuf)
    ]);
    statusEl.textContent =
      `Ready: coralcraft.wasm (${(appBuf.byteLength / 1024).toFixed(1)} KB, ${WebAssembly.Module.exports(appMod).length} exports) | ` +
      `coralcraft-cli.wasm (${(cliBuf.byteLength / 1048576).toFixed(2)} MB, ${WebAssembly.Module.exports(cliMod).length} exports)`;
  } catch (err) {
    statusEl.textContent = 'WebAssembly load status: ' + err;
  }
})();
</script>
</body>
</html>
EOF

OUT_ZIP="$DIST/$BASENAME.zip"
OUT_TAR="$DIST/$BASENAME.tar.gz"
rm -f "$OUT_ZIP" "$OUT_TAR"

tar -C "$WORK" -czf "$OUT_TAR" "$BASENAME"
python3 - "$WORK" "$BASENAME" "$OUT_ZIP" <<'PY'
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

echo "wrote $OUT_ZIP"
echo "wrote $OUT_TAR"
ls -lh "$OUT_ZIP" "$OUT_TAR"
