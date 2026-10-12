"""The launcher: a spring barrel that fires the player into the air, as
`sprites/launcher.json`.

A banded wooden barrel on a stone footing, mouth up, with a spring-loaded
plate inside. Run by `tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/launcher.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin, in world units. A launcher is placed with its
# middle (the 24x16 box the player touches) on the ground, so the barrel
# stands on the origin and the box covers its lower half.
FRAME = (36, 40)
ORIGIN = (18, 32)

# (name, frames, ticks per frame, passes or None to loop). `fire` plays
# through the 20 ticks a launcher rests after firing.
ANIMATIONS = [
    ("waiting", 1, 6, None),
    ("fire", 10, 2, 1),
]

COLORS = {
    "wood": "#b07a46",
    "bands": "#5d6b7a",
    "footing": "#8d8a9b",
    "plate": "#e8553d",
    "inside": "#3a2a22",
    "puff": "#f3efe6",
}

HEIGHT = 1.7


def build():
    m = {name: cr.material(name, color) for name, color in COLORS.items()}
    for metal in ("bands",):
        m[metal].node_tree.nodes["Principled BSDF"].inputs["Metallic"].default_value = 0.8
        m[metal].node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.35
    m["puff"].node_tree.nodes["Principled BSDF"].inputs["Sheen Weight"].default_value = 1.0

    rig = cr.Rig("launcher", [("root", (0, 0, 0), (0, 0, 0.3), None),
                              ("barrel", (0, 0, 0.3), (0, 0, HEIGHT), "root"),
                              ("plate", (0, 0, HEIGHT - 0.4), (0, 0, HEIGHT - 0.2), "barrel"),
                              ("puff", (0, 0, HEIGHT), (0, 0, HEIGHT + 0.3), "barrel")],
                 yaw=-10)
    rig.attach(
        "root",
        cr.lathe("footing", m["footing"], [(1.15, 0), (1.15, 0.2), (1.0, 0.32), (0, 0.32)],
                 segments=10),
    )
    # a barrel bulging at the middle, hollow at the top
    profile = [(0.82, 0.3), (0.95, 0.3 + HEIGHT * 0.25), (0.98, 0.3 + HEIGHT * 0.45),
               (0.9, HEIGHT), (0.78, HEIGHT), (0.72, HEIGHT - 0.25), (0, HEIGHT - 0.25)]
    rig.attach(
        "barrel",
        cr.lathe("barrel", m["wood"], profile, segments=20),
        cr.ring("band.low", m["bands"], 0.91, 0.06, (0, 0, 0.55)),
        cr.ring("band.high", m["bands"], 0.92, 0.06, (0, 0, HEIGHT - 0.2)),
        cr.lathe("inside", m["inside"], [(0.73, HEIGHT - 0.3), (0, HEIGHT - 0.3)], segments=20),
    )
    for k in range(8):
        a = 2 * math.pi * (k + 0.5) / 8
        rig.attach("barrel", cr.box(
            f"stave.{k}", m["inside"], (0.03, 0.03, HEIGHT - 0.6),
            (0.965 * math.cos(a), 0.965 * math.sin(a), 0.3 + HEIGHT * 0.45),
            (0, 0, a), bevel=0,
        ))
    rig.attach("plate", cr.lathe(
        "plate", m["plate"], [(0.62, HEIGHT - 0.4), (0.62, HEIGHT - 0.28), (0, HEIGHT - 0.25)],
        segments=20,
    ))
    for k in range(7):
        a = 2 * math.pi * k / 7
        rig.attach("puff", cr.ball(
            f"puff.{k}", m["puff"], (0.28, 0.28, 0.24),
            (0.6 * math.cos(a), 0.6 * math.sin(a), HEIGHT + 0.15),
        ))
    return rig


def pose(rig, name, i, count):
    puffs = [p for p in rig.parts if p.name.startswith("puff.")]
    if name == "waiting":
        for puff in puffs:
            puff.scale = (0.001, 0.001, 0.001)
    elif name == "fire":
        t = i / count
        # the plate kicks up out of the mouth as the barrel stretches,
        # then settles back while a ring of smoke rises, spreads and thins
        kick = math.sin(math.pi * min(1.0, t * 3))
        rig.move("plate", z=4.5 * kick)
        rig.squash("barrel", -0.18 * kick + 0.08 * max(0.0, math.sin(math.pi * (t * 3 - 1))))
        spread = 0.8 + 0.9 * t
        # the bone's own y runs up the barrel
        rig.obj.pose.bones["puff"].scale = (spread, 1, spread)
        rig.move("puff", z=7 * t)
        size = max(0.001, math.sin(math.pi * min(1.0, 0.15 + t)))
        for puff in puffs:
            x, y, z = puff["rest_scale"]
            puff.scale = (x * size, y * size, z * size)
    else:
        raise ValueError(name)


cr.run("launcher", build, pose, FRAME, ORIGIN, ANIMATIONS)
