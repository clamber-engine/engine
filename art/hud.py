"""The HUD's icons, as `ui/hud.json`: a life, a gem, a relic, and a hit
point full and lost, one still frame each, named for its icon.

They are the pickups' models made simple and turned a little towards
the light: the heart, the gem, the relic's star, and a round flask of the
tonic's green (empty, dark glass for a hit point lost). Rendered at high
resolution and unlit, so they stay sharp at 4K. Run by
`tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/hud.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle), in world units. The game draws
# each icon scaled to the size it wants.
FRAME = (20, 20)
ORIGIN = (10, 10)
# Texels per world unit: 200 texels an icon.
SCALE = 10

ICONS = ["life", "gem", "relic", "hit-point", "hit-point-lost"]
ANIMATIONS = [(icon, 1, 1, None) for icon in ICONS]

# How far the icons not being drawn are moved away, in world units.
AWAY = 1000


def heart():
    """The heart's outline, `(x, z)`: the classic curve."""
    points = []
    for i in range(48):
        t = 2 * math.pi * i / 48
        x = 16 * math.sin(t) ** 3
        z = 13 * math.cos(t) - 5 * math.cos(2 * t) - 2 * math.cos(3 * t) - math.cos(4 * t)
        points.append((x / 17, z / 17 + 0.1))
    return list(reversed(points))


def star(points, outer, inner):
    """A star's outline, a point straight up."""
    outline = []
    for k in range(2 * points):
        a = math.pi / 2 + math.pi * k / points
        r = outer if k % 2 == 0 else inner
        outline.append((r * math.cos(a), r * math.sin(a)))
    return outline


def metal(name, color, roughness):
    mat = cr.material(name, color, roughness=roughness)
    mat.node_tree.nodes["Principled BSDF"].inputs["Metallic"].default_value = 1.0
    return mat


def flask(rig, bone, liquid):
    """A round flask with a cork, filled with `liquid` if it isn't None."""
    glass = cr.material(f"{bone}.glass", "#d8f4ff" if liquid else "#58646c", roughness=0.05)
    shader = glass.node_tree.nodes["Principled BSDF"]
    shader.inputs["Coat Weight"].default_value = 1.0
    shader.inputs["Alpha"].default_value = 0.35 if liquid else 0.8
    cork = cr.material(f"{bone}.cork", "#a8743c" if liquid else "#5c4630", roughness=0.85)
    parts = [
        cr.lathe(f"{bone}.flask", glass,
                 [(0, -0.8), (0.46, -0.72), (0.7, -0.38), (0.7, -0.04), (0.46, 0.26),
                  (0.22, 0.36), (0.22, 0.6), (0.26, 0.66), (0, 0.66)]),
        cr.lathe(f"{bone}.cork", cork, [(0, 0.54), (0.18, 0.54), (0.2, 0.86), (0, 0.88)]),
    ]
    if liquid:
        fill = cr.glow(f"{bone}.liquid", liquid, 1.2)[0]
        fill.node_tree.nodes["Principled BSDF"].inputs["Coat Weight"].default_value = 1.0
        shine = cr.glow(f"{bone}.shine", "#ffffff", 3.0)[0]
        parts += [
            cr.lathe(f"{bone}.liquid", fill,
                     [(0, -0.74), (0.42, -0.66), (0.64, -0.38), (0.64, -0.04), (0.42, 0.2),
                      (0, 0.2)]),
            cr.ball(f"{bone}.shine", shine, (0.09, 0.04, 0.2), (-0.36, -0.62, -0.2)),
        ]
    rig.attach(bone, *parts)


def build():
    rig = cr.Rig("hud", [(icon, (0, 0, -0.9), (0, 0, 0.9), None) for icon in ICONS])

    red = cr.material("red", "#d8283c", roughness=0.25, sheen=0.3)
    core = cr.glow("core", "#ffd0d6", 2.5)[0]
    rig.attach(
        "life",
        cr.prism("heart", red, [(x * 0.9, z * 0.9) for x, z in heart()], 0.6, bevel=0.2),
        cr.ball("core", core, (0.13, 0.05, 0.13), (-0.3, -0.33, 0.3)),
    )

    stone = cr.glow("stone", "#3ee0b0", 0.35)[0]
    shader = stone.node_tree.nodes["Principled BSDF"]
    shader.inputs["Roughness"].default_value = 0.08
    shader.inputs["Coat Weight"].default_value = 1.0
    jewel = cr.lathe(
        "jewel", stone,
        [(0, -0.8), (0.9, 0.06), (0.76, 0.42), (0.46, 0.56), (0, 0.56)],
        segments=8, smooth=False,
    )
    jewel.location = (0, 0, 0.1)
    jewel.scale = (1, 0.45, 1)
    jewel.rotation_euler = (0, 0, math.radians(22.5))
    rig.attach("gem", jewel)

    gold = metal("gold", "#f2b134", 0.25)
    rim = metal("rim", "#b8742a", 0.3)
    amber = cr.glow("amber", "#ff9d3a", 2.0)[0]
    rig.attach(
        "relic",
        cr.prism("points", gold, star(8, 0.92, 0.5), 0.26, bevel=0.05),
        cr.ring("rim", rim, 0.45, 0.08, (0, 0, 0), (math.pi / 2, 0, 0)),
        cr.ball("amber", amber, (0.36, 0.24, 0.36)),
    )

    flask(rig, "hit-point", "#22c84a")
    flask(rig, "hit-point-lost", None)
    return rig


def pose(rig, name, _i, _count):
    if name not in ICONS:
        raise ValueError(name)
    for icon in ICONS:
        if icon != name:
            rig.move(icon, x=AWAY)
    # A little of each icon's side shows, so it reads as solid.
    rig.turn(name, 18, axis=(0, 0, 1))


cr.run("hud", build, pose, FRAME, ORIGIN, ANIMATIONS, folder="ui", scale=SCALE, normals=False)
