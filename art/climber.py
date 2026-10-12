"""The climber: Clamber's player character, as `sprites/player.json`.

A round little mountaineer in a bobble hat and scarf, with a backpack and
a coil of rope. Run by `tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/climber.py -- <content folder> [--quick]

`--quick` renders with few samples, for trying out changes.
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (between the feet), in world units. The body
# the game collides is 16 wide and 32 tall.
FRAME = (32, 40)
ORIGIN = (16, 38)

# Animations named after the player's states, plus `land` and `hurt`:
# (name, frames, ticks per frame, passes or None to loop).
ANIMATIONS = [
    ("idle", 12, 4, None),
    ("walk", 16, 2, None),
    ("run", 12, 2, None),
    ("roll", 16, 2, None),
    ("jump", 6, 2, 1),
    ("fall", 8, 3, 1),
    ("land", 6, 2, 1),
    ("swap", 5, 2, 1),
    ("hurt", 8, 3, 1),
]

COLORS = {
    "jacket": "#2f9e8f",
    "hat": "#e8553d",
    "trim": "#f3e3c3",
    "scarf": "#f2b134",
    "mittens": "#f2b134",
    "trousers": "#3b4a6b",
    "boots": "#7a4a2e",
    "soles": "#3a2a22",
    "skin": "#f1c7a1",
    "cheeks": "#ec9a8f",
    "nose": "#e9977a",
    "eyes": "#1f1a2e",
    "shine": "#ffffff",
    "pack": "#8a5a36",
    "flap": "#6e4528",
    "rope": "#d9b77e",
}

# The second roster slot's colours.
VARIANTS = {
    "alt": {
        "jacket": "#7d5ab5",
        "hat": "#3a8fd9",
        "scarf": "#8ccf4d",
        "mittens": "#8ccf4d",
        "pack": "#5d6b7a",
        "flap": "#46525e",
    }
}

# Bones: the near side faces the camera.
NEAR, FAR = -1, 1
BONES = [("root", (0, 0, 0), (0, 0, 0.5), None),
         ("hips", (0, 0, 1.05), (0, 0, 1.45), "root"),
         ("chest", (0, 0, 1.45), (0, 0, 1.95), "hips"),
         ("head", (0, 0, 1.95), (0, 0, 2.5), "chest")]
for side, s in (("near", NEAR), ("far", FAR)):
    BONES += [(f"arm.{side}", (0, 0.46 * s, 1.82), (0, 0.52 * s, 1.28), "chest"),
              (f"thigh.{side}", (0, 0.22 * s, 1.05), (0, 0.22 * s, 0.58), "hips"),
              (f"shin.{side}", (0, 0.22 * s, 0.58), (0, 0.22 * s, 0.12), f"thigh.{side}")]


def build():
    m = {name: cr.material(name, color) for name, color in COLORS.items()}
    m["shine"].node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.15
    m["eyes"].node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.2
    for soft in ("hat", "trim", "scarf", "mittens"):
        m[soft].node_tree.nodes["Principled BSDF"].inputs["Sheen Weight"].default_value = 0.6

    rig = cr.Rig("climber", BONES, yaw=-36)
    rig.attach("hips", cr.ball("seat", m["trousers"], (0.4, 0.38, 0.3), (0, 0, 1.1)))
    rig.attach(
        "chest",
        cr.ball("body", m["jacket"], (0.46, 0.44, 0.52), (0.02, 0, 1.52)),
        cr.ring("scarf", m["scarf"], 0.3, 0.11, (0.02, 0, 1.97)),
        cr.box("scarf end", m["scarf"], (0.14, 0.1, 0.42), (-0.3, -0.18, 1.76), (0.3, 0.35, 0)),
        cr.box("pack", m["pack"], (0.38, 0.62, 0.72), (-0.52, 0, 1.55)),
        cr.box("flap", m["flap"], (0.42, 0.66, 0.22), (-0.5, 0, 1.87)),
        cr.ring("rope", m["rope"], 0.2, 0.065, (-0.52, -0.36, 1.48), (math.pi / 2, 0, 0)),
    )
    rig.attach(
        "head",
        cr.ball("face", m["skin"], (0.6, 0.6, 0.58), (0.04, 0, 2.45)),
        cr.ball("hat", m["hat"], (0.63, 0.63, 0.45), (0.02, 0, 2.72)),
        cr.ring("cuff", m["trim"], 0.56, 0.09, (0.04, 0, 2.66), (0, -0.12, 0)),
        cr.ball("bobble", m["trim"], (0.16, 0.16, 0.16), (-0.08, 0, 3.2)),
        cr.ball("nose", m["nose"], (0.09, 0.09, 0.08), (0.64, 0, 2.34)),
    )
    for name, s in (("near", NEAR), ("far", FAR)):
        eye = cr.ball(f"eye.{name}", m["eyes"], (0.07, 0.1, 0.16), (0.56, 0.21 * s, 2.45))
        shine = cr.ball(f"shine.{name}", m["shine"], (0.035, 0.04, 0.045), (0.62, 0.19 * s, 2.51))
        cheek = cr.ball(f"cheek.{name}", m["cheeks"], (0.05, 0.11, 0.07), (0.52, 0.33 * s, 2.3))
        rig.attach("head", eye, shine, cheek)
        rig.attach(
            f"arm.{name}",
            cr.limb(f"sleeve.{name}", m["jacket"], (0, 0.46 * s, 1.82), (0, 0.52 * s, 1.32), 0.13),
            cr.ball(f"mitten.{name}", m["mittens"], (0.16, 0.15, 0.17), (0.02, 0.52 * s, 1.22)),
        )
        rig.attach(
            f"thigh.{name}",
            cr.limb(f"thigh.{name}", m["trousers"], (0, 0.22 * s, 1.05), (0, 0.22 * s, 0.58), 0.15),
        )
        rig.attach(
            f"shin.{name}",
            cr.limb(f"shin.{name}", m["trousers"], (0, 0.22 * s, 0.6), (0, 0.22 * s, 0.2), 0.13),
            cr.box(f"boot.{name}", m["boots"], (0.44, 0.27, 0.24), (0.08, 0.22 * s, 0.15)),
            cr.box(f"sole.{name}", m["soles"], (0.46, 0.29, 0.07), (0.08, 0.22 * s, 0.035)),
        )
    return rig


def blink(rig, amount):
    """Closes the eyes by `amount`, 0 to 1."""
    for part in rig.parts:
        if part.name.startswith(("eye.", "shine.")):
            x, y, z = part["rest_scale"]
            part.scale = (x, y, z * (1 - 0.85 * amount))


def legs(rig, near, far, near_knee=0.0, far_knee=0.0):
    """Swings the thighs forward and bends the knees, in degrees."""
    rig.turn("thigh.near", near)
    rig.turn("thigh.far", far)
    rig.turn("shin.near", -near_knee)
    rig.turn("shin.far", -far_knee)


def arms(rig, near, far):
    """Swings the arms forward, in degrees."""
    rig.turn("arm.near", near)
    rig.turn("arm.far", far)


def ease(t):
    return t * t * (3 - 2 * t)


def pose(rig, name, i, count):
    t = i / count
    p = 2 * math.pi * t
    if name == "idle":
        breath = math.sin(p)
        rig.squash("hips", 0.015 * breath)
        rig.move("root", z=-0.15 * (1 + breath))
        rig.turn("chest", 1.5 * breath)
        rig.turn("head", -2 * breath)
        arms(rig, 4 + 3 * breath, -2 - 3 * breath)
        legs(rig, 3, -3)
        blink(rig, 1.0 if i == 9 else 0.5 if i in (8, 10) else 0.0)
    elif name == "walk":
        # two steps per cycle; the near leg leads first
        swing = math.sin(p)
        rig.move("root", z=0.5 * math.cos(2 * p) - 0.3)
        rig.turn("hips", -4)
        rig.turn("head", 3)
        legs(rig, 28 * swing, -28 * swing,
             35 * max(0.0, math.cos(p)) + 5, 35 * max(0.0, -math.cos(p)) + 5)
        arms(rig, -24 * swing, 24 * swing)
        rig.turn("chest", 3 * math.sin(2 * p))
    elif name == "run":
        swing = math.sin(p)
        rig.move("root", z=1.0 * abs(math.cos(p)) - 0.8)
        rig.turn("hips", -14)
        rig.turn("head", 10)
        legs(rig, 48 * swing, -48 * swing,
             80 * max(0.0, math.cos(p)) + 15, 80 * max(0.0, -math.cos(p)) + 15)
        arms(rig, -55 * swing + 10, 55 * swing + 10)
        rig.squash("hips", 0.04 * math.cos(2 * p))
    elif name == "roll":
        # tucked into a ball, a full turn forward per cycle
        rig.turn("hips", -360 * t)
        rig.turn("chest", -55)
        rig.turn("head", -40)
        legs(rig, 110, 100, 135, 130)
        arms(rig, 95, 80)
        rig.ground()
    elif name == "jump":
        # from a launching stretch to tucking up for the climb
        e = ease(t)
        rig.squash("root", -0.08 * (1 - e))
        rig.turn("hips", -6 + 4 * e)
        arms(rig, 40 + 110 * e, 20 + 80 * e)
        legs(rig, -10 + 50 * e, -20 + 10 * e, 70 * e, 20 + 10 * e)
        rig.turn("head", 8)
    elif name == "fall":
        # over the top with arms out, then reaching up as it drops
        e = ease(t)
        rig.turn("hips", 4 - 6 * e)
        arms(rig, 150 - 10 * e + 8 * math.sin(4 * p), 100 + 50 * e - 8 * math.sin(4 * p))
        legs(rig, 40 - 30 * e, 10 - 25 * e, 70 - 40 * e, 30)
        rig.turn("head", 8 - 14 * e)
    elif name == "land":
        # squashes on impact, then springs back
        k = math.sin(math.pi * min(1.0, t * 1.6)) * (1 - t)
        rig.squash("root", 0.2 * k)
        rig.turn("hips", -10 * k)
        legs(rig, 30 * k, 20 * k, 50 * k, 45 * k)
        arms(rig, 20 + 30 * k, -10 + 30 * k)
    elif name == "swap":
        # a twirl on the spot
        rig.turn("root", 360 * ease(t), axis=(0, 0, 1))
        rig.squash("root", -0.05 * math.sin(math.pi * t))
        arms(rig, 70, 70)
        rig.turn("arm.near", -50, axis=(1, 0, 0))
        rig.turn("arm.far", 50, axis=(1, 0, 0))
        legs(rig, 5, -5)
    elif name == "hurt":
        # knocked back, then slumping
        e = ease(min(1.0, t * 2))
        rig.turn("hips", 22 * e - 6 * ease(t))
        rig.turn("head", 14 * e)
        arms(rig, 130 * e, 160 * e)
        legs(rig, 30 * e, -10 * e, 30 * e, 10)
        blink(rig, 1.0)
    else:
        raise ValueError(name)


cr.run(
    "player", build, pose, FRAME, ORIGIN, ANIMATIONS, variants=VARIANTS,
    hurtbox=lambda rig: [p for p in rig.parts if not p.name.startswith("bobble")],
)
