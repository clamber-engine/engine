"""The heart, an extra-life item: a glossy red heart with a bright core
that beats, as `sprites/heart.json`.

Run by `tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/heart.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle), in world units.
FRAME = (20, 20)
ORIGIN = (10, 10)

# (name, frames, ticks per frame, passes or None to loop): one beat.
ANIMATIONS = [("beat", 16, 3, None)]


def outline():
    """The heart's outline, `(x, z)` around it: the classic curve."""
    points = []
    for i in range(48):
        t = 2 * math.pi * i / 48
        x = 16 * math.sin(t) ** 3
        z = 13 * math.cos(t) - 5 * math.cos(2 * t) - 2 * math.cos(3 * t) - math.cos(4 * t)
        points.append((x / 17, z / 17 + 0.1))
    return list(reversed(points))


def build():
    red = cr.material("red", "#d8283c", roughness=0.25, sheen=0.3)
    core, _ = cr.glow("core", "#ffd0d6", 2.5)
    rig = cr.Rig("heart", [("root", (0, 0, -0.9), (0, 0, 0.9), None)])
    rig.attach(
        "root",
        cr.prism("heart", red, outline(), 0.55, bevel=0.2),
        cr.ball("core", core, (0.16, 0.05, 0.16), (-0.3, -0.29, 0.3)),
    )
    return rig


def pose(rig, name, i, count):
    if name != "beat":
        raise ValueError(name)
    # A quick double beat, then a rest.
    t = i / count
    beat = max(0.0, math.sin(2 * math.pi * t * 2)) if t < 0.5 else 0.0
    rig.squash("root", -0.12 * beat)
    rig.turn("root", 20 * math.sin(2 * math.pi * t), axis=(0, 0, 1))


cr.run("heart", build, pose, FRAME, ORIGIN, ANIMATIONS)
