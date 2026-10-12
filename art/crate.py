"""The crate: a wooden box that breaks apart to give up what's inside, as
`sprites/crate.json`.

Planked sides with a cross brace and a gold mark, built from loose boards
so they can fly apart. Run by `tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/crate.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle of its base), in world units. The
# crate is 24 across.
FRAME = (48, 40)
ORIGIN = (24, 36)

# (name, frames, ticks per frame, passes or None to loop).
ANIMATIONS = [
    ("whole", 1, 6, None),
    ("break", 8, 2, 1),
]

SIDE = 2.4

# Each board: its bone, where it sits, its size, and how it flies when the
# crate breaks (sideways and up in world units, and a spin in degrees).
BOARDS = [
    ("front", (0, -SIDE / 2 + 0.06, SIDE / 2), (SIDE, 0.12, SIDE), (-3, 8, -50)),
    ("back", (0, SIDE / 2 - 0.06, SIDE / 2), (SIDE, 0.12, SIDE), (4, 6, 40)),
    ("left", (-SIDE / 2 + 0.06, 0, SIDE / 2), (0.12, SIDE - 0.24, SIDE), (-9, 4, 70)),
    ("right", (SIDE / 2 - 0.06, 0, SIDE / 2), (0.12, SIDE - 0.24, SIDE), (9, 5, -80)),
    ("lid", (0, 0, SIDE - 0.06), (SIDE - 0.24, SIDE - 0.24, 0.12), (2, 13, 120)),
]


def build():
    wood = cr.material("wood", "#c08a52", roughness=0.7)
    trim = cr.material("trim", "#7a5230", roughness=0.7)
    mark, _glow = cr.glow("mark", "#ffd23f", 0.6)

    bones = [("root", (0, 0, 0), (0, 0, 0.5), None)]
    bones += [(name, at, (at[0], at[1], at[2] + 0.3), "root") for name, at, _s, _f in BOARDS]
    bones.append(("mark", (0, 0, SIDE / 2), (0, 0, SIDE / 2 + 0.3), "root"))
    rig = cr.Rig("crate", bones, yaw=-25)
    for name, at, size, _fly in BOARDS:
        rig.attach(name, cr.box(name, wood, size, at, bevel=0.15))
    # trim around the front's edges, and a brace across it
    x, y, z = BOARDS[0][1]
    for k, (dx, dz, w, h) in enumerate([(0, SIDE / 2 - 0.14, SIDE, 0.28),
                                        (0, -SIDE / 2 + 0.14, SIDE, 0.28),
                                        (-SIDE / 2 + 0.14, 0, 0.28, SIDE),
                                        (SIDE / 2 - 0.14, 0, 0.28, SIDE)]):
        rig.attach("front", cr.box(f"trim.{k}", trim, (w, 0.08, h), (x + dx, y - 0.08, z + dz),
                                   bevel=0.3))
    rig.attach("front", cr.box("brace", trim, (SIDE * 1.2, 0.08, 0.26), (x, y - 0.08, z),
                               (0, math.radians(45), 0), bevel=0.3))
    rig.attach("mark", cr.ball("mark", mark, (0.32, 0.32, 0.32), (0, 0, SIDE / 2)))
    return rig


def pose(rig, name, i, count):
    if name == "whole":
        return
    if name != "break":
        raise ValueError(name)
    t = (i + 1) / count
    for board, _at, _size, (dx, dz, spin) in BOARDS:
        rig.move(board, x=dx * t, z=dz * t - 18 * t * t)
        rig.turn(board, spin * t)
    rig.move("mark", z=4 * math.sin(math.pi * t))
    shrink = max(0.001, 1 - t * t)
    for part in rig.parts:
        if part.name != "mark":
            x, y, z = part["rest_scale"]
            part.scale = (x * shrink, y * shrink, z * shrink)


cr.run("crate", build, pose, FRAME, ORIGIN, ANIMATIONS)
