"""The gem, the common pickup, turning on the spot, as `sprites/gem.json`.

A flat-cut jewel, pointed below and tabled above. Run by
`tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/gem.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle), in world units.
FRAME = (20, 20)
ORIGIN = (10, 10)

# (name, frames, ticks per frame, passes or None to loop): a full turn.
ANIMATIONS = [("spin", 16, 2, None)]

COLOR = "#3ee0b0"


def build():
    stone, glow = cr.glow("stone", COLOR, 0.35)
    shader = stone.node_tree.nodes["Principled BSDF"]
    shader.inputs["Roughness"].default_value = 0.08
    shader.inputs["Coat Weight"].default_value = 1.0
    rig = cr.Rig("gem", [("root", (0, 0, -0.8), (0, 0, 0.8), None)])
    jewel = cr.lathe(
        "jewel", stone,
        [(0, -0.72), (0.5, 0.12), (0.42, 0.42), (0.26, 0.52), (0, 0.52)],
        segments=8, smooth=False,
    )
    jewel.scale = (1, 0.45, 1)
    rig.attach("root", jewel)
    return rig


def pose(rig, name, i, count):
    if name != "spin":
        raise ValueError(name)
    rig.turn("root", 360 * i / count, axis=(0, 0, 1))


cr.run("gem", build, pose, FRAME, ORIGIN, ANIMATIONS)
