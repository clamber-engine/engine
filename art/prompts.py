"""Button prompts, as `ui/prompts.json`: the keys and gamepad controls the
controls screen shows bindings as, one still frame each.

- `key`: a keycap. The game stretches its middle to fit the key's name,
  which it writes on the cap as text.
- `pill`: a shoulder button, or a pad's small middle buttons, stretched
  like the key and labelled by the game.
- `trigger`: a trigger, labelled by the game.
- `face-letters-<position>`: the face buttons coloured as on pads that
  label them with letters, which the game writes on them.
- `face-shapes-<position>`: the face buttons of pads that mark them with
  shapes: a cross, a circle, a square and a triangle.
- `dpad-<direction>`: the directional pad with one arm lit.
- `stick-<direction>` and `stick-press`: a stick pushed one way, with an
  arrow, or pressed in, with a ring. The game writes which stick on it.

Rendered at high resolution and unlit, so they stay sharp at 4K. Run by
`tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/prompts.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle), in world units. The game draws
# every prompt at the same view units per world unit (`WORLD` in
# `crates/game/src/prompts.rs`).
FRAME = (12, 12)
ORIGIN = (6, 6)
# Texels per world unit: 120 texels a frame.
SCALE = 10

POSITIONS = ["south", "east", "west", "north"]
DIRECTIONS = ["up", "down", "left", "right"]
PIECES = (
    ["key", "pill", "trigger"]
    + [f"face-letters-{p}" for p in POSITIONS]
    + [f"face-shapes-{p}" for p in POSITIONS]
    + [f"dpad-{d}" for d in DIRECTIONS]
    + [f"stick-{d}" for d in DIRECTIONS]
    + ["stick-press"]
)
ANIMATIONS = [(piece, 1, 1, None) for piece in PIECES]

# How far the pieces not being drawn are moved away, in world units.
AWAY = 1000

# Each direction as `(x, z)` on the camera's picture.
TOWARDS = {"up": (0, 1), "down": (0, -1), "left": (-1, 0), "right": (1, 0)}

# The face buttons of lettered pads, by position.
LETTER_COLORS = {"south": "#3aa848", "east": "#d0303c", "west": "#2c68d0", "north": "#e8b020"}
# The shapes' colours, by position.
SHAPE_COLORS = {"south": "#7ab4f4", "east": "#f06478", "west": "#e894d4", "north": "#4cd8b0"}

# Facing the camera: a lathe's axis turned from +Z to -Y.
FACING = (math.pi / 2, 0, 0)


def rounded(w, h, r, segments=6):
    """A `w` by `h` outline with corners rounded by `r`."""
    outline = []
    for cx, cz, start in ((w / 2 - r, h / 2 - r, 0), (-(w / 2 - r), h / 2 - r, 90),
                          (-(w / 2 - r), -(h / 2 - r), 180), (w / 2 - r, -(h / 2 - r), 270)):
        for i in range(segments + 1):
            a = math.radians(start + 90 * i / segments)
            outline.append((cx + r * math.cos(a), cz + r * math.sin(a)))
    return outline


def stadium(w, h):
    """A `w` by `h` outline with round ends."""
    return rounded(w, h, h / 2 - 1e-4, segments=12)


def plastic(name, color, roughness=0.35):
    mat = cr.material(name, color, roughness=roughness)
    mat.node_tree.nodes["Principled BSDF"].inputs["Coat Weight"].default_value = 0.6
    return mat


def dome(name, mat, radius):
    """A round button facing the camera, domed a little."""
    return cr.lathe(
        name, mat,
        [(0, -0.1), (radius, -0.1), (radius, 0.04), (radius * 0.8, 0.1), (0, 0.12)],
        rotation=FACING,
    )


def arrow(name, mat, centre, towards, size):
    """A flat arrowhead at `centre`, pointing `towards`."""
    dx, dz = towards
    tip = (centre[0] + dx * size, centre[1] + dz * size)
    back = (centre[0] - dx * size * 0.6, centre[1] - dz * size * 0.6)
    side = (-dz * size, dx * size)
    outline = [tip, (back[0] - side[0], back[1] - side[1]), (back[0] + side[0], back[1] + side[1])]
    # Counterclockwise as the camera sees it.
    if (outline[1][0] - tip[0]) * (outline[2][1] - tip[1]) - \
            (outline[1][1] - tip[1]) * (outline[2][0] - tip[0]) < 0:
        outline = [outline[0], outline[2], outline[1]]
    return cr.prism(name, mat, outline, 0.06, (0, -0.2, 0), bevel=0.01)


def shape(rig, bone, position, mat):
    """The shape marking the face button at `position`, on its front."""
    y, r = -0.16, 0.035
    if position == "south":
        d = 0.18
        parts = [cr.limb(f"{bone}.a", mat, (-d, y, -d), (d, y, d), r),
                 cr.limb(f"{bone}.b", mat, (-d, y, d), (d, y, -d), r)]
    elif position == "east":
        parts = [cr.ring(f"{bone}.ring", mat, 0.2, r, (0, y, 0), FACING)]
    elif position == "west":
        d = 0.17
        corners = [(-d, -d), (d, -d), (d, d), (-d, d)]
        parts = [cr.limb(f"{bone}.{i}", mat, (a[0], y, a[1]), (b[0], y, b[1]), r)
                 for i, (a, b) in enumerate(zip(corners, corners[1:] + corners[:1]))]
    else:
        corners = [(0, 0.22), (-0.21, -0.14), (0.21, -0.14)]
        parts = [cr.limb(f"{bone}.{i}", mat, (a[0], y, a[1]), (b[0], y, b[1]), r)
                 for i, (a, b) in enumerate(zip(corners, corners[1:] + corners[:1]))]
    rig.attach(bone, *parts)


def build():
    rig = cr.Rig("prompts", [(piece, (0, 0, -0.9), (0, 0, 0.9), None) for piece in PIECES])

    # A keycap: a pale top in a darker skirt.
    skirt = plastic("skirt", "#a0a4ac", roughness=0.5)
    top = plastic("top", "#eeece6", roughness=0.4)
    rig.attach(
        "key",
        cr.box("key.skirt", skirt, (1.04, 0.3, 1.0), (0, 0, 0), bevel=0.3),
        cr.box("key.top", top, (0.86, 0.12, 0.8), (0, -0.16, 0.04), bevel=0.4),
    )

    dark = plastic("dark", "#4a505c")
    rim = plastic("rim", "#a4aab4", roughness=0.5)
    rig.attach(
        "pill",
        cr.prism("pill.rim", rim, stadium(1.08, 0.7), 0.2, (0, 0, 0), bevel=0.04),
        cr.prism("pill.body", dark, stadium(0.98, 0.6), 0.2, (0, -0.06, 0), bevel=0.06),
    )

    # A trigger seen from the front: taller than wide, its top an arch.
    outline = []
    # Counterclockwise from the bottom left: round bottom corners, then
    # the arch from right to left.
    for cx, start in ((-0.28, 180), (0.28, 270)):
        for i in range(7):
            t = math.radians(start + 90 * i / 6)
            outline.append((cx + 0.14 * math.cos(t), -0.36 + 0.14 * math.sin(t)))
    for i in range(17):
        t = math.pi * i / 16
        outline.append((0.42 * math.cos(t), -0.22 + 0.7 * math.sin(t) ** 0.7))
    rig.attach(
        "trigger",
        cr.prism("trigger.rim", rim, outline, 0.2, (0, 0, 0), bevel=0.04),
        cr.prism("trigger.body", dark, [(x * 0.86, z * 0.88 - 0.01) for x, z in outline], 0.2,
                 (0, -0.06, 0), bevel=0.06),
    )

    for position in POSITIONS:
        bone = f"face-letters-{position}"
        rig.attach(bone, dome(bone, plastic(bone, LETTER_COLORS[position], 0.25), 0.5))
        bone = f"face-shapes-{position}"
        rig.attach(bone, dome(bone, dark, 0.5))
        shape(rig, bone, position, cr.glow(f"{bone}.mark", SHAPE_COLORS[position], 1.5)[0])

    # The directional pad: a plus with one arm lit and an arrow on it.
    lit = plastic("lit", "#f2c84a", 0.3)
    for direction in DIRECTIONS:
        bone = f"dpad-{direction}"
        arm, half = 0.36, 0.52
        rig.attach(
            bone,
            cr.box(f"{bone}.across", dark, (2 * half, 0.22, arm), bevel=0.25),
            cr.box(f"{bone}.down", dark, (arm, 0.22, 2 * half), bevel=0.25),
        )
        dx, dz = TOWARDS[direction]
        size = (arm, 0.1, half - 0.08) if dx == 0 else (half - 0.08, 0.1, arm)
        middle = (dx * (half / 2 + 0.06), -0.1, dz * (half / 2 + 0.06))
        rig.attach(
            bone,
            cr.box(f"{bone}.lit", lit, (size[0] * 0.92, size[1], size[2] * 0.92), middle,
                   bevel=0.3),
            arrow(f"{bone}.arrow", dark, (dx * 0.3, dz * 0.3), (dx, dz), 0.1),
        )

    # A stick's cap, pushed a little the way it's going, and an arrow.
    cap = plastic("cap", "#565c68", 0.45)
    for direction in DIRECTIONS + ["press"]:
        bone = f"stick-{direction}"
        dx, dz = TOWARDS.get(direction, (0, 0))
        well = cr.lathe(f"{bone}.well", rim, [(0, -0.08), (0.36, -0.08), (0.36, 0.0), (0, 0.0)],
                        location=(0, 0.1, 0), rotation=FACING)
        top = cr.lathe(
            f"{bone}.cap", cap,
            [(0, -0.1), (0.28, -0.1), (0.3, 0.04), (0.27, 0.1), (0.19, 0.06), (0, 0.05)],
            location=(dx * 0.05, -0.02, dz * 0.05), rotation=FACING,
        )
        rig.attach(bone, well, top)
        if direction == "press":
            rig.attach(bone, cr.ring(f"{bone}.ring", lit, 0.44, 0.04, (0, -0.1, 0), FACING))
        else:
            rig.attach(bone, arrow(f"{bone}.arrow", lit, (dx * 0.44, dz * 0.44), (dx, dz), 0.1))
    return rig


def pose(rig, name, _i, _count):
    if name not in PIECES:
        raise ValueError(name)
    for piece in PIECES:
        if piece != name:
            rig.move(piece, x=AWAY)
    # Face buttons turned a little, so they read as solid.
    if name.startswith("face"):
        rig.turn(name, 12, axis=(1, 0, 0))


cr.run("prompts", build, pose, FRAME, ORIGIN, ANIMATIONS, folder="ui", scale=SCALE, normals=False)
