#!/usr/bin/env python3
"""Combines an x86_64 Mach-O and an arm64 Mach-O into a Universal2 Fat Mach-O binary (`0xCAFEBABE`)."""

import struct
import sys

FAT_MAGIC = 0xCAFEBABE
CPU_TYPE_X86_64 = 0x01000007
CPU_SUBTYPE_X86_64_ALL = 3
CPU_TYPE_ARM64 = 0x0100000C
CPU_SUBTYPE_ARM64_ALL = 0
ALIGN_POW2 = 14
ALIGN_BYTES = 1 << ALIGN_POW2  # 16384 bytes (page alignment required by Apple Silicon)


def align_up(val: int, align: int) -> int:
    return (val + align - 1) & ~(align - 1)


def make_universal(x86_64_path: str, arm64_path: str, out_path: str) -> None:
    with open(x86_64_path, "rb") as f:
        x86_bytes = f.read()
    with open(arm64_path, "rb") as f:
        arm_bytes = f.read()

    off_x86 = ALIGN_BYTES
    off_arm = align_up(off_x86 + len(x86_bytes), ALIGN_BYTES)

    header = bytearray()
    header.extend(struct.pack(">II", FAT_MAGIC, 2))
    header.extend(struct.pack(">IIIII", CPU_TYPE_X86_64, CPU_SUBTYPE_X86_64_ALL, off_x86, len(x86_bytes), ALIGN_POW2))
    header.extend(struct.pack(">IIIII", CPU_TYPE_ARM64, CPU_SUBTYPE_ARM64_ALL, off_arm, len(arm_bytes), ALIGN_POW2))

    out = bytearray(off_arm + len(arm_bytes))
    out[0 : len(header)] = header
    out[off_x86 : off_x86 + len(x86_bytes)] = x86_bytes
    out[off_arm : off_arm + len(arm_bytes)] = arm_bytes

    with open(out_path, "wb") as f:
        f.write(out)


if __name__ == "__main__":
    if len(sys.argv) != 4:
        print("usage: make_universal_macho.py <x86_64_bin> <arm64_bin> <out_universal_bin>", file=sys.stderr)
        sys.exit(2)
    make_universal(sys.argv[1], sys.argv[2], sys.argv[3])
