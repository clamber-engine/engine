"""The checkpoint: a trail marker that raises its pennant when reached, as
`sprites/checkpoint.json`.

A wooden pole in a little cairn of stones, with a pennant that waits
furled at its foot and flies from the top once reached. Run by
`tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/checkpoint.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the pole's foot), in world units.
FRAME = (32, 52)
ORIGIN = (10, 50)

# (name, frames, ticks per frame, passes or None to loop).
ANIMATIONS = [
    ("waiting", 1, 6, None),
    ("reached", 12, 2, None),
]

COLORS = {
    "pole": "#a0703f",
    "cap": "#f2b134",
    "stones": "#8d8a9b",
    "dark stones": "#6c6979",
    "pennant": "#e8553d",
    "stripe": "#f3e3c3",
}

# The pennant is three strips, each hanging from the one before so the
# cloth can ripple.
STRIPS = 3
LENGTH = 1.5
TOP = 4.2
BONES = [("root", (0, 0, 0), (0, 0, 0.5), None),
         ("hoist", (0, 0, TOP), (0, 0, TOP + 0.2), "root")]
for n in range(STRIPS):
    x = 0.1 + LENGTH * n / STRIPS
    BONES.append((f"strip.{n}", (x, 0, TOP), (x + LENGTH / STRIPS, 0, TOP),
                  "hoist" if n == 0 else f"strip.{n - 1}"))


def build():
    m = {name: cr.material(name, color) for name, color in COLORS.items()}
    m["stones"].node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.9
    m["pennant"].node_tree.nodes["Principled BSDF"].inputs["Sheen Weight"].default_value = 0.5

    rig = cr.Rig("checkpoint", BONES, yaw=-20)
    rig.attach(
        "root",
        cr.lathe("pole", m["pole"], [(0.11, 0), (0.1, TOP + 0.3), (0, TOP + 0.32)], segments=16),
        cr.ball("cap", m["cap"], (0.18, 0.18, 0.18), (0, 0, TOP + 0.35)),
        cr.ball("stone.0", m["stones"], (0.55, 0.45, 0.3), (-0.25, 0.1, 0.18)),
        cr.ball("stone.1", m["dark stones"], (0.45, 0.4, 0.28), (0.4, -0.1, 0.16)),
        cr.ball("stone.2", m["stones"], (0.35, 0.3, 0.24), (0.05, -0.3, 0.42), (0.3, 0.2, 0)),
        cr.ball("stone.3", m["dark stones"], (0.3, 0.3, 0.2), (-0.1, 0.35, 0.45)),
    )
    # the pennant tapers from the pole to its tip
    for n in range(STRIPS):
        x0 = 0.1 + LENGTH * n / STRIPS
        x1 = 0.1 + LENGTH * (n + 1) / STRIPS
        h0 = 0.5 * (1 - n / STRIPS) + 0.04
        h1 = 0.5 * (1 - (n + 1) / STRIPS) + 0.04
        mat = m["stripe"] if n == 1 else m["pennant"]
        rig.attach(f"strip.{n}", cr.prism(
            f"strip.{n}", mat,
            [(x0, TOP - h0), (x1, TOP - h1), (x1, TOP + h1), (x0, TOP + h0)],
            0.05, bevel=0.015,
        ))
    return rig


def pose(rig, name, i, count):
    if name == "waiting":
        # furled low on the pole, hanging limp
        rig.move("hoist", z=-(TOP - 1.4) * 10)
        rig.turn("strip.0", -70)
        rig.turn("strip.1", -15)
        rig.turn("strip.2", -5)
    elif name == "reached":
        p = 2 * math.pi * i / count
        for n in range(STRIPS):
            rig.turn(f"strip.{n}", 12 * math.sin(p - 1.2 * n) - 4)
            rig.turn(f"strip.{n}", 25 * math.sin(p - 1.2 * n + 0.6), axis=(0, 0, 1))
    else:
        raise ValueError(name)


cr.run("checkpoint", build, pose, FRAME, ORIGIN, ANIMATIONS)
