"""The tonic, the pickup that gives back a hit point: a round glass flask
of glowing green liquid with a cork, bobbing and rocking, as
`sprites/tonic.json`. Run by `tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/tonic.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle), in world units.
FRAME = (20, 20)
ORIGIN = (10, 10)

# (name, frames, ticks per frame, passes or None to loop): one bob.
ANIMATIONS = [("bob", 16, 3, None)]


def build():
    glass = cr.material("glass", "#d8f4ff", roughness=0.05)
    shader = glass.node_tree.nodes["Principled BSDF"]
    shader.inputs["Coat Weight"].default_value = 1.0
    shader.inputs["Alpha"].default_value = 0.25
    liquid, _ = cr.glow("liquid", "#22c84a", 0.8)
    liquid.node_tree.nodes["Principled BSDF"].inputs["Coat Weight"].default_value = 1.0
    cork = cr.material("cork", "#a8743c", roughness=0.85)
    shine, _ = cr.glow("shine", "#ffffff", 3.0)
    rig = cr.Rig("tonic", [("root", (0, 0, -0.8), (0, 0, 0.8), None)])
    rig.attach(
        "root",
        cr.lathe(
            "flask", glass,
            [(0, -0.72), (0.38, -0.66), (0.56, -0.36), (0.56, -0.08), (0.36, 0.2),
             (0.18, 0.32), (0.18, 0.56), (0.22, 0.62), (0, 0.62)],
        ),
        cr.lathe(
            "liquid", liquid,
            [(0, -0.66), (0.34, -0.6), (0.5, -0.36), (0.5, -0.08), (0.32, 0.14), (0, 0.14)],
        ),
        cr.lathe("cork", cork, [(0, 0.5), (0.15, 0.5), (0.17, 0.82), (0, 0.84)]),
        cr.ball("shine", shine, (0.08, 0.04, 0.16), (-0.3, -0.5, -0.2)),
    )
    return rig


def pose(rig, name, i, count):
    if name != "bob":
        raise ValueError(name)
    t = 2 * math.pi * i / count
    rig.move("root", z=0.08 * math.sin(t))
    rig.turn("root", 12 * math.sin(t))


cr.run("tonic", build, pose, FRAME, ORIGIN, ANIMATIONS)
