"""The signpost: scenery that nothing touches, as `sprites/signpost.json`.

A wooden board on a single post, with a painted arrow pointing right.
It stands on its position and is 40 high. Run by `tools/render-art.sh`,
or on its own:

    blender -b --factory-startup -P art/signpost.py -- <content folder> [--quick]
"""

import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (where the post stands), in world units.
FRAME = (40, 48)
ORIGIN = (20, 44)

# (name, frames, ticks per frame, passes or None to loop).
ANIMATIONS = [
    ("stand", 1, 6, None),
]


def build():
    wood = cr.material("wood", "#9c6a3a", roughness=0.8)
    board = cr.material("board", "#c99a5b", roughness=0.7)
    paint = cr.material("paint", "#f4efe2", roughness=0.5)

    rig = cr.Rig("signpost", [("root", (0, 0, 0), (0, 0, 0.5), None)], yaw=-15)
    rig.attach(
        "root",
        cr.box("post", wood, (0.35, 0.35, 3.6), (0, 0, 1.8), bevel=0.1),
        cr.box("board", board, (3.0, 0.25, 1.5), (0, -0.25, 3.0), bevel=0.08),
        cr.prism("arrow", paint,
                 [(-1.0, -0.2), (0.4, -0.2), (0.4, -0.5), (1.1, 0), (0.4, 0.5), (0.4, 0.2),
                  (-1.0, 0.2)],
                 0.06, location=(0, -0.4, 3.0)),
    )
    return rig


def pose(rig, name, i, count):
    if name != "stand":
        raise ValueError(name)


cr.run("signpost", build, pose, FRAME, ORIGIN, ANIMATIONS)
