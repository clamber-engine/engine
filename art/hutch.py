"""The hutch: a slatted crate with an animal inside, as `sprites/hutch.json`.

The crate is a cage of green-stained slats with eyes peering out between
them; it breaks apart, and the animal it held, a stout shelled trundler
with four short legs, stands where it was. Run by `tools/render-art.sh`, or
on its own:

    blender -b --factory-startup -P art/hutch.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle of its base), in world units. The
# crate is 26 across; the animal about 30 across and 22 tall.
FRAME = (56, 48)
ORIGIN = (28, 44)

# (name, frames, ticks per frame, passes or None to loop).
ANIMATIONS = [
    ("whole", 1, 6, None),
    ("break", 8, 2, 1),
    ("animal", 8, 4, None),
]

SIDE = 2.6
SLAT = 0.34
GAP = (SIDE - 4 * SLAT) / 3

# Each panel: its bone, where it sits, how its slats run, and how it flies
# when the crate breaks (sideways and up in world units, and a spin in
# degrees).
PANELS = [
    ("front", (0, -SIDE / 2 + 0.06, SIDE / 2), "x", (-4, 8, -60)),
    ("back", (0, SIDE / 2 - 0.06, SIDE / 2), "x", (4, 6, 45)),
    ("left", (-SIDE / 2 + 0.06, 0, SIDE / 2), "y", (-10, 5, 75)),
    ("right", (SIDE / 2 - 0.06, 0, SIDE / 2), "y", (10, 6, -85)),
    ("lid", (0, 0, SIDE - 0.06), "lid", (2, 14, 130)),
]

COLORS = {
    "slats": "#6f9a5a",
    "frame": "#3f5e34",
    "shell": "#c9733e",
    "ridges": "#8e4a26",
    "skin": "#e8c48f",
    "legs": "#7a5a3c",
    "eyes": "#ffffff",
    "pupils": "#1f1a2e",
}

NEAR, FAR = -1, 1
LEGS = [(-0.75, "back"), (0.75, "front")]


def bones():
    out = [("root", (0, 0, 0), (0, 0, 0.5), None)]
    out += [(name, at, (at[0], at[1], at[2] + 0.3), "root") for name, at, _r, _f in PANELS]
    out.append(("peek", (0.2, 0, 1.1), (0.2, 0, 1.4), "root"))
    out.append(("animal", (0, 0, 0.3), (0, 0, 1.0), "root"))
    out.append(("head", (1.3, 0, 1.0), (1.7, 0, 1.2), "animal"))
    for side, s in (("near", NEAR), ("far", FAR)):
        for x, n in LEGS:
            out.append((f"leg.{side}.{n}", (x, 0.6 * s, 0.6), (x, 0.7 * s, 0.05), "animal"))
    return out


def panel(name, at, run, m):
    """The slats of one side of the crate, and two bars across them."""
    parts = []
    x, y, z = at
    for k in range(4):
        o = -SIDE / 2 + SLAT / 2 + k * (SLAT + GAP)
        if run == "x":
            parts.append(cr.box(f"{name}.{k}", m["slats"], (SLAT, 0.12, SIDE), (x + o, y, z),
                                bevel=0.2))
        elif run == "y":
            parts.append(cr.box(f"{name}.{k}", m["slats"], (0.12, SLAT, SIDE), (x, y + o, z),
                                bevel=0.2))
        else:
            parts.append(cr.box(f"{name}.{k}", m["slats"], (SLAT, SIDE - 0.24, 0.12),
                                (x + o, y, z), bevel=0.2))
    if run != "lid":
        for k, dz in enumerate((SIDE / 2 - 0.2, -SIDE / 2 + 0.2)):
            size = (SIDE, 0.16, 0.26) if run == "x" else (0.16, SIDE, 0.26)
            out = -0.08 if (run == "x") == (y < 0 or x < 0) else 0.08
            loc = (x, y + out, z + dz) if run == "x" else (x + out, y, z + dz)
            parts.append(cr.box(f"{name}.bar.{k}", m["frame"], size, loc, bevel=0.3))
    return parts


def build():
    m = {name: cr.material(name, color) for name, color in COLORS.items()}
    m["shell"].node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.4
    m["eyes"].node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.2

    rig = cr.Rig("hutch", bones(), yaw=-25)
    for name, at, run, _fly in PANELS:
        rig.attach(name, *panel(name, at, run, m))
    # eyes peering out from the dark inside
    for side, s in (("near", NEAR), ("far", FAR)):
        rig.attach(
            "peek",
            cr.ball(f"peek.{side}", m["eyes"], (0.13, 0.13, 0.18), (0.25 * s, -0.9, 1.25)),
            cr.ball(f"peek.pupil.{side}", m["pupils"], (0.06, 0.06, 0.09),
                    (0.25 * s + 0.04, -1.02, 1.22)),
        )

    rig.attach(
        "animal",
        cr.ball("shell", m["shell"], (1.45, 1.0, 0.85), (-0.1, 0, 1.15)),
        cr.ring("rim", m["ridges"], 1.0, 0.1, (-0.1, 0, 0.75), (0, 0, 0)),
        cr.ball("ridge.0", m["ridges"], (0.18, 0.75, 0.12), (-0.55, 0, 1.85), (0, 0.3, 0)),
        cr.ball("ridge.1", m["ridges"], (0.18, 0.85, 0.12), (0.15, 0, 1.92), (0, -0.1, 0)),
        cr.ball("belly", m["skin"], (1.25, 0.85, 0.35), (0, 0, 0.7)),
        cr.ball("tail", m["skin"], (0.35, 0.22, 0.2), (-1.5, 0, 0.75), (0, 0.4, 0)),
    )
    rig.attach(
        "head",
        cr.ball("face", m["skin"], (0.55, 0.5, 0.48), (1.5, 0, 1.05)),
        cr.ball("snout", m["skin"], (0.3, 0.32, 0.25), (1.95, 0, 0.92)),
    )
    for side, s in (("near", NEAR), ("far", FAR)):
        rig.attach(
            "head",
            cr.ball(f"eye.{side}", m["eyes"], (0.15, 0.15, 0.2), (1.78, 0.25 * s, 1.3)),
            cr.ball(f"pupil.{side}", m["pupils"], (0.06, 0.08, 0.11), (1.9, 0.25 * s, 1.3)),
        )
        for x, n in LEGS:
            rig.attach(
                f"leg.{side}.{n}",
                cr.limb(f"leg.{side}.{n}", m["legs"], (x, 0.6 * s, 0.6), (x, 0.7 * s, 0.12), 0.18),
                cr.ball(f"foot.{side}.{n}", m["legs"], (0.24, 0.18, 0.1), (x + 0.08, 0.7 * s, 0.1)),
            )
    return rig


def hide(rig, test):
    for part in rig.parts:
        if test(part.name):
            part.scale = (0.001, 0.001, 0.001)


def crate_part(name):
    return name.split(".")[0] in {p[0] for p in PANELS} | {"peek"}


def pose(rig, name, i, count):
    if name == "whole":
        hide(rig, lambda n: not crate_part(n))
        return
    if name == "break":
        hide(rig, lambda n: not crate_part(n))
        t = (i + 1) / count
        for board, _at, _run, (dx, dz, spin) in PANELS:
            rig.move(board, x=dx * t, z=dz * t - 18 * t * t)
            rig.turn(board, spin * t)
        shrink = max(0.001, 1 - t * t)
        for part in rig.parts:
            if crate_part(part.name):
                x, y, z = part["rest_scale"]
                part.scale = (x * shrink, y * shrink, z * shrink)
        return
    if name != "animal":
        raise ValueError(name)
    hide(rig, crate_part)
    # breathing, and a slow look around
    p = 2 * math.pi * i / count
    rig.squash("animal", 0.04 * math.sin(p))
    rig.turn("head", 6 * math.sin(p))
    for side in ("near", "far"):
        for _x, n in LEGS:
            rig.turn(f"leg.{side}.{n}", 3 * math.sin(p + (n == "front")))


cr.run("hutch", build, pose, FRAME, ORIGIN, ANIMATIONS)
