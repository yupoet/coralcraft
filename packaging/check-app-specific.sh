#!/usr/bin/env bash
# App-specific package assertions retained alongside shared packaging lint.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

python3 - <<'PYEOF'
import yaml
manifests = []
for name in ("io.github.yupoet.coralcraft.yml", "io.github.yupoet.coralcraft.bundle.yml"):
    with open("packaging/linux/flatpak/" + name) as source:
        manifest = yaml.safe_load(source)
    assert manifest["id"] == "io.github.yupoet.coralcraft" and manifest["command"] == "coralcraft", name
    manifests.append(manifest)
for key in ("runtime", "runtime-version", "sdk", "finish-args"):
    assert manifests[0][key] == manifests[1][key], f"Flatpak manifests disagree on {key}"
from pathlib import Path
desktop = Path("packaging/linux/io.github.yupoet.coralcraft.desktop").read_text()
for entry in ("Name=CoralCraft", "Exec=coralcraft %F", "TryExec=coralcraft"):
    assert entry in desktop, f"desktop entry missing {entry}"
portable = Path("packaging/windows/portable.txt").read_text()
for expected in ("CoralCraft portable mode", "coralcraft.exe", "CoralCraftData", "%APPDATA%\\CoralCraft"):
    assert expected in portable, f"portable instructions missing {expected}"
assert "VectorCraft" not in portable and "vectorcraft.exe" not in portable
print("CoralCraft package identity ok")
PYEOF
