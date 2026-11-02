"""Writes a calibration card: a 1920 x 1080 picture to put through another
program's effects and read back exactly, in place of screenshots.

- A Hald CLUT of level 8 (512 x 512 at 32, 32): every color of a 64-step
  lattice once, so the effect's output there is its whole color table
  (the industry's way to capture a look from any program).
- Ramps, a step each value (4 px wide): gray, then red, green, blue,
  cyan, magenta and yellow from black to full.
- For effects that work on neighbors (blurs, sharpening, noise): a slanted
  edge (ISO 12233's way to measure sharpness), single white pixels every
  32 px (each one's spread), one-pixel lines, a zone plate (a ring
  sweep, every frequency) and flat grays of 25, 50 and 75%.
- Corner marks: white squares 8 px in, to check nothing moved or scaled.

Usage, from the repository's root (needs numpy and Pillow):

    python tools/calibration_card.py card.png

In the other program: a 1920 x 1080 project, the card on the timeline (at
its size: nothing scaled), the effect, then the frame saved at full
quality as PNG (or rendered as a PNG / EXR sequence). Without the effect
it must come back unchanged.
"""
import sys

import numpy as np
from PIL import Image

W, H = 1920, 1080
HALD = (32, 32)
RAMPS = (576, 32)
SPATIAL = (576, 352)


def hald(level=8):
    n = level * level
    side = level ** 3
    i = np.arange(side * side)
    rgb = np.stack([i % n, (i // n) % n, i // (n * n)], -1)
    return np.round(rgb * 255.0 / (n - 1)).astype(np.uint8).reshape(side, side, 3)


def ramps():
    v = np.repeat(np.arange(256), 4)  # 1024 wide
    rows = []
    for c in ((1, 1, 1), (1, 0, 0), (0, 1, 0), (0, 0, 1), (0, 1, 1), (1, 0, 1), (1, 1, 0)):
        row = np.stack([v * c[0], v * c[1], v * c[2]], -1).astype(np.uint8)
        rows.append(np.repeat(row[None], 36, 0))
        rows.append(np.zeros((8, 1024, 3), np.uint8))
    return np.concatenate(rows, 0)


def spatial():
    out = np.zeros((700, 1312, 3), np.uint8)
    # A slanted edge: dark left, light right, 5 degrees off vertical.
    y, x = np.mgrid[0:300, 0:300]
    edge = (x - 150) - np.tan(np.radians(5)) * (y - 150) > 0
    out[0:300, 0:300] = np.where(edge[..., None], 200, 50)
    # Single white pixels every 32 px on black.
    out[16:300:32, 332:632:32] = 255
    # One-pixel lines, vertical then horizontal, spaced 1 to 8 px apart.
    x0 = 664
    for gap in range(1, 9):
        for k in range(4):
            out[0:140, x0] = 255
            x0 += gap + 1
        x0 += 8
    y0 = 160
    for gap in range(1, 9):
        for k in range(4):
            out[y0, 664:900] = 255
            y0 += gap + 1
        y0 += 4
    # A zone plate: rings whose frequency rises outward.
    y, x = np.mgrid[-150:150, -150:150].astype(float)
    zone = 0.5 + 0.5 * np.cos(np.pi * (x * x + y * y) / 300.0)
    out[0:300, 1000:1300] = np.round(zone * 255)[..., None].astype(np.uint8)
    # Flat grays: 25, 50 and 75 %.
    for k, g in enumerate((64, 128, 191)):
        out[340:640, k * 320:k * 320 + 300] = g
    return out


def main(path):
    card = np.zeros((H, W, 3), np.uint8)
    hx, hy = HALD
    card[hy:hy + 512, hx:hx + 512] = hald()
    rx, ry = RAMPS
    r = ramps()
    card[ry:ry + r.shape[0], rx:rx + r.shape[1]] = r
    sx, sy = SPATIAL
    s = spatial()
    card[sy:sy + s.shape[0], sx:sx + s.shape[1]] = s
    for x, y in ((8, 8), (W - 16, 8), (8, H - 16), (W - 16, H - 16)):
        card[y:y + 8, x:x + 8] = 255
    Image.fromarray(card).save(path)
    print('wrote', path)


if __name__ == '__main__':
    main(sys.argv[1] if len(sys.argv) > 1 else 'calibration_card.png')
