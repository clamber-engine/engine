"""The cluster, a collectible worth ten, spinning, as
`sprites/cluster.json`.

A ring of small glowing beads around a bright middle. Run by
`tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/cluster.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle), in world units.
FRAME = (28, 24)
ORIGIN = (14, 14)

# (name, frames, ticks per frame, passes or None to loop): a full turn.
ANIMATIONS = [("spin", 16, 2, None)]

BEADS = 6


def build():
    bead, _ = cr.glow("bead", "#ffd35a", 0.6)
    core, _ = cr.glow("core", "#fff6d8", 2.5)
    rig = cr.Rig("cluster", [("root", (0, 0, -0.9), (0, 0, 0.9), None)])
    parts = [cr.ball("core", core, (0.26, 0.26, 0.26))]
    for k in range(BEADS):
        a = 2 * math.pi * k / BEADS
        parts.append(cr.ball(f"bead.{k}", bead, (0.2, 0.2, 0.2),
                             (0.72 * math.cos(a), 0, 0.72 * math.sin(a))))
    rig.attach("root", *parts)
    return rig


def pose(rig, name, i, count):
    if name != "spin":
        raise ValueError(name)
    rig.turn("root", 360 * i / count, axis=(0, 0, 1))


cr.run("cluster", build, pose, FRAME, ORIGIN, ANIMATIONS)
