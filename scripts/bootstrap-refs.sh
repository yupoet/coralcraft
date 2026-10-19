#!/usr/bin/env bash
# Reconstructs external reference repositories under refs/.
# Clean-room rule: refs/ contents are never committed (.gitignore); only refs/REFS.md and this script
# are tracked so a fresh workspace can restore the reference repositories with a single command.
#
# Usage:
#   scripts/bootstrap-refs.sh            # Checkout exact pinned commits from refs/REFS.md §D
#   scripts/bootstrap-refs.sh --latest   # Clone latest branch tips (update REFS.md §D afterward)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REFS="$ROOT/refs"
mkdir -p "$REFS"
cd "$REFS"

LATEST="${1:-}"
if [ -n "$LATEST" ] && [ "$LATEST" != "--latest" ]; then
  echo "usage: scripts/bootstrap-refs.sh [--latest]" >&2
  exit 2
fi

# name|url|pinned-sha (must match refs/REFS.md §D; full 40-char SHA required for git fetch --depth 1)
ENTRIES=(
  "vectorcraft|https://github.com/storytold/vectorcraft.git|a466625c8987550635cda951ebf36e08e1e80a56"
  "designcraft|https://github.com/storytold/designcraft.git|443ea4640ca368fcf0241b04813bd3817acd5df8"
  "cadcraft|https://github.com/storytold/cadcraft.git|0b32c2e6a14c761835ea62c01e7b20845971d781"
  "craft-libs|https://github.com/storytold/craft-libs.git|ae2f796b673f29cf3161f36eb6a8933f0a3fe028"
  "craft-common-actions|https://github.com/storytold/craft-common-actions.git|d7bf8f06e138618890d0ebe8a84a18b010bf3f66"
  "craft-fonts|https://github.com/storytold/craft-fonts.git|d1cf47d05e04426615fbda1d637b9ca6b1e7af35"
  "libcdr|https://github.com/LibreOffice/libcdr.git|22c396810e72d40b94f78c192eb06e6d09bc34a0"
  "cdr-spec|https://github.com/photopea/CDR-specification.git|f91b3c7e8c812b28473f6399e95a624a72a3d6b8"
  "uniconvertor|https://github.com/sk1project/uniconvertor.git|973d5b6f8fccce7e7bf9bc88e91bc80f9f9d9472"
  "rea|https://github.com/cyberkaida/reverse-engineering-assistant.git|a05a6b0a18649efd6d74ec8447bdadf8d468c365"
  "morluto-rea|https://github.com/morluto/rea.git|ba866a26ecbe179a717e746cbc0d1605ae616ad5"
)

for e in "${ENTRIES[@]}"; do
  IFS='|' read -r name url pin <<<"$e"
  if [ -d "$name/.git" ]; then
    echo "== skipping existing: $name"
    continue
  fi
  if [ "$LATEST" = "--latest" ]; then
    echo "== cloning latest tip: $name"
    git clone --depth 1 --single-branch "$url" "$name"
  else
    echo "== restoring pinned commit: $name @ ${pin}"
    git init -q "$name"
    git -C "$name" remote add origin "$url"
    if git -C "$name" fetch -q --depth 1 origin "$pin"; then
      git -C "$name" checkout -q --detach FETCH_HEAD
    else
      echo "   !! failed to fetch pinned commit (remote history may have changed); falling back to latest tip" >&2
      rm -rf "$name"
      git clone --depth 1 --single-branch "$url" "$name"
    fi
  fi
done

echo "== done: $(ls -d */ 2>/dev/null | wc -l | tr -d ' ') reference repositories"
echo "== clean-room reminder: uniconvertor is AGPL-3.0 — agents must never read its source code (see refs/REFS.md §C)"
