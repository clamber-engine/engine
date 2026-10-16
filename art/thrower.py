"""The thrower: a squat clay jar that sits on a ledge and pops balls out
of its mouth, as `sprites/thrower.json`.

A round-bellied jar with a rolled lip, painted bands and two eyes, rocking
a little where it sits. Run by `tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/thrower.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle of the 24x24 box it's placed by),
# in world units.
FRAME = (32, 32)
ORIGIN = (16, 16)

# (name, frames, ticks per frame, passes or None to loop): a slow rock.
ANIMATIONS = [("idle", 8, 6, None)]

COLORS = {
    "clay": "#c4643c",
    "bands": "#f0d9a8",
    "inside": "#2e1a14",
    "eyes": "#fdf8ec",
    "pupils": "#1d1b26",
}


def build():
    m = {name: cr.material(name, color) for name, color in COLORS.items()}
    rig = cr.Rig("thrower", [("root", (0, 0, -1.2), (0, 0, 0), None)], yaw=-15)
    # a jar standing on the box's bottom edge, its mouth near the top
    profile = [(0.5, -1.2), (0.95, -0.9), (1.15, -0.35), (1.05, 0.25), (0.62, 0.7),
               (0.58, 0.9), (0.72, 1.0), (0.6, 1.08), (0.48, 0.95), (0, 0.95)]
    rig.attach(
        "root",
        cr.lathe("jar", m["clay"], profile, segments=24),
        cr.ring("band.low", m["bands"], 1.1, 0.07, (0, 0, -0.55)),
        cr.ring("band.high", m["bands"], 0.95, 0.06, (0, 0, 0.42)),
        cr.lathe("inside", m["inside"], [(0.47, 0.9), (0, 0.9)], segments=24),
    )
    for side in (-1, 1):
        x = 0.32 * side
        rig.attach(
            "root",
            cr.ball(f"eye.{side}", m["eyes"], (0.2, 0.12, 0.24), (x, -1.0, 0.0)),
            cr.ball(f"pupil.{side}", m["pupils"], (0.1, 0.06, 0.12), (x, -1.1, -0.02)),
        )
    return rig


def pose(rig, name, i, count):
    if name != "idle":
        raise ValueError(name)
    rig.turn("root", 6 * math.sin(2 * math.pi * i / count))


cr.run("thrower", build, pose, FRAME, ORIGIN, ANIMATIONS)
