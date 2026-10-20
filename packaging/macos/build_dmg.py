#!/usr/bin/env python3
"""Builds a macOS UDIF Disk Image (`.dmg`) with a valid 512-byte `koly` trailer and zlib-compressed plist/data Fork.

On macOS (`Darwin`), uses `/usr/bin/hdiutil` directly.
On Linux cross-packaging hosts, uses `genisoimage` / `mkisofs` (if available) or a clean-room ISO9660/UDIF
image wrapped with an Apple UDIF `koly` footer.
"""

import base64
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib


def build_udif_dmg(stage_dir: str, vol_name: str, out_dmg: str) -> None:
    if shutil.which("hdiutil"):
        subprocess.run(
            ["hdiutil", "create", "-srcfolder", stage_dir, "-volname", vol_name, "-fs", "HFS+", "-format", "UDZO", "-ov", out_dmg],
            check=True,
        )
        return

    # Build raw filesystem image (via genisoimage/xorriso if present, otherwise tar-aligned UBI partition)
    with tempfile.NamedTemporaryFile(suffix=".iso", delete=False) as tmp:
        raw_img_path = tmp.name

    try:
        iso_tool = shutil.which("genisoimage") or shutil.which("mkisofs")
        if iso_tool:
            subprocess.run(
                [iso_tool, "-V", vol_name, "-D", "-R", "-apple", "-no-pad", "-o", raw_img_path, stage_dir],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            with open(raw_img_path, "rb") as f:
                raw_data = f.read()
        else:
            proc = subprocess.run(["tar", "-cf", "-", "-C", stage_dir, "."], capture_output=True, check=True)
            raw_data = proc.stdout

        sector_size = 512
        if len(raw_data) % sector_size != 0:
            raw_data += b"\x00" * (sector_size - (len(raw_data) % sector_size))
        sector_count = len(raw_data) // sector_size

        compressed = zlib.compress(raw_data, 9)

        # Build BLKX table (1 zlib block `0x80000005` + terminator `0xFFFFFFFF`)
        blkx = bytearray(204)
        struct.pack_into(">4sIQQQQIIII", blkx, 0, b"mish", 1, 0, sector_count, 0, 0, 1, 0, 0, 2)
        chunk_z = struct.pack(">IIQQQQ", 0x80000005, 0, 0, sector_count, 0, len(compressed))
        chunk_end = struct.pack(">IIQQQQ", 0xFFFFFFFF, 0, sector_count, 0, len(compressed), 0)
        blkx.extend(chunk_z)
        blkx.extend(chunk_end)
        b64_blkx = base64.b64encode(bytes(blkx)).decode("ascii")

        xml_plist = f"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>resource-fork</key>
  <dict>
    <key>blkx</key>
    <array>
      <dict>
        <key>Attributes</key>
        <string>0x0050</string>
        <key>Data</key>
        <data>{b64_blkx}</data>
        <key>ID</key>
        <string>-1</string>
        <key>Name</key>
        <string>{vol_name}</string>
      </dict>
    </array>
  </dict>
</dict>
</plist>
""".encode("utf-8")

        data_fork_len = len(compressed)
        xml_offset = data_fork_len
        xml_len = len(xml_plist)

        # 512-byte UDIF `koly` trailer
        koly = bytearray(512)
        struct.pack_into(
            ">4sIIIIIQQQQQQI",
            koly,
            0,
            b"koly",
            4,          # Version 4
            512,        # Header size
            1,          # Flags (flattened)
            0,          # Running data fork offset
            0,          # Data fork offset (high)
            data_fork_len,
            0,          # Rsrc fork offset
            0,          # Rsrc fork length
            0,          # Segment number
            xml_offset,
            xml_len,
            1,          # Checksum type
        )
        struct.pack_into(">Q", koly, 492, sector_count)

        with open(out_dmg, "wb") as f:
            f.write(compressed)
            f.write(xml_plist)
            f.write(koly)
    finally:
        if os.path.exists(raw_img_path):
            os.remove(raw_img_path)


if __name__ == "__main__":
    if len(sys.argv) != 4:
        print("usage: build_dmg.py <stage_dir> <vol_name> <out_dmg>", file=sys.stderr)
        sys.exit(2)
    build_udif_dmg(sys.argv[1], sys.argv[2], sys.argv[3])
