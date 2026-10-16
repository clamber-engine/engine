"""The ball a thrower throws: a hard, knobbly seed, spinning as it
flies, as `sprites/ball.json`.

Run by `tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/ball.py -- <content folder> [--quick]
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
ANIMATIONS = [("spin", 8, 2, None)]


def build():
    shell = cr.material("shell", "#7c5a3a", roughness=0.6)
    knobs = cr.material("knobs", "#d9b77a", roughness=0.5)
    rig = cr.Rig("ball", [("root", (0, 0, 0), (0, 0, 0.5), None)])
    rig.attach("root", cr.ball("shell", shell, (0.55, 0.55, 0.55)))
    for k in range(6):
        a = 2 * math.pi * k / 6
        rig.attach("root", cr.ball(
            f"knob.{k}", knobs, (0.14, 0.14, 0.14),
            (0.52 * math.cos(a), -0.1, 0.52 * math.sin(a)),
        ))
    return rig


def pose(rig, name, i, count):
    if name != "spin":
        raise ValueError(name)
    rig.turn("root", -360 * i / count)


cr.run("ball", build, pose, FRAME, ORIGIN, ANIMATIONS)
