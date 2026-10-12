"""The flutter: a lantern moth that flies a fixed path, as
`sprites/flutter.json`.

A round, fuzzy moth with a glowing tail, big eyes, feathery feelers and
two pairs of wings. Run by `tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/flutter.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle of the body), in world units. The
# body the game collides is 18 wide and 16 tall.
FRAME = (34, 34)
ORIGIN = (17, 17)

# (name, frames, ticks per frame, passes or None to loop). `defeated`
# shows for the 20 ticks before the flutter is gone.
ANIMATIONS = [
    ("fly", 8, 2, None),
    ("defeated", 10, 2, 1),
]

COLORS = {
    "fuzz": "#f08a3c",
    "collar": "#fbe2b8",
    "wings": "#f6c36b",
    "edges": "#b8572a",
    "eyes": "#1f1a2e",
    "shine": "#ffffff",
    "feelers": "#7a3e22",
}
LANTERN = "#ffc22e"

NEAR, FAR = -1, 1
BONES = [("root", (0, 0, -0.5), (0, 0, 0), None),
         ("body", (0, 0, -0.2), (0, 0, 0.5), "root"),
         ("tail", (-0.25, 0, -0.1), (-0.75, 0, -0.45), "body")]
for side, s in (("near", NEAR), ("far", FAR)):
    BONES += [(f"wing.{side}", (-0.05, 0.3 * s, 0.35), (-0.05, 1.0 * s, 0.9), "body"),
              (f"hindwing.{side}", (-0.2, 0.3 * s, 0.1), (-0.4, 0.9 * s, 0.2), "body")]


def build():
    m = {name: cr.material(name, color) for name, color in COLORS.items()}
    m["fuzz"].node_tree.nodes["Principled BSDF"].inputs["Sheen Weight"].default_value = 0.8
    m["collar"].node_tree.nodes["Principled BSDF"].inputs["Sheen Weight"].default_value = 0.8
    m["eyes"].node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.15
    lantern, glow = cr.glow("lantern", LANTERN, 1.2)

    rig = cr.Rig("flutter", BONES, yaw=-30)
    rig.track(glow, "default_value")
    rig.attach(
        "body",
        cr.ball("body", m["fuzz"], (0.52, 0.48, 0.5), (0.1, 0, 0.12)),
        cr.ring("collar", m["collar"], 0.32, 0.12, (0.32, 0, 0.12), (0, math.pi / 2, 0)),
    )
    rig.attach(
        "tail",
        cr.ball("lantern", lantern, (0.34, 0.3, 0.3), (-0.5, 0, -0.22), (0, 0.5, 0)),
        cr.ring("tail band", m["edges"], 0.3, 0.06, (-0.3, 0, -0.1), (0, math.pi / 2 + 0.5, 0)),
    )
    for side, s in (("near", NEAR), ("far", FAR)):
        rig.attach(
            "body",
            cr.ball(f"eye.{side}", m["eyes"], (0.12, 0.13, 0.17), (0.5, 0.22 * s, 0.22)),
            cr.ball(f"shine.{side}", m["shine"], (0.04, 0.04, 0.05), (0.59, 0.2 * s, 0.29)),
            cr.limb(f"feeler.{side}", m["feelers"], (0.45, 0.15 * s, 0.5), (0.75, 0.3 * s, 1.0), 0.035),
            cr.ball(f"plume.{side}", m["feelers"], (0.08, 0.05, 0.2), (0.78, 0.3 * s, 0.98), (0, 0.5, 0)),
        )
        rig.attach(
            f"wing.{side}",
            cr.ball(f"wing.{side}", m["wings"], (0.42, 0.62, 0.05), (-0.1, 0.85 * s, 0.6), (0.7 * s, 0, 0)),
            cr.ball(f"spot.{side}", m["edges"], (0.13, 0.17, 0.06), (-0.12, 0.95 * s, 0.68), (0.7 * s, 0, 0)),
        )
        rig.attach(
            f"hindwing.{side}",
            cr.ball(f"hindwing.{side}", m["edges"], (0.3, 0.42, 0.04), (-0.35, 0.65 * s, 0.15), (0.3 * s, 0, 0)),
        )
    return rig


def flap(rig, near, far):
    """Raises the wings, in degrees, about the body's length."""
    for side, angle in (("near", near), ("far", far)):
        s = NEAR if side == "near" else FAR
        rig.turn(f"wing.{side}", angle * s, axis=(1, 0, 0))
        rig.turn(f"hindwing.{side}", 0.7 * angle * s, axis=(1, 0, 0))


def pose(rig, name, i, count):
    t = i / count
    p = 2 * math.pi * t
    glow = rig.tracks[0][0]
    if name == "fly":
        beat = math.cos(p)
        flap(rig, 45 * beat - 10, 45 * beat - 10)
        rig.move("body", z=-0.8 * beat)
        rig.turn("tail", 8 * math.sin(p))
        glow.default_value = 1.1 + 0.4 * math.sin(p)
    elif name == "defeated":
        # wings fold and the light goes out as it tumbles
        e = min(1.0, t * 2.5)
        flap(rig, 70 * e, 70 * e)
        rig.turn("body", -150 * t)
        rig.squash("body", 0.15 * e)
        rig.move("root", z=-3 * t * t)
        glow.default_value = 1.2 * (1 - e)
        for part in rig.parts:
            if part.name.startswith(("eye.", "shine.")):
                x, y, z = part["rest_scale"]
                part.scale = (x, y, z * (1 - 0.85 * e))
    else:
        raise ValueError(name)


cr.run("flutter", build, pose, FRAME, ORIGIN, ANIMATIONS, hurtbox=lambda rig: [
    p for p in rig.parts if p.name.startswith(("body", "lantern", "eye."))
])
