"""The canister: a metal drum the player can carry and throw, as
`sprites/canister.json`.

A steel drum standing on end, with two raised ribs, a rimmed lid and a
gold mark, built from parts that can fly apart when it breaks. Run by
`tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/canister.py -- <content folder> [--quick]
"""

import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle of its base), in world units. The
# drum is 20 across and 24 high.
FRAME = (48, 40)
ORIGIN = (24, 36)

# (name, frames, ticks per frame, passes or None to loop).
ANIMATIONS = [
    ("whole", 1, 6, None),
    ("break", 8, 2, 1),
]

RADIUS = 1.0
HEIGHT = 2.4
PROFILE = [(0, 0), (RADIUS * 0.95, 0), (RADIUS, 0.08), (RADIUS, HEIGHT - 0.08),
           (RADIUS * 0.95, HEIGHT), (0, HEIGHT)]

# Each part: its bone, where it sits, and how it flies when the drum
# breaks (sideways and up in world units, and a spin in degrees).
PARTS = [
    ("body", (0, 0, HEIGHT / 2), (-2, 6, -30)),
    ("rib.0", (0, 0, HEIGHT * 0.33), (-8, 5, 80)),
    ("rib.1", (0, 0, HEIGHT * 0.67), (8, 7, -70)),
    ("lid", (0, 0, HEIGHT), (3, 13, 120)),
]


def build():
    steel = cr.material("steel", "#8a96a3", roughness=0.35)
    rib = cr.material("rib", "#5b6672", roughness=0.4)
    mark, _glow = cr.glow("mark", "#ffd23f", 0.6)

    bones = [("root", (0, 0, 0), (0, 0, 0.5), None)]
    bones += [(name, at, (at[0], at[1], at[2] + 0.3), "root") for name, at, _f in PARTS]
    rig = cr.Rig("canister", bones, yaw=-20)
    rig.attach(
        "body",
        cr.lathe("body", steel, PROFILE, segments=24),
        cr.prism("mark", mark, [(-0.3, -0.3), (0.3, -0.3), (0.3, 0.3), (-0.3, 0.3)],
                 0.06, (0, -RADIUS - 0.02, HEIGHT / 2)),
    )
    rig.attach("rib.0", cr.ring("rib.0", rib, RADIUS + 0.04, 0.07, PARTS[1][1]))
    rig.attach("rib.1", cr.ring("rib.1", rib, RADIUS + 0.04, 0.07, PARTS[2][1]))
    rig.attach("lid", cr.ring("lid", rib, RADIUS * 0.9, 0.08, PARTS[3][1]))
    return rig


def pose(rig, name, i, count):
    if name == "whole":
        return
    if name != "break":
        raise ValueError(name)
    t = (i + 1) / count
    for part, _at, (dx, dz, spin) in PARTS:
        rig.move(part, x=dx * t, z=dz * t - 18 * t * t)
        rig.turn(part, spin * t)
    shrink = max(0.001, 1 - t * t)
    for part in rig.parts:
        x, y, z = part["rest_scale"]
        part.scale = (x * shrink, y * shrink, z * shrink)


cr.run("canister", build, pose, FRAME, ORIGIN, ANIMATIONS)
