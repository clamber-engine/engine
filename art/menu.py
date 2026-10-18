"""The menus' rendered pieces, as `ui/menu.json`: a panel, a highlight and
a selection cursor.

- `panel`: a slab of dark slate in a frame of wooden beams, with moss and
  leaves over the corners. The game draws it as nine pieces, keeping the
  corners as they are and stretching the rest, so the moss stays at the
  corners and nothing else in it minds being stretched: the slate's
  mottling is soft and the beams' grain runs along them.
- `highlight`: a plank of light wood with rounded ends, behind the
  selected item. The game stretches its middle to the item's width.
- `cursor`: the gem, turning, beside the selected item.

Rendered at high resolution and unlit, so they stay sharp at 4K. Run by
`tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/menu.py -- <content folder> [--quick]
"""

import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import clamber_render as cr  # noqa: E402

# Frame size and the origin (the middle), in world units: the panel fills
# it, edge to edge.
FRAME = (48, 32)
ORIGIN = (24, 16)
# Texels per world unit.
SCALE = 10

# Frames of the cursor's turn: half a turn, as the gem looks the same
# from behind.
TURN = 16
ANIMATIONS = [("panel", 1, 1, None), ("highlight", 1, 1, None), ("cursor", TURN, 3, None)]
PIECES = ["panel", "highlight", "cursor"]

# How far the pieces not being drawn are moved away, in world units.
AWAY = 1000

# The beams' thickness, in Blender units (3.4 world units); the game's
# `BORDER` in `crates/game/src/panel.rs` keeps them and the corners whole.
BEAM = 0.34
WOOD = "#8a5a32"


def textured(name, dark, light, kind, scale, axis="X", bump=0.3, roughness=0.8):
    """A material mottled between two `#rrggbb` colours by a noise texture,
    or grained along `axis` by a wave texture, with a little relief."""
    mat = cr.material(name, dark, roughness=roughness)
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    shader = nodes["Principled BSDF"]
    coords = nodes.new("ShaderNodeTexCoord")
    if kind == "wave":
        tex = nodes.new("ShaderNodeTexWave")
        tex.bands_direction = axis
        tex.inputs["Distortion"].default_value = 6.0
        tex.inputs["Detail"].default_value = 3.0
    else:
        tex = nodes.new("ShaderNodeTexNoise")
        tex.inputs["Detail"].default_value = 6.0
    tex.inputs["Scale"].default_value = scale
    links.new(coords.outputs["Object"], tex.inputs["Vector"])
    ramp = nodes.new("ShaderNodeValToRGB")
    ramp.color_ramp.elements[0].color = (*cr.linear(dark), 1.0)
    ramp.color_ramp.elements[1].color = (*cr.linear(light), 1.0)
    links.new(tex.outputs["Fac"], ramp.inputs["Fac"])
    links.new(ramp.outputs["Color"], shader.inputs["Base Color"])
    relief = nodes.new("ShaderNodeBump")
    relief.inputs["Strength"].default_value = bump
    links.new(tex.outputs["Fac"], relief.inputs["Height"])
    links.new(relief.outputs["Normal"], shader.inputs["Normal"])
    return mat


def leaf(name, mat, length, location, angle):
    """A leaf `length` long from its stem at `location`, pointing `angle`
    degrees counterclockwise from the right, as the camera sees it."""
    outline = []
    for i in range(16):
        t = i / 15
        outline.append((t * length, 0.32 * length * math.sin(math.pi * t) ** 0.8))
    for i in range(14, 0, -1):
        t = i / 15
        outline.append((t * length, -0.32 * length * math.sin(math.pi * t) ** 0.8))
    # Tipped a little towards the light, so neighbours don't shade alike.
    return cr.prism(name, mat, outline, 0.05, location, (0.3, math.radians(-angle), 0), bevel=0)


def stadium(w, h):
    """A `w` by `h` outline with round ends."""
    r = h / 2
    outline = []
    for end, middle in ((1, w / 2 - r), (-1, -(w / 2 - r))):
        for i in range(17):
            a = -math.pi / 2 + math.pi * i / 16
            outline.append((middle + end * r * math.cos(a), end * r * math.sin(a)))
    return outline


def corner(rig, sx, sz, moss, leaves):
    """Moss and leaves over the panel's corner at `(sx, sz)`, each ±1."""
    x, z = sx * (2.4 - BEAM / 2), sz * (1.6 - BEAM / 2)
    parts = [
        cr.ball(f"moss {sx} {sz}", moss, (0.34, 0.14, 0.26), (x - sx * 0.02, -0.2, z)),
        cr.ball(f"moss2 {sx} {sz}", moss, (0.2, 0.12, 0.15),
                (x - sx * 0.34, -0.22, z - sz * 0.02)),
        cr.ball(f"moss3 {sx} {sz}", moss, (0.15, 0.1, 0.18),
                (x, -0.22, z - sz * 0.3)),
    ]
    # Pointing in along each beam and in towards the middle, never out
    # past the frame's edge.
    inward = math.degrees(math.atan2(-sz, -sx))
    for k, (turn, length, along) in enumerate([(-38, 0.36, 0.12), (0, 0.3, 0.16), (38, 0.36, 0.12)]):
        a = math.radians(inward + turn)
        start = (x + along * math.cos(a), -0.36, z + along * math.sin(a))
        parts.append(leaf(f"leaf {sx} {sz} {k}", leaves, length, start, inward + turn))
    rig.attach("panel", *parts)


def build():
    rig = cr.Rig("menu", [(piece, (0, 0, -0.9), (0, 0, 0.9), None) for piece in PIECES])

    slate = textured("slate", "#1c2228", "#323a42", "noise", 3.0, bump=0.15, roughness=0.9)
    rig.attach("panel", cr.box("slab", slate, (4.6, 0.2, 3.0), (0, 0.12, 0), bevel=0.2))
    across = textured("beam across", "#5e3c22", WOOD, "wave", 1.4, "Z")
    down = textured("beam down", "#5e3c22", WOOD, "wave", 1.4, "X")
    for sz in (-1, 1):
        rig.attach("panel", cr.box(f"beam {sz}", across, (4.8, 0.36, BEAM),
                                   (0, 0, sz * (1.6 - BEAM / 2)), bevel=0.7))
    for sx in (-1, 1):
        rig.attach("panel", cr.box(f"post {sx}", down, (BEAM, 0.36, 3.2 - 2 * BEAM),
                                   (sx * (2.4 - BEAM / 2), 0.02, 0), bevel=0.7))
    moss = textured("moss", "#2e5a26", "#6aa040", "noise", 12.0, bump=0.6, roughness=0.95)
    leaves = cr.material("leaves", "#4c9a3a", roughness=0.6, sheen=0.3)
    for sx in (-1, 1):
        for sz in (-1, 1):
            corner(rig, sx, sz, moss, leaves)

    plank = textured("plank", "#7a4e28", "#b07c48", "wave", 1.0, "Z", bump=0.15, roughness=0.6)
    rig.attach("highlight", cr.prism("plank", plank, stadium(3.6, 0.7), 0.16, bevel=0.06))

    stone = cr.glow("stone", "#3ee0b0", 0.35)[0]
    shader = stone.node_tree.nodes["Principled BSDF"]
    shader.inputs["Roughness"].default_value = 0.08
    shader.inputs["Coat Weight"].default_value = 1.0
    jewel = cr.lathe(
        "jewel", stone,
        [(0, -0.8), (0.9, 0.06), (0.76, 0.42), (0.46, 0.56), (0, 0.56)],
        segments=8, smooth=False,
    )
    jewel.location = (0, 0, 0.05)
    jewel.scale = (0.4, 0.18, 0.4)
    rig.attach("cursor", jewel)
    return rig


def pose(rig, name, i, count):
    if name not in PIECES:
        raise ValueError(name)
    for piece in PIECES:
        if piece != name:
            rig.move(piece, x=AWAY)
    if name == "cursor":
        rig.turn(name, 180 * i / count, axis=(0, 0, 1))


cr.run("menu", build, pose, FRAME, ORIGIN, ANIMATIONS, folder="ui", scale=SCALE, normals=False)
