"""The crawler: a pebble mite that walks the ground, as `sprites/crawler.json`.

A round bug under a domed stone shell, with six stubby legs, big eyes and
two feelers. Run by `tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/crawler.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (between the feet), in world units. The body
# the game collides is 20 wide and 18 tall.
FRAME = (36, 26)
ORIGIN = (18, 24)

# (name, frames, ticks per frame, passes or None to loop). `defeated`
# shows for the 20 ticks before the crawler is gone.
ANIMATIONS = [
    ("walk", 10, 2, None),
    ("defeated", 10, 2, 1),
]

COLORS = {
    "shell": "#7b6aa8",
    "bands": "#5b4c86",
    "belly": "#d9cbe6",
    "legs": "#3e3354",
    "eyes": "#ffffff",
    "pupils": "#1f1a2e",
    "feelers": "#3e3354",
    "tips": "#f2b134",
}

NEAR, FAR = -1, 1
BONES = [("root", (0, 0, 0), (0, 0, 0.4), None),
         ("body", (0, 0, 0.35), (0, 0, 1.0), "root"),
         ("head", (0.7, 0, 0.6), (1.0, 0, 0.9), "body")]
LEGS = [(-0.55, 0), (0.0, 1), (0.55, 2)]
for side, s in (("near", NEAR), ("far", FAR)):
    for x, n in LEGS:
        BONES.append((f"leg.{side}.{n}", (x, 0.45 * s, 0.5), (x, 0.6 * s, 0.05), "body"))
    BONES.append((f"feeler.{side}", (0.95, 0.2 * s, 1.0), (1.2, 0.3 * s, 1.45), "head"))


def build():
    m = {name: cr.material(name, color) for name, color in COLORS.items()}
    m["shell"].node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.8
    m["eyes"].node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.2

    rig = cr.Rig("crawler", BONES, yaw=-30)
    rig.attach(
        "body",
        cr.ball("shell", m["shell"], (0.95, 0.72, 0.62), (-0.05, 0, 0.72)),
        cr.ring("band", m["bands"], 0.72, 0.07, (-0.05, 0, 0.55), (0, 0, 0)),
        cr.ball("spot.0", m["bands"], (0.2, 0.2, 0.1), (-0.35, -0.3, 1.18), (0.5, -0.3, 0)),
        cr.ball("spot.1", m["bands"], (0.15, 0.15, 0.08), (0.25, -0.38, 1.15), (0.6, 0.3, 0)),
        cr.ball("spot.2", m["bands"], (0.13, 0.13, 0.07), (-0.6, -0.5, 0.85), (1.0, -0.6, 0)),
        cr.ball("spot.3", m["bands"], (0.18, 0.18, 0.09), (-0.2, 0.3, 1.2), (-0.5, -0.2, 0)),
        cr.ball("belly", m["belly"], (0.85, 0.6, 0.3), (0, 0, 0.45)),
    )
    rig.attach(
        "head",
        cr.ball("face", m["belly"], (0.42, 0.48, 0.4), (0.78, 0, 0.62)),
    )
    for side, s in (("near", NEAR), ("far", FAR)):
        rig.attach(
            "head",
            cr.ball(f"eye.{side}", m["eyes"], (0.16, 0.16, 0.22), (1.05, 0.22 * s, 0.78)),
            cr.ball(f"pupil.{side}", m["pupils"], (0.06, 0.09, 0.12), (1.19, 0.21 * s, 0.78)),
        )
        rig.attach(
            f"feeler.{side}",
            cr.limb(f"feeler.{side}", m["feelers"], (0.95, 0.2 * s, 1.0), (1.2, 0.3 * s, 1.45), 0.04),
            cr.ball(f"tip.{side}", m["tips"], (0.08, 0.08, 0.08), (1.22, 0.3 * s, 1.5)),
        )
        for x, n in LEGS:
            rig.attach(
                f"leg.{side}.{n}",
                cr.limb(f"leg.{side}.{n}", m["legs"], (x, 0.45 * s, 0.5), (x, 0.6 * s, 0.1), 0.09),
                cr.ball(f"foot.{side}.{n}", m["legs"], (0.13, 0.1, 0.07), (x + 0.05, 0.6 * s, 0.06)),
            )
    return rig


def pose(rig, name, i, count):
    t = i / count
    p = 2 * math.pi * t
    if name == "walk":
        # two sets of three legs take turns, like a tripod
        for side in ("near", "far"):
            for _x, n in LEGS:
                phase = p + (math.pi if (n + (side == "far")) % 2 else 0)
                rig.turn(f"leg.{side}.{n}", 24 * math.sin(phase))
                rig.move(f"leg.{side}.{n}", z=0.6 * max(0.0, math.cos(phase)))
        rig.move("body", z=0.4 * abs(math.sin(2 * p)))
        rig.turn("body", 2 * math.sin(2 * p))
        rig.turn("head", 4 * math.sin(p))
        rig.turn("feeler.near", 14 * math.sin(p + 1))
        rig.turn("feeler.far", 14 * math.sin(p + 2))
    elif name == "defeated":
        # flattened from above, legs splayed, then sinking
        k = min(1.0, t * 3)
        rig.squash("root", 0.4 * k)
        for side in ("near", "far"):
            for x, n in LEGS:
                rig.turn(f"leg.{side}.{n}", 40 * (n - 1) * k)
        rig.turn("feeler.near", -50 * k)
        rig.turn("feeler.far", -35 * k)
        for part in rig.parts:
            if part.name.startswith("pupil."):
                x, y, z = part["rest_scale"]
                part.scale = (x, y, z * (1 - 0.8 * k))
        rig.move("root", z=-4 * max(0.0, t - 0.5))
    else:
        raise ValueError(name)


cr.run("crawler", build, pose, FRAME, ORIGIN, ANIMATIONS, hurtbox=lambda rig: [
    p for p in rig.parts if not p.name.startswith(("feeler.", "tip."))
])
