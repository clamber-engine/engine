"""The spring: a fat rubber ring lying on the ground that the player
bounces on, as `sprites/spring.json`.

A dark rubber ring, tread outward, with a bright band around its top
edge. It stands on its position and is 16 high at rest; `squash` is the
ring pressed down to 12. Run by `tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/spring.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (where the spring stands), in world units.
FRAME = (56, 28)
ORIGIN = (28, 24)

# (name, frames, ticks per frame, passes or None to loop).
ANIMATIONS = [
    ("rest", 1, 6, None),
    ("squash", 1, 6, None),
]

RADIUS = 1.55
THICK = 0.8


def build():
    rubber = cr.material("rubber", "#3b3a44", roughness=0.85)
    band = cr.material("band", "#f2b134", roughness=0.4)
    hub = cr.material("hub", "#8d8a9b", roughness=0.5)

    rig = cr.Rig("spring", [("root", (0, 0, 0), (0, 0, 0.5), None)], yaw=0)
    rig.attach(
        "root",
        cr.ring("tyre", rubber, RADIUS, THICK, (0, 0, THICK), (0.3, 0, 0)),
        cr.ring("band", band, RADIUS, 0.14,
                (0, -THICK * math.sin(0.3), THICK * (1 + math.cos(0.3))), (0.3, 0, 0)),
        cr.lathe("hub", hub, [(RADIUS - 0.5, 0.2), (RADIUS - 0.5, 0.5), (0, 0.5)],
                 location=(0, 0, 0.4), rotation=(0.3, 0, 0), segments=24),
    )
    for k in range(12):
        a = 2 * math.pi * (k + 0.5) / 12
        rig.attach("root", cr.box(
            f"tread.{k}", rubber, (0.12, 0.3, 0.5),
            ((RADIUS + THICK - 0.05) * math.cos(a), (RADIUS + THICK - 0.05) * math.sin(a),
             THICK),
            (0.3, 0, a), bevel=0.05,
        ))
    return rig


def pose(rig, name, i, count):
    if name == "squash":
        rig.squash("root", 0.25)
    elif name != "rest":
        raise ValueError(name)


cr.run("spring", build, pose, FRAME, ORIGIN, ANIMATIONS)
