"""The spark, the small collectible, turning on the spot, as
`sprites/spark.json`.

A four-pointed star with a bright middle. Run by `tools/render-art.sh`,
or on its own:

    blender -b --factory-startup -P art/spark.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle), in world units.
FRAME = (16, 16)
ORIGIN = (8, 8)

# (name, frames, ticks per frame, passes or None to loop): a full turn.
ANIMATIONS = [("spin", 12, 2, None)]


def star(points, outer, inner):
    """A star's outline, a point straight up."""
    outline = []
    for k in range(2 * points):
        a = math.pi / 2 + math.pi * k / points
        r = outer if k % 2 == 0 else inner
        outline.append((r * math.cos(a), r * math.sin(a)))
    return outline


def build():
    body, _ = cr.glow("body", "#ffd35a", 0.6)
    core, _ = cr.glow("core", "#fff6d8", 3.0)
    rig = cr.Rig("spark", [("root", (0, 0, -0.6), (0, 0, 0.6), None)])
    rig.attach(
        "root",
        cr.prism("points", body, star(4, 0.6, 0.2), 0.16, bevel=0.04),
        cr.ball("core", core, (0.14, 0.12, 0.14)),
    )
    return rig


def pose(rig, name, i, count):
    if name != "spin":
        raise ValueError(name)
    rig.turn("root", 360 * i / count, axis=(0, 0, 1))


cr.run("spark", build, pose, FRAME, ORIGIN, ANIMATIONS)
