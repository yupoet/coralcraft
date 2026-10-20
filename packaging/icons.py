#!/usr/bin/env python3
"""Clean-room vector-derived icon generator for CoralCraft desktop packages.

Generates:
  - Linux hicolor PNG icons (16x16, 32x32, 48x48, 64x64, 128x128, 256x256, 512x512)
  - macOS Apple Icon Image (`CoralCraft.icns`)
  - Windows Icon (`coralcraft.ico`)
"""

import math
import os
import struct
import sys
import zlib


def encode_png_rgba(width: int, height: int, rgba: bytes) -> bytes:
    def chunk(tag: bytes, data: bytes) -> bytes:
        crc = zlib.crc32(tag + data) & 0xFFFFFFFF
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", crc)

    raw = bytearray()
    stride = width * 4
    for y in range(height):
        raw.append(0)
        raw.extend(rgba[y * stride : (y + 1) * stride])

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    idat = zlib.compress(bytes(raw), 9)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")


def render_icon_rgba(size: int) -> bytes:
    buf = bytearray(size * size * 4)
    cx = (size - 1) * 0.5
    cy = (size - 1) * 0.5
    r_card = size * 0.44
    corner = size * 0.18

    for y in range(size):
        for x in range(size):
            dx = abs(x - cx) - (r_card - corner)
            dy = abs(y - cy) - (r_card - corner)
            dist = math.hypot(max(dx, 0.0), max(dy, 0.0)) + min(max(dx, dy), 0.0) - corner
            idx = (y * size + x) * 4
            if dist > 0.5:
                buf[idx : idx + 4] = b"\x00\x00\x00\x00"
                continue

            # Coral-orange to deep-magenta diagonal gradient background
            t = (x + y) / max(2.0 * size, 1.0)
            r = int(245 * (1.0 - t) + 195 * t)
            g = int(105 * (1.0 - t) + 42 * t)
            b = int(55 * (1.0 - t) + 115 * t)

            # Geometric pen nib + Bezier ring motif in paper white
            nx = (x - cx) / (size * 0.5)
            ny = (y - cy) / (size * 0.5)
            ring = abs(math.hypot(nx, ny) - 0.56)
            nib = abs(nx * 0.85 + ny * 0.5) + abs(nx * 0.5 - ny * 0.85) * 0.55

            if ring < 0.065 or (nib < 0.34 and ny > -0.35 and math.hypot(nx, ny) > 0.14):
                r, g, b = 252, 248, 240

            alpha = 255 if dist < -0.5 else int(255 * (0.5 - dist))
            buf[idx] = max(0, min(255, r))
            buf[idx + 1] = max(0, min(255, g))
            buf[idx + 2] = max(0, min(255, b))
            buf[idx + 3] = max(0, min(255, alpha))

    return bytes(buf)


def build_icns(png_by_size: dict) -> bytes:
    # Standard PNG-backed ICNS chunk types supported by macOS 10.7+
    entries = [
        (b"icp4", 16),
        (b"icp5", 32),
        (b"icp6", 64),
        (b"ic07", 128),
        (b"ic08", 256),
        (b"ic09", 512),
    ]
    body = bytearray()
    for ostype, sz in entries:
        data = png_by_size[sz]
        body.extend(ostype)
        body.extend(struct.pack(">I", len(data) + 8))
        body.extend(data)
    return b"icns" + struct.pack(">I", len(body) + 8) + bytes(body)


def build_ico(png_by_size: dict) -> bytes:
    sizes = [16, 32, 48, 64, 128, 256]
    header = struct.pack("<HHH", 0, 1, len(sizes))
    directory = bytearray()
    payload = bytearray()
    offset = 6 + 16 * len(sizes)
    for sz in sizes:
        data = png_by_size[sz]
        w_byte = 0 if sz >= 256 else sz
        h_byte = 0 if sz >= 256 else sz
        directory.extend(struct.pack("<BBBBHHII", w_byte, h_byte, 0, 0, 1, 32, len(data), offset))
        payload.extend(data)
        offset += len(data)
    return header + bytes(directory) + bytes(payload)


def main() -> None:
    out_dir = sys.argv[1] if len(sys.argv) > 1 else "target/package-icons"
    app_id = "ai.storyteller.coralcraft"
    sizes = [16, 32, 48, 64, 128, 256, 512]
    png_by_size = {}

    for sz in sizes:
        rgba = render_icon_rgba(sz)
        png = encode_png_rgba(sz, sz, rgba)
        png_by_size[sz] = png
        hicolor_dir = os.path.join(out_dir, "hicolor", f"{sz}x{sz}", "apps")
        os.makedirs(hicolor_dir, exist_ok=True)
        with open(os.path.join(hicolor_dir, f"{app_id}.png"), "wb") as f:
            f.write(png)

    os.makedirs(out_dir, exist_ok=True)
    with open(os.path.join(out_dir, "CoralCraft.icns"), "wb") as f:
        f.write(build_icns(png_by_size))
    with open(os.path.join(out_dir, "coralcraft.ico"), "wb") as f:
        f.write(build_ico(png_by_size))


if __name__ == "__main__":
    main()
