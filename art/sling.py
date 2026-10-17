"""The sling: a launch barrel, as `sprites/sling.json`.

A wooden barrel lying on its side, bound with two iron hoops, its open
mouth dark inside and rimmed with a glowing band. Run by
`tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/sling.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the sling's middle), in world units. The
# sling is 32 across and 32 high.
FRAME = (44, 40)
ORIGIN = (22, 20)

# (name, frames, ticks per frame, passes or None to loop).
ANIMATIONS = [
    ("whole", 1, 6, None),
]

HALF = 1.3
PROFILE = [(0, -HALF), (1.05, -HALF), (1.22, -0.65), (1.3, 0), (1.22, 0.65), (1.05, HALF),
           (0, HALF)]
SIDEWAYS = (0, math.pi / 2, 0)


def build():
    wood = cr.material("wood", "#9c5f2e", roughness=0.75)
    iron = cr.material("iron", "#5d5a66", roughness=0.4)
    dark = cr.material("mouth", "#1b1418", roughness=0.9)
    rim, _glow = cr.glow("rim", "#ff8a3d", 0.6)

    rig = cr.Rig("sling", [("root", (0, 0, 0), (0, 0, 0.5), None)], yaw=25)
    rig.attach(
        "root",
        cr.lathe("body", wood, PROFILE, rotation=SIDEWAYS, segments=24),
        cr.ring("hoop.0", iron, 1.2, 0.07, (-0.75, 0, 0), SIDEWAYS),
        cr.ring("hoop.1", iron, 1.2, 0.07, (0.75, 0, 0), SIDEWAYS),
        cr.lathe("mouth", dark, [(0, -0.05), (0.9, -0.05), (0.9, 0.05), (0, 0.05)],
                 location=(-HALF - 0.02, 0, 0), rotation=SIDEWAYS, segments=24),
        cr.ring("rim", rim, 0.98, 0.08, (-HALF - 0.04, 0, 0), SIDEWAYS),
    )
    return rig


def pose(rig, name, i, count):
    if name != "whole":
        raise ValueError(name)


cr.run("sling", build, pose, FRAME, ORIGIN, ANIMATIONS)
