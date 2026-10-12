"""The relic, the rare pickup: a sun-stone that rocks and glints, as
`sprites/relic.json`.

An eight-pointed golden star around a glowing amber heart. Run by
`tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/relic.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle), in world units.
FRAME = (22, 22)
ORIGIN = (11, 11)

# (name, frames, ticks per frame, passes or None to loop).
ANIMATIONS = [("glint", 20, 2, None)]


def star(points, outer, inner):
    """A star's outline, a point straight up."""
    outline = []
    for k in range(2 * points):
        a = math.pi / 2 + math.pi * k / points
        r = outer if k % 2 == 0 else inner
        outline.append((r * math.cos(a), r * math.sin(a)))
    return outline


def build():
    gold = cr.material("gold", "#f2b134", roughness=0.25)
    gold.node_tree.nodes["Principled BSDF"].inputs["Metallic"].default_value = 1.0
    rim = cr.material("rim", "#b8742a", roughness=0.3)
    rim.node_tree.nodes["Principled BSDF"].inputs["Metallic"].default_value = 1.0
    heart, heart_glow = cr.glow("heart", "#ff9d3a", 2.0)
    sparkle, sparkle_glow = cr.glow("sparkle", "#fff8e0", 8.0)

    rig = cr.Rig("relic", [("root", (0, 0, -0.8), (0, 0, 0.8), None),
                           ("glint", (0.35, -0.4, 0.4), (0.35, -0.4, 0.6), None)])
    rig.track(heart_glow, "default_value")
    rig.attach(
        "root",
        cr.prism("points", gold, star(8, 0.78, 0.42), 0.22, bevel=0.05),
        cr.ring("rim", rim, 0.38, 0.07, (0, 0, 0), (math.pi / 2, 0, 0)),
        cr.ball("heart", heart, (0.3, 0.2, 0.3)),
    )
    rig.attach(
        "glint",
        cr.ball("glint.long", sparkle, (0.03, 0.03, 0.32), (0.35, -0.4, 0.4)),
        cr.ball("glint.wide", sparkle, (0.24, 0.03, 0.03), (0.35, -0.4, 0.4)),
    )
    return rig


def pose(rig, name, i, count):
    if name != "glint":
        raise ValueError(name)
    t = i / count
    p = 2 * math.pi * t
    rig.turn("root", 28 * math.sin(p), axis=(0, 0, 1))
    rig.move("root", z=0.6 * math.sin(p))
    rig.tracks[0][0].default_value = 2.0 + 1.2 * math.sin(p)
    # a sparkle flares and fades over the top of the swing
    flare = max(0.0, math.sin(math.pi * (t - 0.55) / 0.3)) if 0.55 <= t <= 0.85 else 0.0
    for part in rig.parts:
        if part.name.startswith("glint."):
            x, y, z = part["rest_scale"]
            flare = max(flare, 0.001)
            part.scale = (x * flare, y * flare, z * flare)
    rig.turn("glint", 90 * t)


cr.run("relic", build, pose, FRAME, ORIGIN, ANIMATIONS)
