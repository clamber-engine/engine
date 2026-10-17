"""The idol, a golden figure of an animal: a small seated beast on a
plinth that turns and shines, as `sprites/idol.json`.

Run by `tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/idol.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle), in world units.
FRAME = (24, 24)
ORIGIN = (12, 12)

# (name, frames, ticks per frame, passes or None to loop): one turn
# there and back.
ANIMATIONS = [("shine", 20, 3, None)]


def build():
    gold = cr.material("gold", "#f2b134", roughness=0.2)
    gold.node_tree.nodes["Principled BSDF"].inputs["Metallic"].default_value = 1.0
    plinth = cr.material("plinth", "#b8742a", roughness=0.35)
    plinth.node_tree.nodes["Principled BSDF"].inputs["Metallic"].default_value = 1.0
    eye, _ = cr.glow("eye", "#fff2c8", 3.0)

    rig = cr.Rig("idol", [("root", (0, 0, -0.9), (0, 0, 0.9), None)])
    rig.attach(
        "root",
        cr.box("plinth", plinth, (1.1, 0.7, 0.3), (0, 0, -0.75), bevel=0.2),
        cr.ball("body", gold, (0.42, 0.36, 0.5), (0, 0, -0.15)),
        cr.ball("head", gold, (0.3, 0.28, 0.28), (0, -0.05, 0.5)),
        cr.ball("ear.l", gold, (0.1, 0.06, 0.14), (-0.22, -0.02, 0.78)),
        cr.ball("ear.r", gold, (0.1, 0.06, 0.14), (0.22, -0.02, 0.78)),
        cr.ball("snout", gold, (0.13, 0.12, 0.1), (0, -0.3, 0.42)),
        cr.ball("eye.l", eye, (0.05, 0.03, 0.05), (-0.11, -0.27, 0.56)),
        cr.ball("eye.r", eye, (0.05, 0.03, 0.05), (0.11, -0.27, 0.56)),
        cr.limb("tail", gold, (0.3, 0.2, -0.45), (0.5, 0.05, 0.05), 0.06),
    )
    return rig


def pose(rig, name, i, count):
    if name != "shine":
        raise ValueError(name)
    rig.turn("root", 35 * math.sin(2 * math.pi * i / count), axis=(0, 0, 1))


cr.run("idol", build, pose, FRAME, ORIGIN, ANIMATIONS)
