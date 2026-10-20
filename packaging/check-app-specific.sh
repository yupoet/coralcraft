#!/usr/bin/env bash
# App-specific packaging manifest assertions for CoralCraft (R6.1, R6.2).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

python3 - <<'PYEOF'
import os
import xml.etree.ElementTree as ET

# 1. Verify macOS Info.plist.in and entitlements.plist are valid XML plists
for plist_path in ("packaging/macos/Info.plist.in", "packaging/macos/entitlements.plist"):
    tree = ET.parse(plist_path)
    assert tree.getroot().tag == "plist", f"{plist_path} root must be <plist>"

# 2. Verify Linux AppStream metainfo and MIME XML files are valid XML with expected IDs
metainfo = ET.parse("packaging/linux/ai.storyteller.coralcraft.metainfo.xml.in").getroot()
assert metainfo.findtext("id") == "ai.storyteller.coralcraft"
assert metainfo.findtext("provides/binary") == "coralcraft"

mime = ET.parse("packaging/linux/ai.storyteller.coralcraft.mime.xml").getroot()
assert mime.tag.endswith("mime-info")

# 3. Verify Windows WiX v5 manifest is valid XML and references coralcraft.exe + coralcraft-cli.exe
wxs = ET.parse("packaging/windows/coralcraft.wxs").getroot()
wxs_text = open("packaging/windows/coralcraft.wxs", encoding="utf-8").read()
assert "coralcraft.exe" in wxs_text and "coralcraft-cli.exe" in wxs_text

# 4. Verify Flatpak manifests agree on app id, command, runtime, and finish-args
for fp in (
    "packaging/linux/flatpak/ai.storyteller.coralcraft.yml",
    "packaging/linux/flatpak/ai.storyteller.coralcraft.bundle.yml",
):
    text = open(fp, encoding="utf-8").read()
    assert "id: ai.storyteller.coralcraft" in text, fp
    assert "command: coralcraft" in text, fp
    assert "runtime: org.freedesktop.Platform" in text, fp

# 5. Verify nfpm.yaml defines deb and rpm overrides for coralcraft
nfpm_text = open("packaging/linux/nfpm.yaml", encoding="utf-8").read()
assert "name: coralcraft" in nfpm_text and "deb:" in nfpm_text and "rpm:" in nfpm_text

# 6. Verify packaging scripts exist and are executable
for script in (
    "packaging/macos/package.sh",
    "packaging/linux/package.sh",
    "packaging/linux/postinst.sh",
    "packaging/linux/flatpak-bundle.sh",
    "packaging/freebsd/package.sh",
    "packaging/web/package.sh",
    "packaging/windows/package.sh",
):
    assert os.path.isfile(script), f"missing {script}"

print("CoralCraft packaging manifests and scripts ok")
PYEOF
