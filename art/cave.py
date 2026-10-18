"""The cave's props, in `props/cave/`, listed in `props/cave.json` (see
`lib/clamber_props.py`).

- `lip`: a ledge's edge of stone, moss on top and drips of rock below;
- `stalactite`, `stalactites`: hanging from z = 0;
- `stalagmite`, `stalagmites`: rising from z = 0;
- `rocks`: a heap of fallen stone;
- `crystals`: a cluster of glowing crystals, to put a light beside;
- `glowcaps`: pale mushrooms that glow;
- `lantern`: a lantern on a post, for a light;
- `front-curtain`: a fringe of stalactites from z = 0 for a front layer.

Run by `tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/cave.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import bpy  # noqa: E402

import clamber_props as cp  # noqa: E402
import clamber_render as cr  # noqa: E402

ROCK = "#5d5a70"
ROCK_LIGHT = "#6c6880"
ROCK_DARK = "#454257"
CAVE_MOSS = "#4f8a5a"


def rock_mats():
    return [cr.material(f"rock {c}", c, roughness=0.85) for c in (ROCK, ROCK_LIGHT, ROCK_DARK)]


def spike(rng, mat, x, length, down, y=0.0, width=None):
    """A stalactite (`down`) or stalagmite from `(x, 0)`, `length` long."""
    width = width or length * rng.uniform(0.18, 0.26)
    sign = -1 if down else 1
    profile = []
    phase = rng.uniform(0, 2 * math.pi)
    for i in range(13):
        t = i / 12
        r = width * (1 - t) ** 1.3 * (1 + 0.18 * math.sin(t * 19 + phase))
        profile.append((max(r, 0.0) if i < 12 else 0, t * length))
    obj = cr.lathe("spike", mat, profile, (x, y, 0.08 * -sign), (math.pi if down else 0, 0, 0),
                   segments=14)
    texture = bpy.data.textures.new("spike noise", "CLOUDS")
    texture.noise_scale = 0.12
    smooth = obj.modifiers.new("smooth", "SUBSURF")
    smooth.levels = smooth.render_levels = 2
    rough = obj.modifiers.new("rough", "DISPLACE")
    rough.texture = texture
    rough.strength = 0.05 + width * 0.15
    return obj


def lip(rng):
    mats = rock_mats()
    moss = cr.material("moss", CAVE_MOSS, roughness=0.9, sheen=0.5)
    x = -2.4
    while x < 2.4:
        r = rng.uniform(0.18, 0.3)
        cp.lumpy("ledge", mats[rng.randrange(3)], (x, 0, -r * 0.5), (r * 1.3, 0.3, r), rng,
                 strength=0.25, scale=0.25)
        if rng.random() < 0.5:
            cp.lumpy("moss", moss, (x, -0.05, 0.0), (r * 1.2, 0.3, 0.06), rng, strength=0.25,
                     scale=0.1)
        if rng.random() < 0.4:
            spike(rng, mats[rng.randrange(3)], x, rng.uniform(0.25, 0.55), True, y=-0.05)
        x += r * 1.5


def stalactite(rng):
    mats = rock_mats()
    spike(rng, mats[1], 0, 2.2, True)
    cp.lumpy("ceiling", mats[2], (0, 0.1, 0.05), (0.7, 0.35, 0.2), rng, strength=0.2, scale=0.25)


def stalactites(rng):
    mats = rock_mats()
    for i, (x, length) in enumerate([(-1.0, 1.2), (-0.4, 2.0), (0.25, 0.8), (0.8, 1.6),
                                     (1.3, 0.6)]):
        spike(rng, mats[i % 3], x, length, True, y=rng.uniform(-0.2, 0.2))
    for x in (-1.2, -0.2, 0.9):
        cp.lumpy("ceiling", mats[2], (x, 0.1, 0.05), (0.6, 0.3, 0.2), rng, strength=0.2,
                 scale=0.25)


def stalagmite(rng):
    mats = rock_mats()
    spike(rng, mats[1], 0, 1.8, False)
    cp.lumpy("floor", mats[2], (0, 0.1, -0.02), (0.6, 0.3, 0.15), rng, strength=0.2, scale=0.25)


def stalagmites(rng):
    mats = rock_mats()
    for i, (x, length) in enumerate([(-0.9, 0.9), (-0.3, 1.6), (0.35, 0.6), (0.8, 1.1)]):
        spike(rng, mats[i % 3], x, length, False, y=rng.uniform(-0.2, 0.2))
    for x in (-0.6, 0.6):
        cp.lumpy("floor", mats[2], (x, 0.1, -0.02), (0.6, 0.3, 0.15), rng, strength=0.2,
                 scale=0.25)


def rocks(rng):
    mats = rock_mats()
    for i, (x, r) in enumerate([(-0.7, 0.45), (0.1, 0.65), (0.8, 0.4), (-0.25, 0.3),
                                (0.55, 0.25)]):
        cp.lumpy("rock", mats[i % 3], (x, -0.1 * (i % 2), r * 0.55), (r, r * 0.8, r * 0.7), rng,
                 strength=0.3, scale=0.4)


def crystals(rng):
    rock = rock_mats()
    shades = [cr.glow(f"crystal {c}", c, s)[0]
              for c, s in (("#3fb8e0", 0.5), ("#6fd0f5", 0.8), ("#7a5ce0", 0.4))]
    for k in shades:
        k.node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.15
    cp.lumpy("bed", rock[2], (0, 0.1, 0.0), (0.9, 0.5, 0.25), rng, strength=0.2, scale=0.3)
    for i, (x, length, lean) in enumerate([(-0.5, 0.7, 30), (-0.2, 1.2, 12), (0.1, 1.5, -5),
                                           (0.35, 0.9, -25), (0.6, 0.5, -45), (-0.75, 0.4, 50)]):
        w = 0.1 + length * 0.07
        cr.lathe("crystal", shades[i % 3], [(w, 0), (w, length * 0.8), (0, length)],
                 (x, -0.1 + 0.05 * (i % 2), 0.05), (0, math.radians(lean), rng.uniform(0, 1)),
                 segments=6, smooth=False)


def glowcaps(rng):
    cap = cr.glow("glowcap", "#4fd89a", 0.5)[0]
    stem = cr.material("glowcap stem", "#d8e8e0", roughness=0.6)
    for i in range(5):
        x = (i - 2) * 0.25 + rng.uniform(-0.05, 0.05)
        h = rng.uniform(0.25, 0.6)
        r = rng.uniform(0.12, 0.2)
        cr.lathe("stem", stem, [(0.035, 0), (0.03, h * 0.7), (0.028, h)], (x, -0.2, 0),
                 segments=10)
        cr.lathe("cap", cap, [(r, h * 0.95), (r * 0.8, h + r * 0.35), (0, h + r * 0.5)],
                 (x, -0.2, 0), segments=18)
    cp.lumpy("floor", rock_mats()[2], (0, 0.15, -0.02), (0.75, 0.3, 0.1), rng, strength=0.2,
             scale=0.25)


def lantern(rng):
    wood = cr.material("post", "#6e4e34", roughness=0.85)
    iron = cr.material("iron", "#3d3a44", roughness=0.4)
    light, _ = cr.glow("lantern light", "#ffc04a", 1.5)
    cr.box("post", wood, (0.18, 0.18, 2.4), (0, 0, 1.15), bevel=0.15)
    cr.box("arm", wood, (0.7, 0.14, 0.14), (0.3, 0, 2.25), bevel=0.15)
    cr.lathe("hook", iron, [(0.02, 0), (0.02, 0.2)], (0.55, -0.02, 1.98), segments=8)
    cr.lathe("lantern", light, [(0.12, 0), (0.15, 0.15), (0.12, 0.32)], (0.55, -0.05, 1.55),
             segments=16)
    cr.lathe("cap", iron, [(0.17, 0), (0.1, 0.08), (0.03, 0.14)], (0.55, -0.05, 1.86),
             segments=16)
    cr.lathe("base", iron, [(0.14, 0), (0.15, 0.04), (0, 0.05)], (0.55, -0.05, 1.52),
             segments=16)
    for a in (0, 1, 2, 3):
        cr.box("bar", iron, (0.02, 0.02, 0.32), (0.55 + 0.14 * math.cos(a * math.pi / 2), -0.05
                                                  + 0.14 * math.sin(a * math.pi / 2), 1.7),
               bevel=0.5)
    cp.lumpy("stone", rock_mats()[0], (0.0, 0.1, 0.0), (0.4, 0.3, 0.12), rng, strength=0.2,
             scale=0.25)


def front_curtain(rng):
    mats = [cr.material(f"dark rock {c}", c, roughness=0.85) for c in ("#2e2c3c", "#3a3749")]
    x = -3.6
    i = 0
    while x < 3.6:
        spike(rng, mats[i % 2], x, rng.uniform(0.6, 2.2), True, y=rng.uniform(-0.3, 0.3))
        cp.lumpy("ceiling", mats[(i + 1) % 2], (x, 0.0, 0.1), (0.55, 0.4, 0.3), rng, strength=0.2,
                 scale=0.25)
        x += rng.uniform(0.35, 0.7)
        i += 1


cp.render_set("cave", [
    ("lip", lip), ("stalactite", stalactite), ("stalactites", stalactites),
    ("stalagmite", stalagmite), ("stalagmites", stalagmites), ("rocks", rocks),
    ("crystals", crystals), ("glowcaps", glowcaps), ("lantern", lantern),
    ("front-curtain", front_curtain),
])
