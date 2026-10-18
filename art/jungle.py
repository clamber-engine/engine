"""The jungle floor's props, in `props/jungle/`, listed in
`props/jungle.json` (see `lib/clamber_props.py`).

Ground edges, placed on the collision line, overhanging it:
- `lip`, `lip-short`: a lip of turf and soil with grass blades above and
  an uneven edge below; `lip-end`, the end of a ledge, the soil curling
  down with roots and turf hanging off it (mirror it for the left end);
- `roots`: roots hanging under a ledge or a bank, from z = 0 down;
- `rocks`, `flowers`, `fern`, `bush`: small things on the ground.

Props behind the player: `tree`, a broad jungle tree with buttress roots;
`palm`; `log`, fallen and mossy; `mushrooms`; `sign`, a board on two
posts; `old-crates`, a mossy heap of broken boxes, darker than the
crates that can be picked up; `pillar` and `arch`, ruins of stone.

Foliage for front layers, drawn partly transparent: `front-leaves`,
broad leaves rising from the bottom of the view; `front-fern`; `vine`
and `branch`, hanging from z = 0.

Run by `tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/jungle.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import bpy  # noqa: E402
from mathutils import Vector  # noqa: E402

import clamber_props as cp  # noqa: E402
import clamber_render as cr  # noqa: E402


def greens():
    return [cr.material(f"blade {c}", c, roughness=0.6, sheen=0.3)
            for c in (cp.GRASS, cp.BLADE, "#4ea552")]


def lip_strip(rng, width, end=False):
    """Turf and soil along `width` Blender units, z = 0 its top; with `end`
    the right end curls down over a ledge."""
    soil = cr.material("soil", cp.SOIL, roughness=0.9)
    turf = cr.material("turf", cp.GRASS, roughness=0.75, sheen=0.4)
    stone = cr.material("pebble", "#a07a5c", roughness=0.8)
    blades = greens()
    half = width / 2
    x = -half
    while x < half:
        r = rng.uniform(0.12, 0.22)
        drop = 0.06 + r * rng.uniform(0.5, 0.9)
        if end and x > half - 0.6:
            drop += (x - (half - 0.6)) * 0.5
        cp.lumpy("soil", soil, (x, rng.uniform(-0.05, 0.1), -drop * 0.6),
                 (r * 1.2, 0.25, drop * rng.uniform(0.8, 1.5)), rng, strength=0.35, scale=0.2)
        if rng.random() < 0.3:
            cp.lumpy("pebble", stone, (x, -0.2, -drop * rng.uniform(0.5, 1.0)),
                     (r * 0.45, 0.1, r * 0.35), rng, strength=0.2, scale=0.3)
        cp.lumpy("turf", turf, (x, rng.uniform(-0.1, 0.05), -0.02),
                 (r * 1.3, 0.3, 0.07 + r * 0.25), rng, strength=0.2, scale=0.25)
        x += r * 1.4
    x = -half
    while x < half:
        cp.tuft("blade", None, x, 0.03, rng, count=rng.randint(5, 9),
                height=rng.uniform(0.18, 0.4), y=-0.25, mats=blades)
        x += rng.uniform(0.12, 0.25)
    if end:
        roots = cr.material("root", cp.ROOT, roughness=0.85)
        for i in range(4):
            x0 = half - rng.uniform(0.0, 0.5)
            length = rng.uniform(0.4, 1.0)
            cp.tube("root", roots, cp.curve((x0, -0.1, -0.2), (x0 + rng.uniform(0.05, 0.3), -0.1,
                                                                -0.2 - length),
                                             bend=rng.uniform(-0.1, 0.1), steps=6,
                                             wobble=0.03, seed=i),
                    [0.035, 0.03, 0.025, 0.02, 0.015, 0.01, 0], sides=6)
        for i in range(5):
            x0 = half - rng.uniform(-0.05, 0.25)
            cp.tuft("blade", None, x0, -0.15 - rng.uniform(0, 0.25), rng, count=5,
                    height=-rng.uniform(0.15, 0.3), y=-0.35, mats=blades)


def lip(rng):
    lip_strip(rng, 4.8)


def lip_short(rng):
    lip_strip(rng, 3.2)
    petal = cr.material("petal", "#f2d14b", roughness=0.5)
    heart = cr.material("heart", "#c0622f")
    for i, x in enumerate((-0.9, 0.3, 1.1)):
        cp.flower("flower", petal, heart, x, rng.uniform(0.15, 0.25), y=-0.3, seed=i)


def lip_end(rng):
    lip_strip(rng, 3.2, end=True)


def roots(rng):
    mat = cr.material("root", cp.ROOT, roughness=0.85)
    light = cr.material("root light", "#8a5e42", roughness=0.85)
    for i in range(9):
        x0 = rng.uniform(-1.4, 1.4)
        length = rng.uniform(0.5, 1.6)
        end = (x0 + rng.uniform(-0.3, 0.3), 0, -length)
        cp.tube("root", (mat, light)[i % 2], cp.curve((x0, 0, 0.05), end, bend=rng.uniform(-0.2, 0.2),
                                                      steps=8, wobble=0.04, seed=i),
                [0.05, 0.045, 0.04, 0.035, 0.03, 0.025, 0.02, 0.012, 0], sides=6)
        for _ in range(2):
            t = rng.uniform(0.3, 0.7)
            sx, sz = x0 + (end[0] - x0) * t, -length * t
            cp.tube("rootlet", mat, cp.curve((sx, 0, sz), (sx + rng.uniform(-0.3, 0.3), 0,
                                                           sz - rng.uniform(0.2, 0.4)), steps=4),
                    [0.015, 0.012, 0.01, 0.006, 0], sides=5)


def rocks(rng):
    stone = cr.material("stone", cp.STONE, roughness=0.8)
    dark = cr.material("dark stone", "#6f6c80", roughness=0.85)
    moss = cr.material("moss", cp.MOSS, roughness=0.9, sheen=0.5)
    for i, (x, r) in enumerate([(-0.55, 0.42), (0.25, 0.6), (0.85, 0.3)]):
        cp.lumpy("rock", (stone, dark)[i % 2], (x, 0, r * 0.55), (r, r * 0.8, r * 0.7), rng,
                 strength=0.35, scale=0.5)
        cp.lumpy("moss", moss, (x - r * 0.15, -0.05, r * 1.0), (r * 0.7, r * 0.6, r * 0.25), rng,
                 strength=0.3, scale=0.15)
    cp.tuft("blade", None, -0.15, 0.02, rng, count=7, height=0.3, y=-0.5, mats=greens())


def flowers(rng):
    stalk = cr.material("stalk", "#3f8f45", roughness=0.6)
    leaves = cr.material("leaf", cp.LEAF, roughness=0.6, sheen=0.3)
    colors = [("#e8505b", "#f7d35c"), ("#f2d14b", "#c0622f"), ("#f5f0f4", "#f2c14b"),
              ("#9c6ade", "#f7e07a")]
    for i in range(7):
        x = (i - 3) * 0.22 + rng.uniform(-0.05, 0.05)
        h = rng.uniform(0.3, 0.6)
        tip = (x + rng.uniform(-0.08, 0.08), 0, h)
        cp.tube("stalk", stalk, cp.curve((x, 0, 0), tip, bend=0.04, steps=4), 0.012, sides=5,
                subdivide=0)
        petal, heart = colors[i % len(colors)]
        cp.flower("flower", cr.material(f"petal {i}", petal, roughness=0.5),
                  cr.material(f"heart {i}", heart), tip[0], tip[2], size=0.08, y=-0.03, seed=i)
        cp.leaf("leaf", leaves, (x, 0.05), rng.choice((-1, 1)) * rng.uniform(40, 70),
                rng.uniform(0.15, 0.25), 0.07, tilt=rng.uniform(-30, 30), y=-0.02, droop=0.2)
    cp.tuft("blade", None, 0, 0.02, rng, count=11, height=0.25, spread=0.6, y=0.05, mats=greens())


def fern_at(rng, x, size, y=0.0, colors=(cp.LEAF, cp.LEAF_LIGHT)):
    rachis = cr.material("rachis", "#3b7a35", roughness=0.6)
    leaflets = [cr.material(f"leaflet {c}", c, roughness=0.55, sheen=0.4) for c in colors]
    for i, angle in enumerate((-70, -45, -22, -5, 12, 35, 58, 78)):
        cp.frond("frond", rachis, leaflets[i % 2], (x, 0), angle + rng.uniform(-6, 6),
                 size * rng.uniform(0.8, 1.1), 60, rng, y=y + rng.uniform(-0.2, 0.2), pairs=10,
                 leaflet_length=0.14 * size, width=0.045 * size, tilt=rng.uniform(-20, 20))


def fern(rng):
    fern_at(rng, 0, 1.3)


def bush(rng):
    colors = [cr.material(f"bush {c}", c, roughness=0.7, sheen=0.3)
              for c in (cp.LEAF_DARK, cp.LEAF, cp.LEAF_LIGHT)]
    for i, (x, z, r) in enumerate([(-0.7, 0.45, 0.55), (0.6, 0.45, 0.6), (0.0, 0.7, 0.75),
                                   (-0.3, 1.05, 0.45), (0.35, 1.0, 0.45)]):
        cp.leafy("bush", colors[min(i // 2, 2)], (x, i * -0.05, z), (r, r * 0.8, r * 0.85), rng)
    leaves = cr.material("leaf", cp.LEAF_LIGHT, roughness=0.55, sheen=0.4)
    for i in range(14):
        a = rng.uniform(-100, 100)
        base = (rng.uniform(-0.6, 0.6), rng.uniform(0.4, 1.1))
        cp.leaf("leaf", leaves, base, a, rng.uniform(0.3, 0.45), 0.16, tilt=rng.uniform(-40, 40),
                y=-0.6, droop=0.3)


def tree(rng):
    bark = cr.material("bark", cp.BARK, roughness=0.9)
    dark = cr.material("dark bark", "#553a28", roughness=0.9)
    # The trunk, swelling to the ground, a little crooked.
    trunk = [(0, 0, -0.1), (0.05, 0, 1.5), (-0.05, 0, 3.5), (0.1, 0, 5.5), (0.0, 0, 7.6)]
    cp.tube("trunk", bark, trunk, [0.75, 0.5, 0.42, 0.38, 0.3], sides=14)
    # Buttress roots: flat fins flaring out at the foot.
    for i, side in enumerate((-1, -1, 1, 1, 0)):
        spread = rng.uniform(1.2, 1.8) * (side or 0.3)
        depth = (-0.35, 0.2, -0.25, 0.3, -0.45)[i]
        fin = cp.tube("buttress", (bark, dark)[i % 2],
                      cp.curve((0, depth, 2.2), (spread, depth, -0.05), sag=-0.5, steps=8),
                      [0.3, 0.32, 0.3, 0.27, 0.23, 0.19, 0.15, 0.1, 0.06], sides=10)
        fin.scale = (1.0, 0.6, 1.0)
    # Boughs and the canopy.
    canopy = [cr.material(f"canopy {c}", c, roughness=0.7, sheen=0.3)
              for c in (cp.LEAF_DARK, cp.LEAF, cp.LEAF_LIGHT)]
    for side in (-1, 1):
        cp.tube("bough", bark, cp.curve((0, 0, 5.4), (side * 2.4, 0.1, 7.2), sag=-0.4, steps=6),
                [0.25, 0.22, 0.18, 0.14, 0.11, 0.08, 0.05], sides=10)
    for i, (x, z, r) in enumerate([(-2.6, 7.2, 1.2), (2.6, 7.3, 1.25), (-1.3, 8.1, 1.4),
                                   (1.3, 8.2, 1.4), (0.0, 8.9, 1.5), (-0.4, 7.4, 1.1),
                                   (0.9, 7.0, 1.0), (-1.8, 8.8, 0.9), (1.9, 8.9, 0.9)]):
        cp.leafy("canopy", canopy[i % 3], (x, rng.uniform(-0.2, 0.6), z), (r, r * 0.8, r * 0.7),
                 rng, scale=0.3, strength=0.45)
    vines = cr.material("vine", "#3d7d3a", roughness=0.7)
    leaves = cr.material("vine leaf", cp.LEAF_LIGHT, roughness=0.55, sheen=0.4)
    for i, x in enumerate((-2.2, -0.9, 1.6, 2.7)):
        length = rng.uniform(2.0, 3.5)
        points = cp.curve((x, -1.0, 6.8), (x + rng.uniform(-0.2, 0.2), -1.0, 6.8 - length),
                          steps=10, wobble=0.05, seed=i)
        cp.tube("vine", vines, points, 0.03, sides=6)
        for p in points[2::2]:
            cp.leaf("vine leaf", leaves, (p.x, p.z), rng.choice((-1, 1)) * rng.uniform(100, 150),
                    0.22, 0.12, tilt=rng.uniform(-30, 30), y=p.y - 0.02)
    fern_at(rng, -1.1, 0.8, y=-0.6)


def palm(rng):
    bark = cr.material("palm bark", "#8a6a48", roughness=0.85)
    rings = cr.material("palm rings", "#7a5c3e", roughness=0.9)
    points = cp.curve((0, 0, 0), (0.9, 0, 7.0), bend=-0.6, steps=14)
    radii = [0.32 - 0.012 * i for i in range(len(points))]
    cp.tube("trunk", bark, points, radii, sides=12)
    for p, q, r in zip(points[1:-1], points[2:], radii[1:-1]):
        for t in (0.0, 0.5):
            at = p + (q - p) * t
            cr.ring("ring", rings, r * 0.92, 0.025, location=at, rotation=(0, -0.08, 0))
    top = points[-1]
    rachis = cr.material("rachis", "#4a7a35", roughness=0.6)
    leaflets = [cr.material(f"palm leaf {c}", c, roughness=0.55, sheen=0.4)
                for c in (cp.LEAF, cp.LEAF_LIGHT, "#4b9a4a")]
    for i, angle in enumerate((-115, -80, -50, -20, 15, 45, 75, 110, 0)):
        cp.frond("frond", rachis, leaflets[i % 3], (top.x, top.z), angle + rng.uniform(-8, 8),
                 rng.uniform(2.6, 3.2), 70, rng, y=rng.uniform(-0.4, 0.4), pairs=14,
                 leaflet_length=0.75, width=0.13, tilt=rng.uniform(-25, 25))
    nut = cr.material("nut", "#7a5a2a", roughness=0.6)
    for dx in (-0.15, 0.1, 0.0):
        cr.ball("nut", nut, (0.17, 0.17, 0.19), (top.x + dx, -0.25, top.z - 0.2 + abs(dx)))


def log(rng):
    bark = cr.material("log bark", "#5e4330", roughness=0.9)
    wood = cr.material("heartwood", "#c08a52", roughness=0.8)
    moss = cr.material("moss", cp.MOSS, roughness=0.9, sheen=0.5)
    cp.tube("log", bark, cp.curve((-2.2, 0, 0.42), (2.2, 0.2, 0.38), bend=0.08, steps=8,
                                  wobble=0.03),
            [0.45, 0.45, 0.44, 0.45, 0.43, 0.44, 0.43, 0.42, 0.42], sides=16)
    cr.lathe("cut", wood, [(0.4, 0), (0.4, 0.02), (0, 0.03)], (-2.24, 0, 0.42),
             (0, -math.pi / 2, 0))
    cr.ring("growth ring", bark, 0.22, 0.012, location=(-2.27, 0, 0.42), rotation=(0, math.pi / 2, 0))
    for x in (-1.5, -0.4, 0.9, 1.7):
        cp.lumpy("moss", moss, (x, -0.05, 0.82), (rng.uniform(0.35, 0.6), 0.35, 0.12), rng,
                 strength=0.3, scale=0.12)
    cr.lathe("branch stub", bark, [(0.14, 0), (0.11, 0.4), (0, 0.45)], (0.6, -0.1, 0.7),
             (0.2, 0.7, 0))
    mushrooms_at(rng, 1.4, 0.75, 0.5)
    cp.tuft("blade", None, -1.0, 0.02, rng, count=9, height=0.35, spread=0.4, y=-0.5,
            mats=greens())
    cp.tuft("blade", None, 2.0, 0.02, rng, count=9, height=0.3, spread=0.4, y=-0.5, mats=greens())


def mushrooms_at(rng, x0, z0, size, colors=("#d9533f", "#f5ecd8")):
    cap = cr.material(f"cap {colors[0]}", colors[0], roughness=0.45)
    stem = cr.material(f"stem {colors[1]}", colors[1], roughness=0.6)
    spot = cr.material("spot", "#fff8ec", roughness=0.5)
    for i in range(4):
        x = x0 + (i - 1.5) * 0.28 * size + rng.uniform(-0.04, 0.04)
        h = size * rng.uniform(0.35, 0.8)
        r = size * rng.uniform(0.18, 0.3)
        cr.lathe("stem", stem, [(0.06 * size, 0), (0.05 * size, h * 0.7), (0.045 * size, h)],
                 (x, -0.3, z0), segments=12)
        cr.lathe("cap", cap, [(r, h * 0.95), (r * 0.95, h + r * 0.25), (r * 0.6, h + r * 0.55),
                              (0, h + r * 0.65)], (x, -0.3, z0), segments=20)
        for k in range(3):
            a = rng.uniform(-1.2, 1.2)
            cr.ball("spot", spot, (r * 0.13, r * 0.06, r * 0.1),
                    (x + math.sin(a) * r * 0.6, -0.3 - r * 0.62, z0 + h + r * (0.25 + 0.2 * k % 2)))


def mushrooms(rng):
    mushrooms_at(rng, 0, 0, 1.0)
    cp.tuft("blade", None, -0.6, 0.02, rng, count=7, height=0.25, y=-0.6, mats=greens())


def sign(rng):
    wood = cr.material("post", "#7a5232", roughness=0.85)
    board = cr.material("board", "#a8784a", roughness=0.8)
    groove = cr.material("groove", "#5b3a22", roughness=0.9)
    rope = cr.material("rope", "#c8b07a", roughness=0.9)
    moss = cr.material("moss", cp.MOSS, roughness=0.9, sheen=0.5)
    for x in (-0.75, 0.75):
        cr.box("post", wood, (0.2, 0.2, 2.2), (x, 0, 1.0), (0, rng.uniform(-0.05, 0.05), 0),
               bevel=0.15)
        cr.lathe("top", wood, [(0.12, 0), (0, 0.15)], (x, 0, 2.1), segments=8, smooth=False)
        cr.ring("lashing", rope, 0.13, 0.025, location=(x, 0, 1.75))
    cr.box("board", board, (2.0, 0.12, 0.8), (0, -0.15, 1.55), (0, 0.04, 0), bevel=0.1)
    for z in (1.35, 1.55, 1.75):
        cr.box("groove", groove, (1.6, 0.02, 0.03), (0, -0.215, z + rng.uniform(-0.02, 0.02)),
               bevel=0.5)
    cp.lumpy("moss", moss, (-0.5, -0.15, 1.95), (0.4, 0.12, 0.06), rng, strength=0.2, scale=0.1)
    cp.tuft("blade", None, -0.75, 0.0, rng, count=8, height=0.35, y=-0.3, mats=greens())
    cp.tuft("blade", None, 0.8, 0.0, rng, count=8, height=0.3, y=-0.3, mats=greens())


def old_crates(rng):
    wood = cr.material("old wood", "#7d6040", roughness=0.9)
    dark = cr.material("old dark wood", "#5a4430", roughness=0.9)
    moss = cr.material("moss", cp.MOSS, roughness=0.9, sheen=0.5)
    for i, (x, z, s, turn) in enumerate([(-0.6, 0.0, 1.1, 0.05), (0.55, 0.0, 1.0, -0.08),
                                         (-0.1, 1.05, 0.9, 0.18)]):
        cr.box("crate", (wood, dark)[i % 2], (s, s, s), (x, 0, z + s / 2), (0, turn, 0.3),
               bevel=0.06)
        for k in range(3):
            dz = (k - 1) * s * 0.33
            cr.box("slat", dark if i % 2 == 0 else wood, (s * 1.02, 0.04, s * 0.08),
                   (x - dz * math.sin(turn), -s * 0.52, z + s / 2 + dz * math.cos(turn)),
                   (0, turn, 0), bevel=0.3)
        cp.lumpy("moss", moss, (x, 0, z + s * 0.98), (s * 0.45, s * 0.45, 0.07), rng,
                 strength=0.3, scale=0.1)
    vines = cr.material("vine", "#3d7d3a", roughness=0.7)
    leaves = cr.material("vine leaf", cp.LEAF_LIGHT, roughness=0.55, sheen=0.4)
    points = cp.curve((-0.4, -0.7, 1.95), (1.0, -0.6, 0.0), sag=-0.4, steps=10, wobble=0.04)
    cp.tube("vine", vines, points, 0.025, sides=6)
    for p in points[1::2]:
        cp.leaf("vine leaf", leaves, (p.x, p.z), rng.uniform(-150, 150), 0.2, 0.11,
                tilt=rng.uniform(-30, 30), y=p.y - 0.02)
    cp.tuft("blade", None, -1.1, 0.0, rng, count=9, height=0.4, y=-0.7, mats=greens())
    cp.tuft("blade", None, 1.1, 0.0, rng, count=9, height=0.35, y=-0.7, mats=greens())


def stone_block(name, mat, size, location, rng, turn=0.0):
    obj = cr.box(name, mat, size, location, (0, turn, 0), bevel=0.12)
    texture = bpy.data.textures.new(f"{name} chips", "CLOUDS")
    texture.noise_scale = 0.25
    chip = obj.modifiers.new("chipped", "DISPLACE")
    chip.texture = texture
    chip.strength = 0.03
    smooth = obj.modifiers["smooth"]
    smooth.levels = smooth.render_levels = 3
    obj.modifiers.move(obj.modifiers.find("chipped"), len(obj.modifiers) - 1)
    return obj


def ruin_moss(rng, spots):
    moss = cr.material("moss", cp.MOSS, roughness=0.9, sheen=0.5)
    for x, z, r in spots:
        cp.lumpy("moss", moss, (x, -0.05, z - 0.04), (r, 0.46, 0.16), rng, strength=0.4,
                 scale=0.12)


def pillar(rng):
    stone = cr.material("ruin stone", "#a39d8e", roughness=0.85)
    dark = cr.material("ruin dark stone", "#857f72", roughness=0.85)
    stone_block("base", dark, (1.4, 1.2, 0.4), (0, 0, 0.2), rng)
    z = 0.4
    for i, h in enumerate((0.9, 0.85, 0.8, 0.6)):
        stone_block("drum", (stone, dark)[i % 2], (0.9, 0.9, h), (rng.uniform(-0.04, 0.04), 0,
                                                                  z + h / 2), rng,
                    turn=rng.uniform(-0.03, 0.03))
        z += h + 0.02
    # The top, broken off at a slant.
    stone_block("broken", stone, (0.9, 0.9, 0.5), (0.05, 0, z + 0.15), rng, turn=0.25)
    stone_block("fallen", dark, (0.8, 0.8, 0.6), (1.2, 0.3, 0.3), rng, turn=0.6)
    ruin_moss(rng, [(-0.1, z + 0.45, 0.35), (0.0, 0.42, 0.65), (1.1, 0.62, 0.3)])
    vines = cr.material("vine", "#3d7d3a", roughness=0.7)
    leaves = cr.material("vine leaf", cp.LEAF_LIGHT, roughness=0.55, sheen=0.4)
    points = cp.curve((0.2, -0.5, z + 0.3), (-0.3, -0.5, 0.3), bend=0.3, steps=12, wobble=0.04)
    cp.tube("vine", vines, points, 0.025, sides=6)
    for p in points[1:]:
        cp.leaf("vine leaf", leaves, (p.x, p.z), rng.uniform(-150, 150), 0.2, 0.11,
                tilt=rng.uniform(-30, 30), y=p.y - 0.02)
    cp.tuft("blade", None, -0.7, 0.0, rng, count=9, height=0.4, y=-0.7, mats=greens())
    fern_at(rng, 0.9, 0.6, y=-0.8)


def arch(rng):
    stone = cr.material("ruin stone", "#a39d8e", roughness=0.85)
    dark = cr.material("ruin dark stone", "#857f72", roughness=0.85)
    for side in (-1, 1):
        z = 0.0
        for i in range(4):
            h = 0.7
            stone_block("jamb", (stone, dark)[(i + side) % 2], (0.8, 0.8, h),
                        (side * 1.6, 0, z + h / 2), rng, turn=rng.uniform(-0.02, 0.02))
            z += h + 0.02
    # The arch's stones, wedges around a half circle.
    count = 7
    for i in range(count):
        a = math.pi * (i + 0.5) / count
        x, z = -math.cos(a) * 1.6, 2.88 + math.sin(a) * 1.6
        if i == count - 1:
            continue  # fallen
        stone_block("voussoir", (stone, dark)[i % 2], (0.7, 0.8, 0.62), (x, 0, z), rng,
                    turn=a - math.pi / 2)
    stone_block("fallen", dark, (0.7, 0.8, 0.62), (2.6, 0.4, 0.3), rng, turn=1.1)
    ruin_moss(rng, [(-1.2, 4.35, 0.5), (-1.6, 2.95, 0.4), (1.6, 2.95, 0.4), (2.6, 0.62, 0.3)])
    vines = cr.material("vine", "#3d7d3a", roughness=0.7)
    leaves = cr.material("vine leaf", cp.LEAF_LIGHT, roughness=0.55, sheen=0.4)
    for i, x in enumerate((-0.9, 0.2, 0.9)):
        top = 2.88 + math.sqrt(max(0.0, 1.6 ** 2 - x * x)) - 0.3
        points = cp.curve((x, -0.5, top), (x + 0.1, -0.5, top - rng.uniform(0.8, 1.6)),
                          steps=8, wobble=0.04, seed=i)
        cp.tube("vine", vines, points, 0.025, sides=6)
        for p in points[1:]:
            cp.leaf("vine leaf", leaves, (p.x, p.z), rng.choice((-1, 1)) * rng.uniform(100, 160),
                    0.2, 0.11, tilt=rng.uniform(-30, 30), y=p.y - 0.02)
    for x in (-2.1, -1.1, 1.1, 2.0):
        cp.tuft("blade", None, x, 0.0, rng, count=9, height=0.45, y=-0.6, mats=greens())


def front_leaves(rng):
    stalk = cr.material("stalk", "#2f6a32", roughness=0.6)
    colors = [cr.material(f"big leaf {c}", c, roughness=0.5, sheen=0.4)
              for c in (cp.LEAF_DARK, cp.LEAF, "#357a3e")]
    for i in range(9):
        x = (i - 4) * 0.45 + rng.uniform(-0.1, 0.1)
        angle = (i - 4) * -14 + rng.uniform(-10, 10)
        length = rng.uniform(1.6, 2.6)
        a = math.radians(angle)
        tip = Vector((x - math.sin(a) * length * 0.2, 0, math.cos(a) * length * 0.2))
        cp.tube("stalk", stalk, cp.curve((x, 0, 0), tip, steps=4), 0.04, sides=6, subdivide=0)
        cp.leaf("big leaf", colors[i % 3], (tip.x, tip.z), angle * 1.3, length,
                length * 0.38, tilt=rng.uniform(-35, 35), y=rng.uniform(-0.3, 0.3), droop=0.3)


def front_fern(rng):
    fern_at(rng, 0, 2.6, colors=(cp.LEAF_DARK, "#357a3e"))


def vine(rng):
    vines = cr.material("vine", "#3d7d3a", roughness=0.7)
    leaves = [cr.material(f"vine leaf {c}", c, roughness=0.55, sheen=0.4)
              for c in (cp.LEAF, cp.LEAF_LIGHT)]
    for k, (x, length) in enumerate([(-0.2, 3.6), (0.25, 2.6)]):
        points = cp.curve((x, 0, 0.05), (x + 0.1, 0, -length), bend=0.15 * (k * 2 - 1), steps=14,
                          wobble=0.04, seed=k)
        cp.tube("vine", vines, points, 0.035, sides=6)
        for i, p in enumerate(points[1:]):
            cp.leaf("vine leaf", leaves[i % 2], (p.x, p.z), (1 if i % 2 else -1) * rng.uniform(100, 140),
                    0.32, 0.18, tilt=rng.uniform(-30, 30), y=p.y - 0.02, droop=0.2)


def branch(rng):
    bark = cr.material("bark", cp.BARK, roughness=0.9)
    leaves = [cr.material(f"leaf {c}", c, roughness=0.55, sheen=0.4)
              for c in (cp.LEAF_DARK, cp.LEAF, cp.LEAF_LIGHT)]
    points = cp.curve((-2.5, 0, 0), (2.0, 0, -1.4), sag=0.4, steps=10, wobble=0.04)
    cp.tube("branch", bark, points, [0.16 - 0.013 * i for i in range(len(points))], sides=10)
    for i, p in enumerate(points[2:], start=2):
        for _ in range(3):
            cp.leaf("leaf", leaves[rng.randrange(3)], (p.x, p.z), rng.uniform(100, 260) - 180,
                    rng.uniform(0.35, 0.55), 0.2, tilt=rng.uniform(-40, 40),
                    y=rng.uniform(-0.3, 0.3), droop=0.3)


cp.render_set("jungle", [
    ("lip", lip), ("lip-short", lip_short), ("lip-end", lip_end), ("roots", roots),
    ("rocks", rocks), ("flowers", flowers), ("fern", fern), ("bush", bush), ("tree", tree),
    ("palm", palm), ("log", log), ("mushrooms", mushrooms), ("sign", sign),
    ("old-crates", old_crates), ("pillar", pillar), ("arch", arch),
    ("front-leaves", front_leaves), ("front-fern", front_fern), ("vine", vine),
    ("branch", branch),
])
