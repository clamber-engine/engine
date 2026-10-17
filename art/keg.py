"""The keg: the restart barrel, as `sprites/keg.json`.

A stout wooden keg standing on end, bound with two iron hoops and marked
with a gold arrow pointing back the way play resumes. The game draws its
pieces and flash itself when it breaks. Run by `tools/render-art.sh`, or
on its own:

    blender -b --factory-startup -P art/keg.py -- <content folder> [--quick]
"""

import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the keg's middle), in world units. The keg
# is 20 across and 18 high.
FRAME = (32, 32)
ORIGIN = (16, 16)

# (name, frames, ticks per frame, passes or None to loop).
ANIMATIONS = [
    ("whole", 1, 6, None),
]

HALF = 0.9
PROFILE = [(0, -HALF), (0.78, -HALF), (0.93, -0.45), (1.0, 0), (0.93, 0.45), (0.78, HALF),
           (0, HALF)]


def build():
    wood = cr.material("wood", "#b5793f", roughness=0.75)
    iron = cr.material("iron", "#5d5a66", roughness=0.4)
    mark, _glow = cr.glow("mark", "#ffd23f", 0.5)

    rig = cr.Rig("keg", [("root", (0, 0, 0), (0, 0, 0.5), None)], yaw=-20)
    rig.attach(
        "root",
        cr.lathe("body", wood, PROFILE, segments=24),
        cr.ring("hoop.0", iron, 0.95, 0.06, (0, 0, -0.5)),
        cr.ring("hoop.1", iron, 0.95, 0.06, (0, 0, 0.5)),
        cr.prism("mark", mark, [(-0.35, -0.2), (0.05, -0.2), (0.05, -0.35), (0.4, 0),
                                (0.05, 0.35), (0.05, 0.2), (-0.35, 0.2)],
                 0.06, (0, -1.0, 0)),
    )
    return rig


def pose(rig, name, i, count):
    if name != "whole":
        raise ValueError(name)


cr.run("keg", build, pose, FRAME, ORIGIN, ANIMATIONS)
