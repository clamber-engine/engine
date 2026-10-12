"""Shared pieces for Clamber's pre-rendered art, run inside Blender.

Every model is built from code: primitives with modifiers, parented to the
bones of a simple armature and posed with keyframes. `render_sheet` then
renders each animation frame from an orthographic side camera, under the
same three-point lighting, onto a transparent background:

- once in the model's own colours, and once per named variant (other
  colours for the same model, such as the second roster slot's);
- once as a normal map: camera-space normals, x right, y up and z towards
  the camera, each stored as `0.5 + 0.5 * n` in red, green and blue.
  The game doesn't read normal maps yet, so the metadata doesn't name it.

Frames are trimmed to what they show and packed into one image per pass,
all laid out alike, with the `clamber-sprite` JSON beside them: frame
tags per animation, the origin, the texel scale, the variants, and a
`hurtbox` slice per frame around the posed model.

Distances: one Blender unit is ten world units, so a body 32 units tall is
3.2 m. Sheets are drawn at `SCALE` texels per world unit.
"""

import json
import math
import os
import struct
import tempfile
import zlib
from array import array

import addon_utils
import bmesh
import bpy
from bpy_extras.object_utils import world_to_camera_view
from mathutils import Matrix, Vector

# Blender units per world unit.
UNIT = 0.1
# Texels per world unit: the game's 384x216 view at 1920x1080.
SCALE = 5
# The widest a packed sheet gets, in texels.
SHEET_WIDTH = 2048
# Empty texels around each packed frame, so filtering doesn't bleed.
GAP = 2


def reset():
    """An empty scene set up for rendering sprites with Cycles."""
    bpy.ops.wm.read_factory_settings(use_empty=True)
    addon_utils.enable("cycles", default_set=True)
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.use_denoising = False
    scene.cycles.seed = 0
    scene.render.film_transparent = True
    scene.render.use_motion_blur = False
    scene.render.resolution_percentage = 100
    settings = scene.render.image_settings
    settings.file_format = "PNG"
    settings.color_mode = "RGBA"
    settings.color_depth = "8"
    world = bpy.data.worlds.new("sky")
    world.use_nodes = True
    background = world.node_tree.nodes["Background"]
    background.inputs["Color"].default_value = (*linear("#b8c8e0"), 1.0)
    background.inputs["Strength"].default_value = 0.35
    scene.world = world
    return scene


def linear(hex_color):
    """An `#rrggbb` colour as linear RGB."""
    def channel(c):
        c = int(c, 16) / 255
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4

    h = hex_color.lstrip("#")
    return tuple(channel(h[i : i + 2]) for i in (0, 2, 4))


def lights(scene):
    """Three-point lighting from the camera's side: a warm key above and
    in front, a cool fill low on the other side, and a rim from behind.
    Large area lights keep the shadows soft."""
    def area(name, location, energy, size, color):
        data = bpy.data.lights.new(name, "AREA")
        data.energy = energy
        data.size = size
        data.color = linear(color)
        light = bpy.data.objects.new(name, data)
        light.location = location
        aim(light, Vector((0, 0, 1.6)))
        scene.collection.objects.link(light)

    area("key", (-3.0, -7.0, 6.5), 1300, 4.0, "#fff1dc")
    area("fill", (6.0, -6.0, 1.5), 350, 6.0, "#d4e2ff")
    area("rim", (2.5, 6.0, 5.0), 1200, 3.0, "#ffffff")


def aim(obj, target):
    """Points `obj`'s -Z axis (where lights and cameras look) at `target`."""
    direction = target - Vector(obj.location)
    obj.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()


def camera(scene, frame, origin):
    """An orthographic camera looking at the model from the side (from -Y,
    with +X to the right), framing `frame` world units with the world's
    origin at `origin` within it, measured from the frame's top left."""
    fw, fh = frame
    ox, oy = origin
    data = bpy.data.cameras.new("camera")
    data.type = "ORTHO"
    data.ortho_scale = max(fw, fh) * UNIT
    data.clip_start = 0.1
    data.clip_end = 100.0
    cam = bpy.data.objects.new("camera", data)
    cam.location = ((fw / 2 - ox) * UNIT, -30.0, (oy - fh / 2) * UNIT)
    cam.rotation_euler = (math.pi / 2, 0, 0)
    scene.collection.objects.link(cam)
    scene.camera = cam
    scene.render.resolution_x = fw * SCALE
    scene.render.resolution_y = fh * SCALE
    return cam


def material(name, color, roughness=0.55, sheen=0.0):
    """A plain painted material from an `#rrggbb` colour. Its colour can
    be changed for variants through `recolor`."""
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    shader = mat.node_tree.nodes["Principled BSDF"]
    shader.inputs["Base Color"].default_value = (*linear(color), 1.0)
    shader.inputs["Roughness"].default_value = roughness
    shader.inputs["Sheen Weight"].default_value = sheen
    mat["color"] = color
    return mat


def recolor(colors):
    """Sets materials' colours by name, `#rrggbb`; materials not named get
    their own colour back."""
    for mat in bpy.data.materials:
        if "color" not in mat:
            continue
        color = colors.get(mat.name, mat["color"])
        shader = mat.node_tree.nodes["Principled BSDF"]
        shader.inputs["Base Color"].default_value = (*linear(color), 1.0)


def normal_material():
    """Shades every surface with its camera-space normal, encoded. The
    camera looks along +Y with +X to its right, as `camera` sets it up."""
    mat = bpy.data.materials.new("normals")
    mat.use_nodes = True
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    nodes.clear()
    geometry = nodes.new("ShaderNodeNewGeometry")
    world = nodes.new("ShaderNodeSeparateXYZ")
    towards = nodes.new("ShaderNodeMath")
    towards.operation = "MULTIPLY"
    towards.inputs[1].default_value = -1.0
    seen = nodes.new("ShaderNodeCombineXYZ")
    encode = nodes.new("ShaderNodeVectorMath")
    encode.operation = "MULTIPLY_ADD"
    encode.inputs[1].default_value = (0.5, 0.5, 0.5)
    encode.inputs[2].default_value = (0.5, 0.5, 0.5)
    emission = nodes.new("ShaderNodeEmission")
    output = nodes.new("ShaderNodeOutputMaterial")
    links.new(geometry.outputs["Normal"], world.inputs["Vector"])
    links.new(world.outputs["X"], seen.inputs["X"])
    links.new(world.outputs["Z"], seen.inputs["Y"])
    links.new(world.outputs["Y"], towards.inputs[0])
    links.new(towards.outputs["Value"], seen.inputs["Z"])
    links.new(seen.outputs["Vector"], encode.inputs[0])
    links.new(encode.outputs["Vector"], emission.inputs["Color"])
    links.new(emission.outputs["Emission"], output.inputs["Surface"])
    return mat


# Building models.


def _mesh_object(name, build, mat, subdivide=2):
    mesh = bpy.data.meshes.new(name)
    bm = bmesh.new()
    build(bm)
    bm.to_mesh(mesh)
    bm.free()
    for poly in mesh.polygons:
        poly.use_smooth = True
    mesh.materials.append(mat)
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    if subdivide:
        mod = obj.modifiers.new("smooth", "SUBSURF")
        mod.levels = subdivide
        mod.render_levels = subdivide
    return obj


def ball(name, mat, radius=(1, 1, 1), location=(0, 0, 0), rotation=(0, 0, 0)):
    """An ellipsoid with the given radii."""
    def build(bm):
        bmesh.ops.create_uvsphere(bm, u_segments=24, v_segments=16, radius=1.0)

    obj = _mesh_object(name, build, mat, subdivide=1)
    obj.scale = radius
    obj.location = location
    obj.rotation_euler = rotation
    return obj


def box(name, mat, size, location=(0, 0, 0), rotation=(0, 0, 0), bevel=0.3):
    """A box with rounded edges, `size` its full width, depth and height;
    `bevel` is how much of the smallest side rounds off."""
    def build(bm):
        bmesh.ops.create_cube(bm, size=1.0)

    obj = _mesh_object(name, build, mat, subdivide=0)
    obj.scale = size
    mod = obj.modifiers.new("round", "BEVEL")
    mod.width = 0.5 * bevel
    mod.segments = 4
    mod.affect = "EDGES"
    smooth = obj.modifiers.new("smooth", "SUBSURF")
    smooth.levels = smooth.render_levels = 1
    obj.location = location
    obj.rotation_euler = rotation
    return obj


def ring(name, mat, radius, thickness, location=(0, 0, 0), rotation=(0, 0, 0)):
    """A torus around its local Z axis."""
    def build(bm):
        segments, sides = 32, 12
        verts = []
        for i in range(segments):
            a = 2 * math.pi * i / segments
            for j in range(sides):
                b = 2 * math.pi * j / sides
                r = radius + thickness * math.cos(b)
                verts.append(
                    bm.verts.new((r * math.cos(a), r * math.sin(a), thickness * math.sin(b)))
                )
        for i in range(segments):
            for j in range(sides):
                a = verts[i * sides + j]
                b = verts[((i + 1) % segments) * sides + j]
                c = verts[((i + 1) % segments) * sides + (j + 1) % sides]
                d = verts[i * sides + (j + 1) % sides]
                bm.faces.new((a, b, c, d))

    obj = _mesh_object(name, build, mat, subdivide=1)
    obj.location = location
    obj.rotation_euler = rotation
    return obj


def limb(name, mat, start, end, radius):
    """An ellipsoid stretched from `start` to `end`, `radius` thick."""
    start, end = Vector(start), Vector(end)
    length = (end - start).length
    obj = ball(name, mat, radius=(radius, radius, length / 2 + radius * 0.6))
    obj.location = (start + end) / 2
    obj.rotation_euler = (end - start).to_track_quat("Z", "Y").to_euler()
    return obj


class Rig:
    """An armature whose bones carry rigid parts, posed in terms of what
    the camera sees."""

    def __init__(self, name, bones, yaw=0.0):
        """`bones` lists `(name, head, tail, parent)`, parents first.
        `yaw` turns the whole model about the vertical, in degrees, to show
        a little more of its front."""
        data = bpy.data.armatures.new(name)
        self.obj = bpy.data.objects.new(name, data)
        bpy.context.scene.collection.objects.link(self.obj)
        bpy.context.view_layer.objects.active = self.obj
        bpy.ops.object.mode_set(mode="EDIT")
        for bone, head, tail, parent in bones:
            edit = data.edit_bones.new(bone)
            edit.head, edit.tail = head, tail
            if parent:
                edit.parent = data.edit_bones[parent]
        bpy.ops.object.mode_set(mode="OBJECT")
        self.obj.rotation_euler = (0, 0, math.radians(yaw))
        self.parts = []

    def attach(self, bone, *parts):
        """Parents parts, placed in the model's rest pose, to `bone`."""
        bpy.context.view_layer.update()
        for part in parts:
            world = self.obj.matrix_world @ part.matrix_world
            part.parent = self.obj
            part.parent_type = "BONE"
            part.parent_bone = bone
            bpy.context.view_layer.update()
            part.matrix_world = world
            part["rest_scale"] = tuple(part.scale)
            self.parts.append(part)

    def rest(self):
        """Clears the pose, and any part's change of scale."""
        for part in self.parts:
            part.scale = part["rest_scale"]
        for bone in self.obj.pose.bones:
            bone.rotation_mode = "QUATERNION"
            bone.rotation_quaternion = (1, 0, 0, 0)
            bone.location = (0, 0, 0)
            bone.scale = (1, 1, 1)

    def _local(self, bone):
        return self.obj.data.bones[bone].matrix_local.to_3x3()

    def turn(self, bone, degrees, axis=(0, -1, 0)):
        """Turns `bone` (and what hangs from it) relative to its parent.
        About the default axis, positive is counterclockwise as the camera
        sees it, with the model facing right: a leg swings forward, a spine
        leans back."""
        m = self._local(bone)
        r = Matrix.Rotation(math.radians(degrees), 3, Vector(axis))
        q = (m.inverted() @ r @ m).to_quaternion()
        pose = self.obj.pose.bones[bone]
        pose.rotation_quaternion = q @ pose.rotation_quaternion

    def move(self, bone, x=0.0, z=0.0):
        """Moves `bone` by `x` forward and `z` up, in world units."""
        pose = self.obj.pose.bones[bone]
        pose.location = self._local(bone).inverted() @ Vector((x * UNIT, 0, z * UNIT))

    def ground(self, x=0.0):
        """Moves the root so the parts, as the camera sees them, are
        centred `x` world units forward of the origin and rest on it."""
        to_armature = self.obj.matrix_world.to_3x3().inverted()
        to_root = self._local("root").inverted()
        pose = self.obj.pose.bones["root"]
        for _ in range(3):
            depsgraph = bpy.context.evaluated_depsgraph_get()
            points = []
            for part in self.parts:
                evaluated = part.evaluated_get(depsgraph)
                mesh = evaluated.to_mesh()
                points += [evaluated.matrix_world @ v.co for v in mesh.vertices]
                evaluated.to_mesh_clear()
            middle = (min(p.x for p in points) + max(p.x for p in points)) / 2
            bottom = min(p.z for p in points)
            delta = Vector((x * UNIT - middle, 0, -bottom))
            pose.location = pose.location + to_root @ (to_armature @ delta)

    def squash(self, bone, amount):
        """Squashes `bone` (and its children) by `amount` vertically,
        widening to keep its volume. Negative stretches."""
        pose = self.obj.pose.bones[bone]
        tall = 1 - amount
        wide = 1 / math.sqrt(tall)
        pose.scale = (wide, tall, wide)

    def key(self, frame):
        """Keyframes the pose, and any part's scale, on `frame`."""
        for bone in self.obj.pose.bones:
            for path in ("rotation_quaternion", "location", "scale"):
                bone.keyframe_insert(path, frame=frame)
        for part in self.parts:
            part.keyframe_insert("scale", frame=frame)


# Rendering sheets.


def _render(scene, path):
    """Renders the current frame: an `Image`."""
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)
    image = bpy.data.images.load(path)
    w, h = image.size
    pixels = array("f", bytes(4 * w * h * 4))
    image.pixels.foreach_get(pixels)
    bpy.data.images.remove(image)
    data = bytes(min(255, int(v * 255 + 0.5)) for v in pixels)
    # Blender's rows run bottom to top.
    stride = 4 * w
    rows = [data[y * stride : (y + 1) * stride] for y in range(h)]
    return Image(w, h, b"".join(reversed(rows)))


class Image:
    """RGBA texels, rows top to bottom."""

    def __init__(self, w, h, data=None):
        self.w, self.h = w, h
        self.data = bytearray(data if data is not None else bytes(4 * w * h))

    def row(self, y, x=0, w=None):
        w = self.w - x if w is None else w
        start = 4 * (y * self.w + x)
        return self.data[start : start + 4 * w]

    def paste(self, src, rect, at):
        """Copies `rect` of `src`, `(x, y, w, h)`, to `at` in this image."""
        x, y, w, h = rect
        for row in range(h):
            start = 4 * ((at[1] + row) * self.w + at[0])
            self.data[start : start + 4 * w] = src.row(y + row, x, w)


def _bounds(scene, cam, objects):
    """The texel rectangle around `objects` as the camera sees them,
    `(x, y, w, h)` from the frame's top left."""
    depsgraph = bpy.context.evaluated_depsgraph_get()
    w, h = scene.render.resolution_x, scene.render.resolution_y
    xs, ys = [], []
    for obj in objects:
        evaluated = obj.evaluated_get(depsgraph)
        mesh = evaluated.to_mesh()
        for v in mesh.vertices:
            p = world_to_camera_view(scene, cam, evaluated.matrix_world @ v.co)
            xs.append(p.x * w)
            ys.append((1 - p.y) * h)
        evaluated.to_mesh_clear()
    x0, y0 = math.floor(min(xs)), math.floor(min(ys))
    x1, y1 = math.ceil(max(xs)), math.ceil(max(ys))
    return x0, y0, x1 - x0, y1 - y0


def _trim(images):
    """The smallest rectangle holding every visible texel of `images`
    (alike in size), with a texel to spare where there's room."""
    w, h = images[0].w, images[0].h
    cols, rows = set(), []
    for y in range(h):
        seen = False
        for image in images:
            alpha = image.row(y)[3::4]
            if any(alpha):
                seen = True
                cols.update(i for i, a in enumerate(alpha) if a)
        if seen:
            rows.append(y)
    if not rows:
        return 0, 0, 1, 1
    x0, x1 = max(min(cols) - 1, 0), min(max(cols) + 2, w)
    y0, y1 = max(rows[0] - 1, 0), min(rows[-1] + 2, h)
    return x0, y0, x1 - x0, y1 - y0


def _pack(sizes):
    """Places rectangles in rows, in order: their positions and the
    sheet's size."""
    places, x, y, row, width = [], GAP, GAP, 0, 0
    for w, h in sizes:
        if x + w + GAP > SHEET_WIDTH and x > GAP:
            x, y, row = GAP, y + row + GAP, 0
        places.append((x, y))
        x += w + GAP
        row = max(row, h)
        width = max(width, x)
    return places, (width, y + row + GAP)


def write_png(path, image):
    """Writes an `Image` as a PNG."""
    # filter type 2 (up) after the first row, which suits smooth renders
    rows, previous = [], None
    for y in range(image.h):
        row = image.row(y)
        if previous is None:
            rows.append(b"\x00" + row)
        else:
            rows.append(b"\x02" + bytes((a - b) & 255 for a, b in zip(row, previous)))
        previous = row

    def chunk(kind, body):
        crc = zlib.crc32(kind + body) & 0xFFFFFFFF
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", crc)

    header = struct.pack(">IIBBBBB", image.w, image.h, 8, 6, 0, 0, 0)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(chunk(b"IHDR", header))
        f.write(chunk(b"IDAT", zlib.compress(b"".join(rows), 9)))
        f.write(chunk(b"IEND", b""))


def render_sheet(scene, cam, rig, out, frame, origin, animations, pose, variants=None,
                 hurtbox=None, samples=192):
    """Renders and writes a sheet: `out` is the JSON's path, with the PNGs
    beside it (`<name>.png`, `<name>-<variant>.png`, `<name>-normals.png`).

    `frame` and `origin` are in world units, as for `camera`.
    `animations` lists `(name, frame_count, ticks, repeat)`, with `repeat`
    None to loop. `pose(name, i, count)` poses the model for frame `i` of
    an animation of `rig`, starting from its rest pose. `variants` maps variant names to material colours (see
    `recolor`). `hurtbox` lists the objects the `hurtbox` slice surrounds.
    """
    variants = variants or {}
    steps = []
    for name, count, _ticks, _repeat in animations:
        for i in range(count):
            steps.append((name, i, count))
    for n, (name, i, count) in enumerate(steps, start=1):
        rig.rest()
        pose(name, i, count)
        rig.key(n)

    # Keyframes hold until the next, one per rendered frame.
    for action in bpy.data.actions:
        for curve in action.fcurves:
            for point in curve.keyframe_points:
                point.interpolation = "CONSTANT"

    boxes = []
    for n in range(1, len(steps) + 1):
        scene.frame_set(n)
        boxes.append(_bounds(scene, cam, hurtbox) if hurtbox else None)

    passes = {"": {}}
    passes.update({f"-{v}": colors for v, colors in variants.items()})
    renders = {}
    tmp = tempfile.mkdtemp(prefix="clamber-render-")
    view = scene.view_settings
    for suffix, colors in passes.items():
        recolor(colors)
        scene.cycles.samples = samples
        view.view_transform = "Standard"
        renders[suffix] = [
            _render_frame(scene, n, os.path.join(tmp, f"f{suffix}{n}.png"))
            for n in range(1, len(steps) + 1)
        ]
    recolor({})
    scene.view_layers[0].material_override = normal_material()
    scene.cycles.samples = 32
    view.view_transform = "Raw"
    renders["-normals"] = [
        _render_frame(scene, n, os.path.join(tmp, f"n{n}.png"))
        for n in range(1, len(steps) + 1)
    ]
    scene.view_layers[0].material_override = None
    view.view_transform = "Standard"

    trims = [_trim([r[n] for r in renders.values()]) for n in range(len(steps))]
    places, size = _pack([(w, h) for _x, _y, w, h in trims])
    base = os.path.splitext(out)[0]
    stem = os.path.basename(base)
    for suffix, frames in renders.items():
        sheet = Image(*size)
        for trim, place, image in zip(trims, places, frames):
            sheet.paste(image, trim, place)
        write_png(f"{base}{suffix}.png", sheet)

    fw, fh = frame[0] * SCALE, frame[1] * SCALE
    ticks_of = {name: ticks for name, _c, ticks, _r in animations}
    frames = []
    for n, ((name, i, _count), (x, y, w, h), (px, py)) in enumerate(zip(steps, trims, places)):
        ticks = ticks_of[name]
        frames.append({
            "filename": f"{stem} {name} {i}",
            "frame": {"x": px, "y": py, "w": w, "h": h},
            "rotated": False,
            "trimmed": (w, h) != (fw, fh),
            "spriteSourceSize": {"x": x, "y": y, "w": w, "h": h},
            "sourceSize": {"w": fw, "h": fh},
            "duration": round(ticks * 1000 / 60),
            "ticks": ticks,
        })
    tags, first = [], 0
    for name, count, _ticks, repeat in animations:
        tag = {"name": name, "from": first, "to": first + count - 1, "direction": "forward"}
        if repeat:
            tag["repeat"] = str(repeat)
        tags.append(tag)
        first += count
    meta = {
        "app": "clamber render-art",
        "image": f"{stem}.png",
        "format": "RGBA8888",
        "size": {"w": size[0], "h": size[1]},
        "scale": "1",
        "frameTags": tags,
    }
    if hurtbox:
        meta["slices"] = [{
            "name": "hurtbox",
            "keys": [
                {"frame": n, "bounds": {"x": x, "y": y, "w": w, "h": h}}
                for n, (x, y, w, h) in enumerate(boxes)
            ],
        }]
    meta["clamber"] = {
        "origin": {"x": origin[0] * SCALE, "y": origin[1] * SCALE},
        "scale": SCALE,
    }
    if variants:
        meta["clamber"]["variants"] = {v: f"{stem}-{v}.png" for v in variants}
    with open(out, "w") as f:
        json.dump({"frames": frames, "meta": meta}, f, indent=2)
        f.write("\n")
    print(f"wrote {out}: {len(steps)} frames, {size[0]}x{size[1]}")


def _render_frame(scene, n, path):
    scene.frame_set(n)
    return _render(scene, path)
