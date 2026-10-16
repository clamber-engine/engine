"""A letter of a level's word, a collectible, swaying, as
`sprites/letter.json`.

A glowing tile with a bright stud in its face. Every letter uses it;
how each is told apart isn't decided yet. Run by `tools/render-art.sh`,
or on its own:

    blender -b --factory-startup -P art/letter.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle), in world units.
FRAME = (16, 16)
ORIGIN = (8, 8)

# (name, frames, ticks per frame, passes or None to loop): one sway
# there and back.
ANIMATIONS = [("sway", 16, 3, None)]


def build():
    tile, _ = cr.glow("tile", "#f0a848", 0.5)
    stud, _ = cr.glow("stud", "#fff6d8", 2.5)
    rig = cr.Rig("letter", [("root", (0, 0, -0.6), (0, 0, 0.6), None)])
    rig.attach(
        "root",
        cr.box("tile", tile, (1.1, 0.24, 1.1), bevel=0.25),
        cr.ball("stud", stud, (0.2, 0.1, 0.2), (0, -0.14, 0)),
    )
    return rig


def pose(rig, name, i, count):
    if name != "sway":
        raise ValueError(name)
    rig.turn("root", 30 * math.sin(2 * math.pi * i / count), axis=(0, 0, 1))


cr.run("letter", build, pose, FRAME, ORIGIN, ANIMATIONS)
