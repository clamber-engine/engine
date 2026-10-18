"""The critter: a tiny songbird that flits about a level as scenery, as
`sprites/critter.json`.

A round little bird with a pale breast, a stubby beak, a cocked tail and
two quick wings. Run by `tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/critter.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle of the body), in world units. It
# touches nothing; the body drawn is about 16 wide and 12 tall.
FRAME = (28, 26)
ORIGIN = (14, 13)

# (name, frames, ticks per frame, passes or None to loop).
ANIMATIONS = [
    ("fly", 6, 2, None),
]

COLORS = {
    "back": "#3f8fd0",
    "breast": "#f7d58a",
    "wings": "#2a5f9e",
    "beak": "#f29b38",
    "eyes": "#1f1a2e",
    "shine": "#ffffff",
}

NEAR, FAR = -1, 1
BONES = [("root", (0, 0, -0.5), (0, 0, 0), None),
         ("body", (0, 0, -0.2), (0, 0, 0.4), "root"),
         ("tail", (-0.3, 0, -0.05), (-0.75, 0, 0.15), "body")]
for side, s in (("near", NEAR), ("far", FAR)):
    BONES += [(f"wing.{side}", (-0.05, 0.25 * s, 0.25), (-0.15, 0.9 * s, 0.6), "body")]


def build():
    m = {name: cr.material(name, color) for name, color in COLORS.items()}
    m["back"].node_tree.nodes["Principled BSDF"].inputs["Sheen Weight"].default_value = 0.6
    m["breast"].node_tree.nodes["Principled BSDF"].inputs["Sheen Weight"].default_value = 0.6
    m["eyes"].node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.15

    rig = cr.Rig("critter", BONES, yaw=-30)
    rig.attach(
        "body",
        cr.ball("body", m["back"], (0.42, 0.36, 0.36), (0, 0, 0.1)),
        cr.ball("breast", m["breast"], (0.3, 0.3, 0.28), (0.14, 0, 0.02)),
        cr.ball("head", m["back"], (0.26, 0.25, 0.25), (0.34, 0, 0.42)),
        cr.ball("beak", m["beak"], (0.13, 0.06, 0.06), (0.62, 0, 0.4), (0, 0.15, 0)),
    )
    rig.attach(
        "tail",
        cr.ball("tail", m["wings"], (0.34, 0.14, 0.05), (-0.55, 0, 0.12), (0, -0.5, 0)),
    )
    for side, s in (("near", NEAR), ("far", FAR)):
        rig.attach(
            "body",
            cr.ball(f"eye.{side}", m["eyes"], (0.07, 0.07, 0.08), (0.48, 0.15 * s, 0.48)),
            cr.ball(f"shine.{side}", m["shine"], (0.025, 0.025, 0.03), (0.53, 0.15 * s, 0.52)),
        )
        rig.attach(
            f"wing.{side}",
            cr.ball(f"wing.{side}", m["wings"], (0.32, 0.5, 0.04), (-0.12, 0.7 * s, 0.45), (0.6 * s, 0, 0)),
        )
    return rig


def pose(rig, name, i, count):
    p = 2 * math.pi * i / count
    if name == "fly":
        beat = math.cos(p)
        for side, s in (("near", NEAR), ("far", FAR)):
            rig.turn(f"wing.{side}", (55 * beat - 5) * s, axis=(1, 0, 0))
        rig.move("body", z=-0.5 * beat)
        rig.turn("tail", 10 * math.sin(p))
    else:
        raise ValueError(name)


cr.run("critter", build, pose, FRAME, ORIGIN, ANIMATIONS)
