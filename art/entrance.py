"""The entrance: where the characters come out when a level begins, as
`sprites/entrance.json`.

A round seed pod of woven reeds, bound with a copper band and marked with
a gold chevron pointing up and right, the way it throws the characters
out. The game draws its pieces and flash itself when it bursts. Run by
`tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/entrance.py -- <content folder> [--quick]
"""

import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the pod's middle), in world units. The pod
# is 24 across and 24 high.
FRAME = (36, 36)
ORIGIN = (18, 18)

# (name, frames, ticks per frame, passes or None to loop).
ANIMATIONS = [
    ("whole", 1, 6, None),
]

HALF = 1.15
PROFILE = [(0, -HALF), (0.6, -1.0), (1.05, -0.5), (1.2, 0), (1.05, 0.5), (0.6, 1.0),
           (0.25, HALF), (0, HALF)]


def build():
    reed = cr.material("reed", "#6f9a4a", roughness=0.8)
    band = cr.material("band", "#b8693a", roughness=0.35)
    mark, _glow = cr.glow("mark", "#ffd23f", 0.5)

    rig = cr.Rig("entrance", [("root", (0, 0, 0), (0, 0, 0.5), None)], yaw=-15)
    rig.attach(
        "root",
        cr.lathe("body", reed, PROFILE, segments=24),
        cr.ring("band", band, 1.2, 0.07, (0, 0, 0)),
        cr.prism("mark", mark, [(-0.45, -0.2), (0.0, 0.25), (0.45, -0.2), (0.45, 0.1),
                                (0.0, 0.55), (-0.45, 0.1)],
                 0.06, (0, -1.12, 0.35), rotation=(0, 0.6, 0)),
    )
    return rig


def pose(rig, name, i, count):
    if name != "whole":
        raise ValueError(name)


cr.run("entrance", build, pose, FRAME, ORIGIN, ANIMATIONS)
