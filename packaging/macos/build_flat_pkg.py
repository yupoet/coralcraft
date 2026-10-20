#!/usr/bin/env python3
"""Builds a macOS Flat Package (`.pkg`) installer for CoralCraft.app.

On macOS (`Darwin`), invokes `/usr/bin/pkgbuild` directly.
On Linux cross-packaging hosts, constructs a compliant XAR 1.0 Flat Package
containing `PackageInfo` and `Payload` (`cpio -o -H odc | gzip -9`).
"""

import gzip
import hashlib
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib


def build_cpio_odc_gz(root_dir: str) -> tuple[bytes, int, int]:
    file_count = 0
    total_bytes = 0
    for base, dirs, files in os.walk(root_dir):
        file_count += len(dirs) + len(files)
        for fn in files:
            p = os.path.join(base, fn)
            if not os.path.islink(p):
                total_bytes += os.path.getsize(p)

    # Use system cpio -o -H odc if available
    proc = subprocess.run(
        ["sh", "-c", "find . | LC_ALL=C sort | cpio -o -H odc --quiet"],
        cwd=root_dir,
        capture_output=True,
        check=True,
    )
    gz_payload = gzip.compress(proc.stdout, compresslevel=9)
    return gz_payload, file_count, max(1, (total_bytes + 1023) // 1024)


def build_xar_flat_pkg(root_dir: str, version: str, out_pkg: str) -> None:
    if shutil.which("pkgbuild"):
        subprocess.run(
            [
                "pkgbuild",
                "--root",
                root_dir,
                "--identifier",
                "ai.storyteller.coralcraft.pkg",
                "--version",
                version,
                "--install-location",
                "/Applications",
                out_pkg,
            ],
            check=True,
        )
        return

    payload_gz, num_files, install_kbytes = build_cpio_odc_gz(root_dir)
    pkg_info_xml = (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<pkg-info overwrite-permissions="true" relocatable="false" '
        'identifier="ai.storyteller.coralcraft.pkg" '
        'postinstall-action="none" '
        f'version="{version}" '
        'format-version="2" '
        'generator-version="InstallCmds-831" '
        'install-location="/Applications" '
        'auth="root">\n'
        f'    <payload numberOfFiles="{num_files}" installKBytes="{install_kbytes}"/>\n'
        '    <bundle-version>\n'
        '        <bundle id="ai.storyteller.coralcraft" '
        f'CFBundleIdentifier="ai.storyteller.coralcraft" '
        'path="./CoralCraft.app" '
        f'CFBundleVersion="{version}"/>\n'
        '    </bundle-version>\n'
        '</pkg-info>\n'
    ).encode("utf-8")

    pkg_info_zlib = zlib.compress(pkg_info_xml, 9)
    pkg_info_sha1_extracted = hashlib.sha1(pkg_info_xml).hexdigest()
    pkg_info_sha1_archived = hashlib.sha1(pkg_info_zlib).hexdigest()

    payload_sha1 = hashlib.sha1(payload_gz).hexdigest()

    # Heap layout:
    # [0..20]: TOC SHA-1 checksum
    # [20..20+len(pkg_info_zlib)]: PackageInfo (zlib-compressed)
    # [20+len(pkg_info_zlib)..]: Payload (raw cpio.gz)
    pkg_info_offset = 20
    payload_offset = pkg_info_offset + len(pkg_info_zlib)

    toc_xml = f"""<?xml version="1.0" encoding="UTF-8"?>
<xar>
 <toc>
  <checksum style="sha1">
   <offset>0</offset>
   <size>20</size>
  </checksum>
  <creation-time>2026-10-20T00:00:00</creation-time>
  <file id="1">
   <name>PackageInfo</name>
   <type>file</type>
   <data>
    <length>{len(pkg_info_zlib)}</length>
    <offset>{pkg_info_offset}</offset>
    <size>{len(pkg_info_xml)}</size>
    <encoding style="application/x-gzip"/>
    <archived-checksum style="sha1">{pkg_info_sha1_archived}</archived-checksum>
    <extracted-checksum style="sha1">{pkg_info_sha1_extracted}</extracted-checksum>
   </data>
  </file>
  <file id="2">
   <name>Payload</name>
   <type>file</type>
   <data>
    <length>{len(payload_gz)}</length>
    <offset>{payload_offset}</offset>
    <size>{len(payload_gz)}</size>
    <encoding style="application/octet-stream"/>
    <archived-checksum style="sha1">{payload_sha1}</archived-checksum>
    <extracted-checksum style="sha1">{payload_sha1}</extracted-checksum>
   </data>
  </file>
 </toc>
</xar>
""".encode("utf-8")

    toc_zlib = zlib.compress(toc_xml, 9)
    toc_sha1_raw = hashlib.sha1(toc_zlib).digest()

    header = struct.pack(">4sHHQQI", b"xar!", 28, 1, len(toc_zlib), len(toc_xml), 1)
    with open(out_pkg, "wb") as f:
        f.write(header)
        f.write(toc_zlib)
        f.write(toc_sha1_raw)
        f.write(pkg_info_zlib)
        f.write(payload_gz)


if __name__ == "__main__":
    if len(sys.argv) != 4:
        print("usage: build_flat_pkg.py <root_dir> <version> <out_pkg>", file=sys.stderr)
        sys.exit(2)
    build_xar_flat_pkg(sys.argv[1], sys.argv[2], sys.argv[3])
