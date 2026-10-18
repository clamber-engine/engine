"""The treetops' props, in `props/treetops/`, listed in
`props/treetops.json` (see `lib/clamber_props.py`).

- `bough`: a thick branch along z = 0, leaves along its top, to dress a
  platform the player stands on;
- `bough-end`: where a bough meets its trunk, for a platform's end;
- `trunk`: a tall trunk rising from z = 0, with shelf fungus and moss;
- `leaf-clump`, `leaf-clump-small`: dense leaves, around z = 0;
- `nest`: a nest of twigs with eggs, sitting on z = 0;
- `hanging-moss`: moss hanging from z = 0;
- `front-canopy`: leaves hanging down from z = 0 at the top of the view,
  for a front layer.

Run by `tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/treetops.py -- <content folder> [--quick]
"""

import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_props as cp  # noqa: E402
import clamber_render as cr  # noqa: E402


def leaves(rng, x0, x1, z, count, y=-0.3, size=(0.3, 0.5), up=True):
    mats = [cr.material(f"leaf {c}", c, roughness=0.55, sheen=0.4)
            for c in (cp.LEAF_DARK, cp.LEAF, cp.LEAF_LIGHT)]
    for i in range(count):
        x = x0 + (x1 - x0) * rng.random()
        angle = rng.uniform(-80, 80) if up else 180 + rng.uniform(-50, 50)
        cp.leaf("leaf", mats[rng.randrange(3)], (x, z + rng.uniform(-0.05, 0.05)), angle,
                rng.uniform(*size), 0.2, tilt=rng.uniform(-40, 40), y=y + rng.uniform(-0.3, 0.3),
                droop=0.3)


def bough(rng):
    bark = cr.material("bark", cp.BARK, roughness=0.9)
    moss = cr.material("moss", cp.MOSS, roughness=0.9, sheen=0.5)
    points = cp.curve((-3.2, 0, -0.25), (3.2, 0, -0.25), sag=0.06, steps=12, wobble=0.03)
    cp.tube("bough", bark, points, 0.26, sides=12)
    for x in (-2.4, -0.6, 1.3, 2.6):
        cp.lumpy("moss", moss, (x, -0.05, 0.0), (rng.uniform(0.35, 0.6), 0.3, 0.09), rng,
                 strength=0.3, scale=0.1)
    for x in (-1.6, 0.8):
        cp.tube("twig", bark, cp.curve((x, -0.1, -0.1), (x + 0.6, -0.2, 0.6), bend=0.1, steps=4),
                [0.06, 0.05, 0.04, 0.025, 0], sides=6)
        leaves(rng, x + 0.4, x + 0.7, 0.5, 5)
    leaves(rng, -3.0, 3.0, 0.0, 12, y=-0.4, size=(0.2, 0.35))
    leaves(rng, -3.0, 3.0, -0.4, 10, y=-0.4, size=(0.25, 0.4), up=False)


def bough_end(rng):
    bark = cr.material("bark", cp.BARK, roughness=0.9)
    dark = cr.material("dark bark", "#553a28", roughness=0.9)
    cp.tube("trunk", dark, cp.curve((1.2, 0.6, -3.0), (1.1, 0.6, 3.0), steps=6, wobble=0.05),
            0.75, sides=14)
    cp.tube("bough", bark, cp.curve((-2.0, 0, -0.25), (1.0, 0.2, -0.3), sag=0.08, steps=8),
            [0.25, 0.26, 0.27, 0.28, 0.3, 0.33, 0.38, 0.45, 0.55], sides=12)
    leaves(rng, -1.8, 0.4, 0.0, 9, size=(0.2, 0.35))
    fungus(rng, 1.0, 1.2)
    fungus(rng, 0.7, -1.6)


def fungus(rng, x, z, mat=None):
    mat = mat or cr.material("fungus", "#e0b46a", roughness=0.6)
    rim = cr.material("fungus rim", "#f3e2b8", roughness=0.6)
    for k in range(3):
        r = rng.uniform(0.22, 0.35) * (1 - 0.25 * k)
        cr.lathe("shelf", mat, [(0, 0), (r, 0.03), (r * 0.9, 0.08), (0, 0.1)],
                 (x - 0.05 * k, -0.6, z + 0.22 * k), (0.25, 0, 0), segments=20)
        cr.ring("shelf rim", rim, r * 0.92, 0.02, location=(x - 0.05 * k, -0.6, z + 0.22 * k + 0.04),
                rotation=(0.25, 0, 0))


def trunk(rng):
    bark = cr.material("bark", cp.BARK, roughness=0.9)
    dark = cr.material("dark bark", "#553a28", roughness=0.9)
    moss = cr.material("moss", cp.MOSS, roughness=0.9, sheen=0.5)
    cp.tube("trunk", bark, cp.curve((0, 0, -0.1), (0.15, 0, 14.0), bend=0.2, steps=12,
                                    wobble=0.06),
            [0.9, 0.82, 0.78, 0.75, 0.72, 0.7, 0.68, 0.66, 0.64, 0.62, 0.6, 0.58, 0.56], sides=16)
    for i, z in enumerate((1.5, 4.3, 7.8, 11.0)):
        cr.ball("knot", dark, (0.18, 0.1, 0.25), (0.25 * (i % 2 * 2 - 1), -0.7, z))
    for x, z in ((-0.3, 3.0), (0.2, 6.5), (-0.1, 10.2), (0.3, 12.5)):
        cp.lumpy("moss", moss, (x, -0.55, z), (0.45, 0.25, 0.6), rng, strength=0.3, scale=0.12)
    fungus(rng, 0.55, 5.2)
    fungus(rng, -0.6, 9.0)


def leaf_clump_of(rng, size):
    mats = [cr.material(f"clump {c}", c, roughness=0.7, sheen=0.3)
            for c in (cp.LEAF_DARK, cp.LEAF, cp.LEAF_LIGHT)]
    for i in range(7):
        x, z = rng.uniform(-1.0, 1.0) * size, rng.uniform(-0.6, 0.6) * size
        r = rng.uniform(0.6, 0.9) * size
        cp.leafy("clump", mats[i % 3], (x, rng.uniform(-0.3, 0.3), z), (r, r * 0.8, r * 0.7), rng,
                 scale=0.25, strength=0.4)
    leaves(rng, -1.2 * size, 1.2 * size, -0.4 * size, 8, y=-0.9 * size, size=(0.3, 0.45),
           up=False)


def leaf_clump(rng):
    leaf_clump_of(rng, 1.4)


def leaf_clump_small(rng):
    leaf_clump_of(rng, 0.8)


def nest(rng):
    twig = cr.material("twig", "#8a6a48", roughness=0.9)
    dark = cr.material("dark twig", "#6b4e33", roughness=0.9)
    egg = cr.material("egg", "#bfe0e6", roughness=0.4)
    cr.ring("nest", dark, 0.42, 0.15, location=(0, 0, 0.18), rotation=(0.15, 0, 0))
    for i in range(24):
        a = rng.uniform(-0.9, 0.9)
        x = rng.uniform(-0.55, 0.55)
        cp.tube("twig", (twig, dark)[i % 2],
                cp.curve((x - 0.3, -0.45, 0.12 + rng.uniform(0, 0.2)),
                         (x + 0.3, -0.45, 0.12 + rng.uniform(0, 0.2) + a * 0.2), steps=3),
                0.018, sides=5, subdivide=0)
    for dx in (-0.15, 0.12):
        cr.ball("egg", egg, (0.12, 0.12, 0.15), (dx, 0, 0.36))


def hanging_moss(rng):
    moss = [cr.material(f"hanging moss {c}", c, roughness=0.9, sheen=0.5)
            for c in ("#7aa860", "#5e9c48", "#8fb872")]
    for i in range(14):
        x = rng.uniform(-0.9, 0.9)
        length = rng.uniform(0.6, 2.0) * (1 - abs(x) * 0.4)
        points = cp.curve((x, rng.uniform(-0.2, 0.2), 0.05), (x + rng.uniform(-0.1, 0.1), 0,
                                                             -length),
                          steps=8, wobble=0.05, seed=i)
        cp.tube("strand", moss[i % 3], points, [0.07, 0.06, 0.06, 0.05, 0.05, 0.04, 0.03, 0.02, 0],
                sides=6)


def front_canopy(rng):
    mats = [cr.material(f"canopy {c}", c, roughness=0.7, sheen=0.3)
            for c in (cp.LEAF_DARK, "#357a3e")]
    for i in range(8):
        x = (i - 3.5) * 0.9 + rng.uniform(-0.2, 0.2)
        r = rng.uniform(0.7, 1.0)
        cp.leafy("canopy", mats[i % 2], (x, rng.uniform(-0.3, 0.3), 0.1), (r, r * 0.8, r * 0.6),
                 rng, scale=0.25, strength=0.4)
    leaves(rng, -3.6, 3.6, -0.3, 22, y=-0.8, size=(0.4, 0.7), up=False)


cp.render_set("treetops", [
    ("bough", bough), ("bough-end", bough_end), ("trunk", trunk), ("leaf-clump", leaf_clump),
    ("leaf-clump-small", leaf_clump_small), ("nest", nest), ("hanging-moss", hanging_moss),
    ("front-canopy", front_canopy),
])
