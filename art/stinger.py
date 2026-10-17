"""The stinger: a striped hover-bug that sways around where it was
placed, as `sprites/stinger.json`.

A round, banded body with a pointed sting, a small head with big eyes,
short feelers and two pairs of fast, clear wings. Run by
`tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/stinger.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle of the body), in world units. The
# box the game draws it in is 16 wide and 16 tall.
FRAME = (32, 32)
ORIGIN = (16, 16)

# (name, frames, ticks per frame, passes or None to loop). It never stops
# buzzing.
ANIMATIONS = [
    ("fly", 4, 1, None),
]

COLORS = {
    "band": "#f2c230",
    "dark": "#2a2230",
    "head": "#3b2f45",
    "eyes": "#d8452e",
    "shine": "#ffffff",
    "sting": "#e8e2d0",
    "wings": "#cfe8f4",
}

NEAR, FAR = -1, 1
BONES = [("root", (0, 0, -0.5), (0, 0, 0), None),
         ("body", (0, 0, -0.2), (0, 0, 0.5), "root"),
         ("abdomen", (-0.15, 0, 0), (-0.7, 0, -0.35), "body")]
for side, s in (("near", NEAR), ("far", FAR)):
    BONES += [(f"wing.{side}", (0.05, 0.2 * s, 0.35), (-0.1, 0.9 * s, 0.85), "body"),
              (f"hindwing.{side}", (-0.1, 0.2 * s, 0.3), (-0.4, 0.8 * s, 0.55), "body")]


def build():
    m = {name: cr.material(name, color) for name, color in COLORS.items()}
    m["eyes"].node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.15
    m["wings"].node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.2

    rig = cr.Rig("stinger", BONES, yaw=-30)
    rig.attach(
        "body",
        cr.ball("thorax", m["dark"], (0.3, 0.28, 0.28), (0.15, 0, 0.15)),
        cr.ball("head", m["head"], (0.24, 0.24, 0.22), (0.5, 0, 0.2)),
    )
    rig.attach(
        "abdomen",
        cr.ball("abdomen", m["band"], (0.45, 0.36, 0.36), (-0.45, 0, -0.15), (0, 0.4, 0)),
        cr.ring("stripe.1", m["dark"], 0.33, 0.07, (-0.3, 0, -0.08), (0, math.pi / 2 + 0.4, 0)),
        cr.ring("stripe.2", m["dark"], 0.3, 0.07, (-0.55, 0, -0.2), (0, math.pi / 2 + 0.4, 0)),
        cr.limb("sting", m["sting"], (-0.8, 0, -0.32), (-1.05, 0, -0.45), 0.05),
    )
    for side, s in (("near", NEAR), ("far", FAR)):
        rig.attach(
            "body",
            cr.ball(f"eye.{side}", m["eyes"], (0.1, 0.11, 0.14), (0.62, 0.14 * s, 0.26)),
            cr.ball(f"shine.{side}", m["shine"], (0.03, 0.03, 0.04), (0.7, 0.13 * s, 0.31)),
            cr.limb(f"feeler.{side}", m["dark"], (0.6, 0.08 * s, 0.38), (0.85, 0.2 * s, 0.7), 0.03),
        )
        rig.attach(
            f"wing.{side}",
            cr.ball(f"wing.{side}", m["wings"], (0.4, 0.55, 0.04), (-0.1, 0.7 * s, 0.6), (0.6 * s, 0, 0)),
        )
        rig.attach(
            f"hindwing.{side}",
            cr.ball(f"hindwing.{side}", m["wings"], (0.28, 0.38, 0.03), (-0.3, 0.55 * s, 0.45), (0.4 * s, 0, 0)),
        )
    return rig


def buzz(rig, angle):
    """Raises the wings, in degrees, about the body's length."""
    for side, s in (("near", NEAR), ("far", FAR)):
        rig.turn(f"wing.{side}", angle * s, axis=(1, 0, 0))
        rig.turn(f"hindwing.{side}", 0.8 * angle * s, axis=(1, 0, 0))


def pose(rig, name, i, count):
    p = 2 * math.pi * i / count
    if name == "fly":
        buzz(rig, 40 * math.cos(p) - 5)
        rig.move("body", z=-0.25 * math.sin(p))
        rig.turn("abdomen", 6 * math.sin(p))
    else:
        raise ValueError(name)


cr.run("stinger", build, pose, FRAME, ORIGIN, ANIMATIONS, hurtbox=lambda rig: [
    p for p in rig.parts if p.name.startswith(("thorax", "head", "abdomen", "eye."))
])
