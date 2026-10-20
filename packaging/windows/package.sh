#!/usr/bin/env bash
# Build and package CoralCraft for Windows 64-bit (x64 and arm64):
#   - coralcraft-<version>-windows-x64-portable.zip    Portable x64 Windows archive
#   - coralcraft-<version>-windows-arm64-portable.zip  Portable ARM64 Windows archive
#
# The self-extracting setup.exe was withdrawn in the 2026-10-20 review (unchecked trailer parsing,
# no uninstaller). An MSI built with WiX on a windows runner replaces it later (docs/gaps.md R6.2).

set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"

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
  both) ARCH_LIST=(x64 arm64) ;;
  x64 | x86_64) ARCH_LIST=(x64) ;;
  arm64 | aarch64) ARCH_LIST=(arm64) ;;
  *) echo "unsupported --arch $ARCH (expected x64, arm64, or both)" >&2; exit 2 ;;
esac

ICONS_DIR="$CARGO_TARGET_DIR/package-icons"
python3 "$ROOT/packaging/icons.py" "$ICONS_DIR"

if [ -x /tmp/bin/zig ]; then
  if [ ! -x /tmp/bin/x86_64-w64-mingw32-dlltool ]; then
    printf '#!/bin/sh\nexec /tmp/bin/zig dlltool "$@"\n' >/tmp/bin/x86_64-w64-mingw32-dlltool
    chmod +x /tmp/bin/x86_64-w64-mingw32-dlltool
  fi
  if [ ! -x /tmp/bin/llvm-dlltool ]; then
    printf '#!/bin/sh\nexec /tmp/bin/zig dlltool "$@"\n' >/tmp/bin/llvm-dlltool
    chmod +x /tmp/bin/llvm-dlltool
  fi
fi

package_windows_arch() {
  local win_arch="$1"
  local target expected_machine
  if [ "$win_arch" = "x64" ]; then
    target="x86_64-pc-windows-gnu"
    expected_machine=$((0x8664))
  else
    target="aarch64-pc-windows-gnullvm"
    expected_machine=$((0xAA64))
  fi

  echo "==> Packaging CoralCraft $VERSION for Windows ($win_arch / $target)"
  local bin_dir="$CARGO_TARGET_DIR/$target/release"
  if [ "$SKIP_BUILD" = 0 ] || [ ! -f "$bin_dir/coralcraft.exe" ]; then
    PATH="/tmp/bin:$PATH" cargo zigbuild --release -p coralcraft -p coralcraft-cli --target "$target"
  fi

  local work="$CARGO_TARGET_DIR/windows-package-$win_arch"
  local folder="coralcraft-$VERSION-windows-$win_arch"
  local stage="$work/$folder"
  rm -rf "$work"
  mkdir -p "$stage"

  cp "$bin_dir/coralcraft.exe" "$stage/coralcraft.exe"
  cp "$bin_dir/coralcraft-cli.exe" "$stage/coralcraft-cli.exe"
  cp "$ICONS_DIR/coralcraft.ico" "$stage/coralcraft.ico"
  copy_docs "$stage"

  # Patch coralcraft.exe PE Optional Header Subsystem to 2 (IMAGE_SUBSYSTEM_WINDOWS_GUI)
  # and verify COFF Machine matches x64 (0x8664) or ARM64 (0xAA64).
  python3 - "$stage/coralcraft.exe" "$stage/coralcraft-cli.exe" "$expected_machine" <<'PY'
import struct, sys

gui_exe, cli_exe, expected_machine = sys.argv[1], sys.argv[2], int(sys.argv[3])

with open(gui_exe, "r+b") as f:
    data = bytearray(f.read())
    pe_off = struct.unpack_from("<I", data, 0x3C)[0]
    machine = struct.unpack_from("<H", data, pe_off + 4)[0]
    assert machine == expected_machine, f"Expected machine 0x{expected_machine:X}, got 0x{machine:X}"
    struct.pack_into("<H", data, pe_off + 0x5C, 2)
    f.seek(0)
    f.write(data)

with open(cli_exe, "rb") as f:
    data = f.read()
    pe_off = struct.unpack_from("<I", data, 0x3C)[0]
    machine = struct.unpack_from("<H", data, pe_off + 4)[0]
    subsys = struct.unpack_from("<H", data, pe_off + 0x5C)[0]
    assert machine == expected_machine, f"Expected machine 0x{expected_machine:X}, got 0x{machine:X}"
    assert subsys == 3, f"Expected console subsystem 3, got {subsys}"
print(f"Verified PE32+ (0x{expected_machine:X}) headers: coralcraft.exe (GUI=2), coralcraft-cli.exe (CONSOLE=3)")
PY

  local out_zip="$DIST/coralcraft-$VERSION-windows-$win_arch-portable.zip"
  rm -f "$out_zip"
  python3 - "$work" "$folder" "$out_zip" <<'PY'
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
  echo "wrote $out_zip"
}

for a in "${ARCH_LIST[@]}"; do
  package_windows_arch "$a"
done

ls -lh "$DIST"/coralcraft-"$VERSION"-windows-*
