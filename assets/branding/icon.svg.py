#!/usr/bin/env python3
"""Generate the project icon (an isometric Rubik's cube) as SVG.

Run:    python3 icon.svg.py > icon.svg

Each of the three visible faces is a 3x3 grid in WCA colours. The
projection is exact isometric (sin30, cos30), no shortcuts, so the
cube outline is a regular hexagon.
"""

import math

# Canvas + cube sizing.
CANVAS = 1024
# Each step in cube coords goes from -1 to +1 so the cube edge is 2.
# Projected width = 2 * 2 * sin30 * scale = 3.464 * scale. Setting
# scale = 260 puts the cube at ~900 px wide on a 1024 px canvas — a
# comfortable 6% margin on either side.
CUBE_PX = 240
CX, CY = CANVAS // 2, CANVAS // 2

COS30 = math.cos(math.radians(30))
SIN30 = 0.5

# WCA colours.
WHITE  = "#F5F5F5"       # U (top)
GREEN  = "#009E60"       # F (front)
RED    = "#B71234"       # R (right)
LINE   = "#101010"       # cube body / grid lines
BG     = "none"          # transparent canvas

# Project a unit-cube vertex (x, y, z) ∈ {-1, 1}^3 into screen space.
def project(x, y, z):
    sx = CX + CUBE_PX * (x - z) * COS30
    sy = CY + CUBE_PX * (-y + (x + z) * SIN30)
    return sx, sy

# Each face is parameterised by an origin corner and two in-plane axes
# (face-local +i and +j). The grid divides [0, 3] x [0, 3] of those
# axes into 9 stickers.  Stickers are then drawn as quadrilaterals.
FACES = [
    # name, colour, origin (cube vertex), axis_i (vec), axis_j (vec)
    ("U", WHITE,
        (-1,  1, -1),  # back-top-left vertex
        ( 2/3,  0,  0),  # +i = +x  (towards back-top-right)
        ( 0,  0,  2/3),  # +j = +z  (towards front-top-left)
    ),
    ("F", GREEN,
        (-1,  1,  1),  # front-top-left
        ( 2/3,  0,  0),
        ( 0, -2/3,  0),
    ),
    ("R", RED,
        ( 1,  1,  1),  # front-top-right
        ( 0,  0, -2/3),
        ( 0, -2/3,  0),
    ),
]

INSET = 0.06             # gap between adjacent stickers, in face-local units
ROUND = 6                # corner rounding in screen pixels (approx)


def quad_points(origin, ai, aj, i, j):
    """Return the 4 corner pixel positions of sticker (i, j)."""
    ox, oy, oz = origin
    ax, ay, az = ai
    bx, by, bz = aj
    corners = []
    for di, dj in [(0 + INSET, 0 + INSET),
                   (1 - INSET, 0 + INSET),
                   (1 - INSET, 1 - INSET),
                   (0 + INSET, 1 - INSET)]:
        x = ox + ax * (i + di) + bx * (j + dj)
        y = oy + ay * (i + di) + by * (j + dj)
        z = oz + az * (i + di) + bz * (j + dj)
        corners.append(project(x, y, z))
    return corners


def face_outline(origin, ai, aj):
    """Outline of a whole face — used for the dark cube body underneath."""
    ox, oy, oz = origin
    ax, ay, az = ai
    bx, by, bz = aj
    corners = []
    for di, dj in [(0, 0), (3, 0), (3, 3), (0, 3)]:
        x = ox + ax * (i_ := di) + bx * (j_ := dj)
        # one expression for each axis component:
        x = ox + ax * di + bx * dj
        y = oy + ay * di + by * dj
        z = oz + az * di + bz * dj
        corners.append(project(x, y, z))
    return corners


def main():
    out = []
    out.append(
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {CANVAS} {CANVAS}" '
        f'width="{CANVAS}" height="{CANVAS}">'
    )
    out.append(f'  <rect width="{CANVAS}" height="{CANVAS}" fill="{BG}"/>')

    # Draw the dark cube body first — it shows through the inset gaps
    # between stickers and acts as the "plastic" between coloured tiles.
    for _, _, origin, ai, aj in FACES:
        pts = " ".join(f"{x:.2f},{y:.2f}" for x, y in face_outline(origin, ai, aj))
        out.append(f'  <polygon points="{pts}" fill="{LINE}"/>')

    # 9 stickers per face.
    for name, colour, origin, ai, aj in FACES:
        for i in range(3):
            for j in range(3):
                pts = " ".join(
                    f"{x:.2f},{y:.2f}" for x, y in quad_points(origin, ai, aj, i, j)
                )
                out.append(
                    f'  <polygon points="{pts}" fill="{colour}" '
                    f'stroke="{LINE}" stroke-width="2" stroke-linejoin="round"/>'
                )

    out.append('</svg>')
    print('\n'.join(out))


if __name__ == "__main__":
    main()
