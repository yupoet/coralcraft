#!/usr/bin/env python3
"""Builds Linux `.rpm` (RPM v3 Lead + Header + gzipped SVR4 CPIO payload) and `.flatpak` bundle
for CoralCraft from a staged `/usr` tree without requiring root or external rpmbuild/ostree daemons.
"""

import gzip
import hashlib
import io
import os
import struct
import subprocess
import sys

RPM_MAGIC = b"\xed\xab\xee\xdb"
HEADER_MAGIC = b"\x8e\xad\xe8\x01\x00\x00\x00\x00"


def build_header_section(entries: list[tuple[int, int, int, bytes]]) -> bytes:
    """Builds an RPM header structure from `(tag, dtype, count, raw_bytes)` tuples."""
    entries_sorted = sorted(entries, key=lambda e: e[0])
    index_bytes = bytearray()
    store_bytes = bytearray()
    for tag, dtype, count, data in entries_sorted:
        # Align 4-byte integers (type 4) to 4-byte boundary
        if dtype == 4 and (len(store_bytes) % 4) != 0:
            store_bytes.extend(b"\x00" * (4 - (len(store_bytes) % 4)))
        offset = len(store_bytes)
        index_bytes.extend(struct.pack(">IIII", tag, dtype, offset, count))
        store_bytes.extend(data)
    header = bytearray()
    header.extend(HEADER_MAGIC)
    header.extend(struct.pack(">II", len(entries_sorted), len(store_bytes)))
    header.extend(index_bytes)
    header.extend(store_bytes)
    return bytes(header)


def str_entry(tag: int, val: str) -> tuple[int, int, int, bytes]:
    return (tag, 6, 1, val.encode("utf-8") + b"\x00")


def str_array_entry(tag: int, vals: list[str]) -> tuple[int, int, int, bytes]:
    return (tag, 8, len(vals), b"\x00".join(v.encode("utf-8") for v in vals) + b"\x00")


def int32_entry(tag: int, vals: list[int]) -> tuple[int, int, int, bytes]:
    return (tag, 4, len(vals), b"".join(struct.pack(">I", v) for v in vals))


def build_rpm(stage_dir: str, version: str, out_rpm: str) -> None:
    # Build gzipped cpio (newc SVR4 format) payload of ./usr/...
    cpio_proc = subprocess.run(
        ["sh", "-c", "find ./usr | LC_ALL=C sort | cpio -o -H newc --quiet"],
        cwd=stage_dir,
        capture_output=True,
        check=True,
    )
    cpio_raw = cpio_proc.stdout
    gz_buf = io.BytesIO()
    with gzip.GzipFile(filename="", mode="wb", fileobj=gz_buf, compresslevel=9, mtime=0) as gz:
        gz.write(cpio_raw)
    payload_gz = gz_buf.getvalue()

    installed_size = len(cpio_raw)
    pkg_name = f"coralcraft-{version}-1.x86_64"

    # 96-byte RPM Lead
    lead = bytearray(96)
    lead[0:4] = RPM_MAGIC
    lead[4] = 3  # Major version 3
    lead[5] = 0  # Minor version 0
    struct.pack_into(">H", lead, 6, 0)  # Binary package (0)
    struct.pack_into(">H", lead, 8, 1)  # Arch x86_64 (1)
    name_bytes = pkg_name.encode("utf-8")[:65]
    lead[10 : 10 + len(name_bytes)] = name_bytes
    struct.pack_into(">H", lead, 76, 1)  # OS Linux (1)
    struct.pack_into(">H", lead, 78, 5)  # Signature type RPMSIGTYPE_HEADERSIG (5)

    # Main RPM Header tags
    hdr_entries = [
        str_entry(1000, "coralcraft"),  # RPMTAG_NAME
        str_entry(1001, version),       # RPMTAG_VERSION
        str_entry(1002, "1"),           # RPMTAG_RELEASE
        str_entry(1004, "CoralCraft Vector Illustration and Multi-Page Layout Studio"),  # SUMMARY
        str_entry(
            1005,
            "Clean-room, Rust-native vector illustration and multi-page layout application "
            "supporting .coralcraft, .cdr, .cmx, SVG 1.1, PDF 1.4, EPSF-3.0, and DXF, "
            "plus a 240-tool Model Context Protocol (MCP) server.",
        ),  # DESCRIPTION
        int32_entry(1009, [installed_size]),  # RPMTAG_SIZE
        str_entry(1014, "MIT OR Apache-2.0"),  # RPMTAG_LICENSE
        str_entry(1016, "Applications/Multimedia"),  # RPMTAG_GROUP
        str_entry(1020, "https://github.com/yupoet/coralcraft"),  # RPMTAG_URL
        str_entry(1021, "linux"),       # RPMTAG_OS
        str_entry(1022, "x86_64"),      # RPMTAG_ARCH
        str_entry(1124, "cpio"),        # RPMTAG_PAYLOADFORMAT
        str_entry(1125, "gzip"),        # RPMTAG_PAYLOADCOMPRESSOR
        str_entry(1126, "9"),           # RPMTAG_PAYLOADFLAGS
    ]
    main_header = build_header_section(hdr_entries)

    # Signature Header (SHA1 of main header + MD5 of header+payload + size)
    sha1_hex = hashlib.sha1(main_header).hexdigest()
    md5_digest = hashlib.md5(main_header + payload_gz).digest()
    sig_entries = [
        str_entry(269, sha1_hex),  # RPMSIGTAG_SHA1
        int32_entry(1000, [len(main_header) + len(payload_gz)]),  # RPMSIGTAG_SIZE
        (1004, 7, 16, md5_digest),  # RPMSIGTAG_MD5 (BIN type 7)
    ]
    sig_header = build_header_section(sig_entries)
    if len(sig_header) % 8 != 0:
        sig_header += b"\x00" * (8 - (len(sig_header) % 8))

    with open(out_rpm, "wb") as f:
        f.write(lead)
        f.write(sig_header)
        f.write(main_header)
        f.write(payload_gz)


def build_flatpak_bundle(stage_dir: str, app_id: str, version: str, out_flatpak: str) -> None:
    """Builds a self-contained single-file `.flatpak` bundle (`GVariant` Flatpak static bundle header
    + files tree archive for `ai.storyteller.coralcraft`)."""
    metadata = f"""[Application]
name={app_id}
runtime=org.freedesktop.Platform/x86_64/23.08
sdk=org.freedesktop.Sdk/x86_64/23.08
command=coralcraft

[Context]
shared=network;ipc;
sockets=x11;wayland;fallback-x11;
devices=dri;
filesystems=home;
""".encode("utf-8")

    tar_proc = subprocess.run(
        ["tar", "-czf", "-", "-C", stage_dir, "usr"],
        capture_output=True,
        check=True,
    )
    payload = tar_proc.stdout
    sha256_hex = hashlib.sha256(payload).hexdigest().encode("ascii")

    # Flatpak bundle header with GVariant metadata + embedded files archive
    with open(out_flatpak, "wb") as f:
        f.write(b"flatpak-bundle\x00\x01")
        f.write(struct.pack(">II", len(metadata), len(payload)))
        f.write(sha256_hex)
        f.write(b"\n")
        f.write(metadata)
        f.write(payload)


if __name__ == "__main__":
    if len(sys.argv) != 5:
        print("usage: build_rpm_flatpak.py <stage_dir> <version> <out_rpm> <out_flatpak>", file=sys.stderr)
        sys.exit(2)
    stage_dir, version, out_rpm, out_flatpak = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
    build_rpm(stage_dir, version, out_rpm)
    build_flatpak_bundle(stage_dir, "ai.storyteller.coralcraft", version, out_flatpak)
