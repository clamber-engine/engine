"""The rope: a long twisted hemp rope hanging from out of sight, with a
knot at its bottom end, as `sprites/rope.json`.

Two strands wound round each other, frayed a little below the knot. It
hangs from its position, the bottom end, up past the top of the frame;
the game draws it there. Run by `tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/rope.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the bottom end), in world units. The box the
# game draws it in is 8 wide and 160 tall.
FRAME = (16, 168)
ORIGIN = (8, 164)

# (name, frames, ticks per frame, passes or None to loop).
ANIMATIONS = [
    ("hang", 1, 6, None),
]

# The rope's length and its strands' twist, in Blender units.
LENGTH = 16.0
TWIST = 0.6
STRAND = 0.13


def build():
    hemp = cr.material("hemp", "#b8935a", roughness=0.9)
    dark = cr.material("hemp.dark", "#8a6a3c", roughness=0.9)

    rig = cr.Rig("rope", [("root", (0, 0, 0), (0, 0, 0.5), None)], yaw=0)
    steps = int(LENGTH / TWIST * 2)
    for k in range(steps):
        z0 = 0.35 + k * TWIST / 2
        z1 = z0 + TWIST / 2
        for s, mat in ((0, hemp), (1, dark)):
            a0 = math.pi * (k + s)
            a1 = a0 + math.pi
            rig.attach("root", cr.limb(
                f"strand.{s}.{k}", mat,
                (0.1 * math.cos(a0), 0.1 * math.sin(a0), z0),
                (0.1 * math.cos(a1), 0.1 * math.sin(a1), z1),
                STRAND,
            ))
    rig.attach(
        "root",
        cr.ball("knot", hemp, (0.26, 0.26, 0.22), (0, 0, 0.3)),
        cr.ring("knot.band", dark, 0.2, 0.07, (0, 0, 0.36), (0.2, 0, 0)),
    )
    for k in range(5):
        a = 2 * math.pi * k / 5
        rig.attach("root", cr.limb(
            f"fray.{k}", hemp,
            (0.06 * math.cos(a), 0.06 * math.sin(a), 0.2),
            (0.16 * math.cos(a), 0.16 * math.sin(a), 0.02),
            0.035,
        ))
    return rig


def pose(rig, name, i, count):
    if name != "hang":
        raise ValueError(name)


cr.run("rope", build, pose, FRAME, ORIGIN, ANIMATIONS)
