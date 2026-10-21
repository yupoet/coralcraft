#!/usr/bin/env bash
# Regenerate every CoralCraft app icon (original artwork; see packaging/icons-coral.py for the
# design). Replaces the upstream pipeline, which rendered VectorCraft's own icon artwork.
set -euo pipefail
cd "$(dirname "$0")/.."
python3 packaging/icons-coral.py
