"""The goal: a stone summit gate with a glowing heart, as `sprites/goal.json`.

An arch of rough stone blocks with light pulsing in its doorway. Run by
`tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/goal.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle of its base), in world units.
FRAME = (40, 68)
ORIGIN = (20, 66)

# (name, frames, ticks per frame, passes or None to loop).
ANIMATIONS = [("glow", 16, 2, None)]

# The doorway: its half width, and the height where its arch begins.
HALF = 0.95
SPRING = 4.2
# How thick the stone is around it.
WALL = 0.55


def arch(half, spring, segments=12):
    """The doorway's outline, from its bottom left, over the arch and down."""
    points = [(-half, 0.0)]
    for k in range(segments + 1):
        a = math.pi - math.pi * k / segments
        points.append((half * math.cos(a), spring + half * math.sin(a)))
    points.append((half, 0.0))
    return points


def build():
    stone = cr.material("stone", "#8d8a9b", roughness=0.9)
    dark = cr.material("dark stone", "#6c6979", roughness=0.9)
    moss = cr.material("moss", "#6bab4a", roughness=0.8)
    light, glow = cr.glow("light", "#e8902a", 0.6)
    spark, _spark = cr.glow("spark", "#fff3c0", 4.0)

    rig = cr.Rig("goal", [("root", (0, 0, 0), (0, 0, 1), None),
                          ("motes", (0, 0, 1), (0, 0, 2), "root")], yaw=-15)
    rig.track(glow, "default_value")
    inner = arch(HALF, SPRING)
    outer = arch(HALF + WALL, SPRING)
    # the stonework is the outer arch with the doorway cut out of it
    outline = list(reversed(outer)) + inner
    rig.attach(
        "root",
        cr.prism("gate", stone, outline, 0.7, bevel=0.06),
        cr.prism("door", light, arch(HALF + 0.05, SPRING), 0.1, (0, 0.25, 0), bevel=0),
        cr.box("step", dark, (2 * (HALF + WALL) + 0.5, 1.0, 0.3), (0, 0, 0.15), bevel=0.2),
        cr.box("keystone", dark, (0.5, 0.85, 0.6), (0, 0, SPRING + HALF + 0.3), bevel=0.25),
        cr.ball("moss.0", moss, (0.45, 0.42, 0.18), (-HALF - 0.4, -0.1, SPRING + 0.55), (0, 0.6, 0)),
        cr.ball("moss.1", moss, (0.35, 0.42, 0.14), (HALF + 0.3, -0.1, 1.2), (0, -0.2, 0)),
    )
    # blocks set into the pillars
    for k, (x, z) in enumerate([(-1, 0.9), (1, 2.1), (-1, 3.0), (1, 3.7)]):
        rig.attach("root", cr.box(
            f"block.{k}", dark, (WALL * 0.8, 0.76, 0.5),
            (x * (HALF + WALL / 2), 0, z), bevel=0.3,
        ))
    # motes of light rising through the doorway in two files, so that
    # rising two places brings each to where another began
    for k in range(6):
        x = -0.35 if k % 2 else 0.35
        rig.attach("motes", cr.ball(f"mote.{k}", spark, (0.1, 0.1, 0.1), (x, -0.2, 0.2 + k)))
    return rig


def pose(rig, name, i, count):
    if name != "glow":
        raise ValueError(name)
    p = 2 * math.pi * i / count
    rig.tracks[0][0].default_value = 0.6 + 0.3 * math.sin(p)
    rise = 2 * i / count
    rig.move("motes", z=10 * rise)
    # motes fade out before they reach the arch
    for part in rig.parts:
        if part.name.startswith("mote."):
            z = 0.2 + int(part.name.split(".")[1]) + rise
            x, y, _z = part["rest_scale"]
            size = max(0.001, min(1.0, (4.8 - z) / 0.8))
            part.scale = (x * size, y * size, x * size)


cr.run("goal", build, pose, FRAME, ORIGIN, ANIMATIONS)
