#!/usr/bin/env python3
"""Render the CoralCraft app icon (original artwork) to PNG sizes and keep the SVG source.

Design: three stylized coral polyp fingers rising from the seabed on a deep-ocean rounded
square, with small polyp dots. Everything is simple original geometry; no third-party or
upstream artwork is traced or embedded.

Usage: python3 packaging/icons-coral.py   (writes assets/app-icon/*)
"""

from __future__ import annotations

import math
from pathlib import Path

from PIL import Image, ImageDraw

# Palette (original): deep ocean floor, three coral shades, pale polyp accents.
BG_TOP = (7, 46, 59)
BG_BOTTOM = (13, 27, 42)
CORAL = (255, 111, 97)
CORAL_MID = (247, 143, 113)
CORAL_LIGHT = (250, 176, 147)
ACCENT = (255, 224, 204)

OUT = Path(__file__).resolve().parent.parent / "assets" / "app-icon"


def lerp(a, b, t):
    return tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(3))


def draw_icon(size: int, rounded: bool, path: Path) -> None:
    ss = 4  # supersample for smooth edges
    s = size * ss
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    # Background: vertical deep-ocean gradient on a rounded square.
    grad = Image.new("RGBA", (s, s))
    gd = ImageDraw.Draw(grad)
    for y in range(s):
        gd.line([(0, y), (s, y)], fill=lerp(BG_TOP, BG_BOTTOM, y / s) + (255,))
    mask = Image.new("L", (s, s), 0)
    md = ImageDraw.Draw(mask)
    radius = int(s * 0.225) if rounded else 0
    md.rounded_rectangle([0, 0, s - 1, s - 1], radius=radius, fill=255)
    img.paste(grad, (0, 0), mask)
    d = ImageDraw.Draw(img)

    def finger(cx: float, base: float, top: float, w: float, color) -> None:
        """A coral finger: a pill from the base up to `top`, slightly tapered by two pills."""
        r = w / 2
        d.rounded_rectangle([cx - r, top, cx + r, base], radius=r, fill=color)

    # Seabed line.
    ground = s * 0.82
    d.line([(s * 0.14, ground), (s * 0.86, ground)], fill=lerp(BG_BOTTOM, CORAL_MID, 0.35) + (255,), width=int(s * 0.035))

    # Three coral fingers, tallest in the middle-left, leaning composition.
    w = s * 0.115
    finger(s * 0.38, ground + s * 0.02, s * 0.30, w, CORAL)
    finger(s * 0.56, ground + s * 0.02, s * 0.22, w * 0.92, CORAL_MID)
    finger(s * 0.71, ground + s * 0.02, s * 0.38, w * 0.78, CORAL_LIGHT)

    # Polyp dots on and around the fingers.
    for (fx, fy, fr, col) in [
        (0.38, 0.33, 0.032, ACCENT),
        (0.435, 0.44, 0.026, ACCENT),
        (0.56, 0.26, 0.028, ACCENT),
        (0.71, 0.42, 0.030, ACCENT),
        (0.755, 0.55, 0.022, ACCENT),
        (0.30, 0.55, 0.020, CORAL_LIGHT),
    ]:
        x, y, r = fx * s, fy * s, fr * s
        d.ellipse([x - r, y - r, x + r, y + r], fill=col + (255,))

    img = img.resize((size, size), Image.LANCZOS)
    OUT.mkdir(parents=True, exist_ok=True)
    img.save(path)


SVG = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <!-- CoralCraft app icon source (original artwork). Rendered to PNG by packaging/icons-coral.py -->
  <defs>
    <linearGradient id="sea" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="rgb(7,46,59)"/>
      <stop offset="1" stop-color="rgb(13,27,42)"/>
    </linearGradient>
  </defs>
  <rect width="512" height="512" rx="115" fill="url(#sea)"/>
  <line x1="72" y1="420" x2="440" y2="420" stroke="rgb(116,82,84)" stroke-width="18" stroke-linecap="round"/>
  <rect x="160" y="154" width="59" height="282" rx="29.5" fill="rgb(255,111,97)"/>
  <rect x="251" y="215" width="54" height="221" rx="27" fill="rgb(247,143,113)"/>
  <rect x="337" y="112" width="46" height="324" rx="23" fill="rgb(250,176,147)"/>
  <circle cx="194" cy="169" stroke="none" fill="rgb(255,224,204)" r="16"/>
  <circle cx="223" cy="225" fill="rgb(255,224,204)" r="13"/>
  <circle cx="286" cy="133" fill="rgb(255,224,204)" r="14"/>
  <circle cx="364" cy="215" fill="rgb(255,224,204)" r="15"/>
  <circle cx="386" cy="282" fill="rgb(255,224,204)" r="11"/>
  <circle cx="154" cy="282" fill="rgb(250,176,147)" r="10"/>
</svg>
"""


def main() -> None:
    draw_icon(128, True, OUT / "coralcraft-128.png")
    draw_icon(256, True, OUT / "coralcraft-256.png")
    draw_icon(512, False, OUT / "coralcraft-macos-512.png")
    (OUT / "coralcraft.svg").write_text(SVG)
    print(f"wrote icons to {OUT}")


if __name__ == "__main__":
    main()
