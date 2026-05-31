#!/usr/bin/env python3
"""Flatten iOS app-icon PNGs to opaque RGB (strip the alpha channel).

App Store Connect rejects an upload whose 1024px marketing icon has an
alpha channel ("Invalid large app icon ... can't be transparent or
contain an alpha channel"), even when every pixel is opaque.
``cargo tauri icon`` emits RGBA for the iOS set, so this composites each
icon over the app's tile colour and rewrites it as 3-channel RGB.

Only the iOS icons are touched; the desktop ``.icns``/``.ico`` and the
web favicon keep their transparent rounded corners (iOS rounds its icons
itself, so it wants a full opaque square).

Run after ``cargo tauri icon`` (the ``just ios-init`` recipe does this).
Requires Pillow (``python3 -m pip install Pillow``).
"""

from __future__ import annotations

import glob
import os
import sys

try:
    from PIL import Image
except ImportError:  # reported to the user in main()
    Image = None

# The app's dark tile colour (matches icon-manifest.json `bg_color`).
BG = (0x12, 0x2E, 0x38)

# Committed iOS source icons, plus the generated Xcode asset catalogue
# (the set the build actually bundles). Globs are repo-root relative.
PATTERNS = [
    "src-tauri/icons/ios/*.png",
    "src-tauri/gen/apple/**/AppIcon.appiconset/*.png",
]


def flatten(path: str) -> bool:
    """Rewrite ``path`` as opaque RGB if it has alpha.

    Returns ``True`` when the file was changed.
    """
    with Image.open(path) as img:
        if "A" not in img.getbands():
            return False  # already alpha-free
        rgba = img.convert("RGBA")
        flat = Image.new("RGB", rgba.size, BG)
        flat.paste(rgba, mask=rgba.split()[3])  # composite over BG
    tmp = path + ".tmp"
    flat.save(tmp, format="PNG")
    os.replace(tmp, path)  # atomic in-place replace
    return True


def main() -> int:
    """Flatten every iOS app-icon PNG under the repo to opaque RGB."""
    if Image is None:
        sys.stderr.write(
            "error: Pillow is required to flatten the iOS icons.\n"
            "       install it with: python3 -m pip install Pillow\n"
        )
        return 1

    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    changed = 0
    seen = 0
    for pattern in PATTERNS:
        full = os.path.join(repo_root, pattern)
        for path in glob.glob(full, recursive=True):
            seen += 1
            if flatten(path):
                changed += 1
                print(f"  flattened {os.path.relpath(path, repo_root)}")

    if seen == 0:
        print("no iOS icons found (run `just ios-init` first)")
    else:
        print(f"iOS icons: {changed} flattened, {seen - changed} already RGB")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
