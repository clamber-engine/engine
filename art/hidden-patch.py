"""The patch of bare earth a hidden barrel is drawn as until it is set
off, as `sprites/hidden-patch.json`.

A low mound of dry earth with a few pebbles. Run by
`tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/hidden-patch.py -- <content folder> [--quick]
"""

import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle of its base), in world units. The
# patch is about 56 across.
FRAME = (64, 32)
ORIGIN = (32, 26)

ANIMATIONS = [("patch", 1, 6, None)]

PEBBLES = [(-1.6, -0.2, 0.22), (0.9, 0.3, 0.18), (1.7, -0.1, 0.25), (-0.3, 0.5, 0.15)]


def build():
    earth = cr.material("earth", "#8a6a45", roughness=0.95)
    stone = cr.material("stone", "#a79a86", roughness=0.8)
    rig = cr.Rig("hidden-patch", [("root", (0, 0, 0), (0, 0, 0.5), None)])
    parts = [cr.ball("mound", earth, (2.7, 0.9, 0.32), (0, 0, 0))]
    for k, (x, y, r) in enumerate(PEBBLES):
        parts.append(cr.ball(f"pebble.{k}", stone, (r, r, r * 0.7), (x, y, 0.3)))
    rig.attach("root", *parts)
    return rig


def pose(rig, name, i, count):
    if name != "patch":
        raise ValueError(name)


cr.run("hidden-patch", build, pose, FRAME, ORIGIN, ANIMATIONS)
