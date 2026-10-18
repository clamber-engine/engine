"""The title screen's rendered art: the logo, as `ui/logo.json`, and a
scene of the climber waving from a mossy rock, as `ui/title.json`.

- `logo`: "CLAMBER" carved in honey-coloured wood, each letter a block
  set at its own slight tilt on a darker backing that outlines it, with
  moss over the tops and a few leaves. Tipped back a little, so the moss
  on the letters' tops shows.
- `rock` and `wave`: the climber, from `art/climber.py`, facing left
  from a mossy boulder with a fern beside it, waving. The boulder is
  rendered once, and the climber's frames drawn over it, both with the
  same origin.

Rendered at high resolution and unlit, so they stay sharp at 4K. Run by
`tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/title.py -- <content folder> [--quick]
"""

import math
import os
import sys

import bpy

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(here, "lib"))
sys.path.insert(0, here)

import clamber_render as cr  # noqa: E402
import climber  # noqa: E402

# The UI's font, carved.
FONT = os.path.join(here, "..", "content", "base", "fonts", "DejaVuSansCondensed-Bold.ttf")
WORD = "CLAMBER"

# The logo's frame and origin (its middle), in world units, and its
# texels per world unit; the game draws it about 190 view units wide.
LOGO_FRAME = (220, 64)
LOGO_ORIGIN = (110, 32)
LOGO_SCALE = 10

# The scene's frame and origin (between the climber's feet, on top of
# the boulder), in world units, and its texels per world unit: the game
# draws it half as big again as the climber in play.
SCENE_FRAME = (64, 66)
SCENE_ORIGIN = (32, 42)
SCENE_SCALE = 15
WAVE = 8

# Each letter's tilt, in degrees counterclockwise, and how far it's
# raised, in Blender units, so the word looks hand-carved.
TILTS = [-5, 3, -2, 4, -3, 2, -4]
RAISES = [0.0, 0.1, -0.06, 0.08, -0.04, 0.1, -0.02]
# Letter size and the gap between letters, in Blender units.
SIZE = 5.0
GAP = 0.3
# How far the letters are tipped back, so their tops show.
TIP = 18


def sunlit():
    """Swaps the scene's lamps for suns, which light a wide model evenly."""
    for obj in list(bpy.data.objects):
        if obj.type == "LIGHT":
            bpy.data.objects.remove(obj)
    cr.suns(bpy.context.scene)


def carved(name, char, mat, extrude, offset=0.0):
    """`char` as a mesh `extrude` deep each side, centred on the origin
    and facing the camera, its outline grown by `offset`."""
    curve = bpy.data.curves.new(name, "FONT")
    curve.body = char
    curve.font = bpy.data.fonts.load(FONT, check_existing=True)
    curve.size = SIZE
    curve.align_x = "CENTER"
    curve.extrude = extrude
    curve.offset = offset
    curve.bevel_depth = 0.07
    curve.bevel_resolution = 3
    obj = bpy.data.objects.new(name, curve)
    bpy.context.scene.collection.objects.link(obj)
    depsgraph = bpy.context.evaluated_depsgraph_get()
    mesh = bpy.data.meshes.new_from_object(obj.evaluated_get(depsgraph))
    bpy.data.objects.remove(obj)
    xs = [v.co.x for v in mesh.vertices]
    ys = [v.co.y for v in mesh.vertices]
    middle = ((min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2)
    for v in mesh.vertices:
        v.co.x -= middle[0]
        v.co.y -= middle[1]
    for poly in mesh.polygons:
        poly.use_smooth = True
    mesh.materials.append(mat)
    letter = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(letter)
    return letter, max(xs) - min(xs), max(ys) - min(ys)


def build_logo():
    sunlit()
    rig = cr.Rig("logo", [("logo", (0, 0, 0), (0, 0, 1), None)])
    wood = cr.textured("carved", "#a8682c", "#e0a860", "wave", 0.6, "Z", bump=0.2, roughness=0.6)
    cr.mossy(wood, amount=0.3)
    backing = cr.textured("backing", "#2a180c", "#4a2c16", "wave", 0.6, "Z", bump=0.1)
    leaves = cr.material("leaves", "#4c9a3a", roughness=0.6, sheen=0.3)

    letters = [carved(f"letter {i}", c, wood, 0.32) for i, c in enumerate(WORD)]
    width = sum(w for _l, w, _h in letters) + GAP * (len(letters) - 1)
    x = -width / 2
    for i, (letter, w, _h) in enumerate(letters):
        back, _w, _h = carved(f"back {i}", WORD[i], backing, 0.2, offset=0.16)
        centre = (x + w / 2, 0, RAISES[i])
        x += w + GAP
        tilt = (math.radians(90 + TIP), math.radians(-TILTS[i]), 0)
        letter.location, letter.rotation_euler = centre, tilt
        back.location = (centre[0], 0.32, centre[2])
        back.rotation_euler = tilt
        rig.attach("logo", letter, back)

    # Leaves sprouting from behind a few letters' tops.
    for i, (dx, angles) in {0: (0.6, (60, 100)), 3: (-0.4, (75, 120)), 6: (0.5, (50,))}.items():
        letter = letters[i][0]
        top = letters[i][2] / 2 * math.cos(math.radians(TIP))
        for k, angle in enumerate(angles):
            a = angle + TILTS[i]
            base = (letter.location.x + dx, 0.1, letter.location.z + top - 0.25)
            rig.attach("logo", leaf(f"leaf {i} {k}", leaves, 1.1, base, a))
    return rig


def leaf(name, mat, length, location, angle, width=0.3):
    """A leaf `length` long from its stem at `location`, pointing `angle`
    degrees counterclockwise from the right, as the camera sees it."""
    outline = []
    for i in range(16):
        t = i / 15
        outline.append((t * length, width * length * math.sin(math.pi * t) ** 0.8))
    for i in range(14, 0, -1):
        t = i / 15
        outline.append((t * length, -width * length * math.sin(math.pi * t) ** 0.8))
    return cr.prism(name, mat, outline, 0.05, location, (0.3, math.radians(-angle), 0), bevel=0)


def pose_logo(rig, name, i, count):
    if name != "logo":
        raise ValueError(name)


def build_scene():
    rig = climber.build()
    # Facing left, towards the menu, still showing its front.
    rig.obj.rotation_euler = (0, 0, math.radians(216))

    stone = cr.textured("stone", "#5a5e66", "#8a8e96", "noise", 2.5, bump=0.4, roughness=0.9)
    cr.mossy(stone, amount=0.6)
    boulder = cr.ball("boulder", stone, (2.3, 1.4, 1.2), (0.1, 0.4, -0.95))
    lumps = boulder.modifiers.new("lumps", "DISPLACE")
    lumps.texture = bpy.data.textures.new("lumps", "CLOUDS")
    lumps.texture.noise_scale = 0.9
    lumps.strength = 0.35
    lumps.texture_coords = "OBJECT"
    rock = [boulder, cr.ball("pebble", stone, (0.7, 0.6, 0.5), (1.9, -0.4, -1.75))]
    fern = cr.material("fern", "#3f8a34", roughness=0.6, sheen=0.3)
    for k, (angle, length) in enumerate([(60, 1.6), (85, 1.9), (110, 1.7), (135, 1.3), (40, 1.2)]):
        rock.append(leaf(f"frond {k}", fern, length, (-1.9, -0.6, -1.7), angle, width=0.16))
    # The rock and the climber are rendered apart, each shown alone, so
    # the still rock is in the sheet once.
    for part in rock + rig.parts:
        rig.track(part, "hide_render")
    rig.rock = rock
    return rig


def pose_scene(rig, name, i, count):
    if name == "rock":
        for part in rig.parts:
            part.hide_render = True
        return
    if name != "wave":
        raise ValueError(name)
    for part in rig.rock:
        part.hide_render = True
    p = 2 * math.pi * i / count
    breath = math.sin(p)
    rig.squash("hips", 0.015 * breath)
    rig.turn("chest", 2 + 1.5 * breath)
    rig.turn("head", 6 + 3 * breath)
    # The far arm, which faces the camera as the climber faces left,
    # raised and waving beside the head; the other at rest.
    rig.turn("arm.far", 25)
    rig.turn("arm.far", 160 + 30 * math.sin(p), axis=(1, 0, 0))
    rig.turn("arm.near", -6)
    climber.legs(rig, 4, -6)


cr.run("logo", build_logo, pose_logo, LOGO_FRAME, LOGO_ORIGIN, [("logo", 1, 1, None)],
       folder="ui", scale=LOGO_SCALE, normals=False)
cr.run("title", build_scene, pose_scene, SCENE_FRAME, SCENE_ORIGIN, [("rock", 1, 1, None), ("wave", WAVE, 4, None)],
       folder="ui", scale=SCENE_SCALE, normals=False)
