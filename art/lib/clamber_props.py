"""Shared pieces for the levels' props: scenery drawn in props layers,
which nothing touches.

Each prop is a model built from code about the world's origin, with
z = 0 where it meets what it grows from or stands on: the ground for a
fern, the ceiling for a stalactite, the lip of a ledge for the grass that
overhangs it. `render_set` renders every prop of a set from the side, lit
by the same distant suns as the terrain tiles, onto a transparent
background, framed to the model with the origin across the middle:

- `props/<set>/<name>.png`, in the model's own colours;
- `props/<set>/<name>-normals.png`, its normal map, as for sprites;
- `props/<set>.json`, for the level tools: each prop's size in world
  units and `ground`, how far above the image's bottom edge z = 0 is.
  A props layer places an image by the middle of its bottom edge, so a
  prop meets the line it's placed on when its `at` is `ground` below it.

Props are drawn at `clamber_render.SCALE` texels per world unit, the
level layers' scale.
"""

import json
import math
import os
import random
import tempfile

import bmesh
import bpy
from mathutils import Vector

import clamber_render as cr

# Leaves, blades and moss.
LEAF = "#3f8f45"
LEAF_LIGHT = "#5bb35b"
LEAF_DARK = "#2a6a3a"
GRASS = "#5bb35b"
BLADE = "#7cc46e"
SOIL = "#8a5a3b"
ROOT = "#6e4630"
BARK = "#6b4a33"
MOSS = "#5e9c48"
STONE = "#8d8a9b"


def tube(name, mat, points, radii, sides=10, location=(0, 0, 0), subdivide=1):
    """A round stem along `points`, `(x, y, z)` in Blender units, `radii`
    thick at each (or one radius for all), closed to a point at either end
    whose radius is 0."""
    points = [Vector(p) for p in points]
    if not isinstance(radii, (list, tuple)):
        radii = [radii] * len(points)

    def build(bm):
        rings = []
        for i, (p, r) in enumerate(zip(points, radii)):
            ahead = points[min(i + 1, len(points) - 1)] - points[max(i - 1, 0)]
            ahead.normalize()
            side = ahead.cross(Vector((0, 1, 0)))
            if side.length < 1e-4:
                side = ahead.cross(Vector((1, 0, 0)))
            side.normalize()
            up = side.cross(ahead)
            if r == 0:
                rings.append([bm.verts.new(p)])
                continue
            rings.append([
                bm.verts.new(p + r * (math.cos(a) * side + math.sin(a) * up))
                for a in (2 * math.pi * k / sides for k in range(sides))
            ])
        for low, high in zip(rings, rings[1:]):
            for k in range(sides):
                j = (k + 1) % sides
                if len(low) == 1:
                    bm.faces.new((low[0], high[k], high[j]))
                elif len(high) == 1:
                    bm.faces.new((low[k], low[j], high[0]))
                else:
                    bm.faces.new((low[k], low[j], high[j], high[k]))
        for end in (rings[0], rings[-1]):
            if len(end) > 1:
                bm.faces.new(end)
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)

    obj = cr._mesh_object(name, build, mat, subdivide=subdivide)
    obj.location = location
    return obj


def curve(start, end, sag=0.0, bend=0.0, steps=8, wobble=0.0, seed=0):
    """Points from `start` to `end`, sagging `sag` downwards in the middle
    and bowing `bend` to the side as the camera sees it, wavering by
    `wobble`."""
    rng = random.Random(seed)
    start, end = Vector(start), Vector(end)
    across = end - start
    side = Vector((-across.z, 0, across.x))
    if side.length:
        side.normalize()
    out = []
    for i in range(steps + 1):
        t = i / steps
        bow = 4 * t * (1 - t)
        p = start + across * t - Vector((0, 0, sag * bow)) + side * bend * bow
        if 0 < i < steps:
            p += Vector((rng.uniform(-wobble, wobble), 0, rng.uniform(-wobble, wobble)))
        out.append(p)
    return out


def leaf(name, mat, base, angle, length, width, tilt=0.0, y=0.0, droop=0.0):
    """A pointed leaf from `base`, `(x, z)`, pointing `angle` degrees
    anticlockwise from straight up as the camera sees it, turned `tilt`
    degrees about its length to catch the light differently, and its tip
    curving `droop` (a fraction of its length) downwards."""
    a = math.radians(angle)
    # Built pointing up along z, then turned into place: towards -x is
    # downwards once it points left, towards +x once it points right.
    curl = -math.copysign(droop * length, math.sin(a)) if abs(math.sin(a)) > 1e-3 else 0.0
    steps = 8
    right, left = [], []
    for i in range(1, steps):
        t = i / steps
        half = width / 2 * math.sin(math.pi * t) ** 0.8
        mid = curl * t * t
        right.append((mid + half, t * length))
        left.append((mid - half, t * length))
    outline = [(0.0, 0.0)] + right + [(curl, length)] + list(reversed(left))
    obj = cr.prism(name, mat, outline, 0.01, bevel=0)
    obj.rotation_mode = "ZXY"
    obj.location = (base[0], y, base[1])
    obj.rotation_euler = (0, -a, math.radians(tilt))
    return obj


def blade(name, mat, base, height, lean, width=0.035, y=0.0, turn=0.0):
    """A blade of grass from `base`, `(x, z)`, `height` tall, its tip
    leaning `lean` to the side."""
    outline = [(-width / 2, 0), (width / 2, 0), (lean * 0.55 + width * 0.2, height * 0.6),
               (lean, height), (lean * 0.55 - width * 0.3, height * 0.55)]
    obj = cr.prism(name, mat, outline, 0.008, bevel=0)
    obj.location = (base[0], y, base[1])
    obj.rotation_euler = (0, 0, math.radians(turn))
    return obj


def tuft(name, mat, x, z, rng, count=9, height=0.35, spread=0.18, y=0.0, mats=None):
    """A tuft of blades fanning out from `(x, z)`."""
    for i in range(count):
        t = (i / max(count - 1, 1)) - 0.5
        h = height * rng.uniform(0.55, 1.0) * (1 - abs(t) * 0.6)
        m = mats[i % len(mats)] if mats else mat
        blade(name, m, (x + t * spread * 0.5, z - 0.02), h, t * spread * 1.6 + rng.uniform(-0.04, 0.04),
              y=y + rng.uniform(-0.12, 0.12), turn=rng.uniform(-35, 35))


def lumpy(name, mat, center, radius, rng, strength=0.25, scale=0.6, subdivide=2):
    """A rounded lump, `radius` `(x, y, z)`, made uneven by noise: rocks,
    soil, moss and masses of leaves."""
    obj = cr.ball(name, mat, radius, center)
    obj.modifiers["smooth"].levels = obj.modifiers["smooth"].render_levels = subdivide
    texture = bpy.data.textures.new(f"{name} noise", "CLOUDS")
    texture.noise_scale = scale
    texture.noise_depth = 2
    displace = obj.modifiers.new("uneven", "DISPLACE")
    displace.texture = texture
    displace.strength = strength
    displace.mid_level = 0.5
    # A different patch of the noise for each lump.
    displace.texture_coords = "OBJECT"
    anchor = bpy.data.objects.new(f"{name} anchor", None)
    anchor.location = (rng.uniform(-50, 50), rng.uniform(-50, 50), rng.uniform(-50, 50))
    bpy.context.scene.collection.objects.link(anchor)
    displace.texture_coords_object = anchor
    return obj


def leafy(name, mat, center, radius, rng, scale=0.18, strength=0.35):
    """A mass of leaves: a lump made bumpy at the size of a leaf."""
    obj = lumpy(name, mat, center, radius, rng, strength=strength, scale=scale)
    voronoi = bpy.data.textures.new(f"{name} leaves", "VORONOI")
    voronoi.noise_scale = scale
    voronoi.distance_metric = "DISTANCE"
    obj.modifiers["uneven"].texture = voronoi
    return obj


def flower(name, petal, heart, x, z, size=0.06, y=-0.1, petals=5, seed=0):
    """A small flower facing the camera, on a short stalk."""
    rng = random.Random(seed)
    turn = rng.uniform(0, 2 * math.pi)
    for k in range(petals):
        a = turn + 2 * math.pi * k / petals
        cr.ball(name, petal, (size * 0.75, size * 0.25, size * 0.45),
                (x + math.cos(a) * size * 0.8, y, z + math.sin(a) * size * 0.8),
                (0, -a, 0))
    cr.ball(name, heart, (size * 0.45, size * 0.4, size * 0.45), (x, y - 0.02, z))


def frond(name, rachis, leaflet, base, angle, length, droop, rng, y=0.0, pairs=9,
          leaflet_length=0.3, width=0.07, tilt=0.0):
    """A fern or palm frond: a stem from `base`, `(x, z)`, setting off
    `angle` degrees from straight up and curling `droop` downwards, with
    leaflets in pairs along it, longest in the middle."""
    a = math.radians(angle)
    points = []
    for i in range(pairs + 2):
        t = i / (pairs + 1)
        bend = a + math.radians(droop) * t * t * (1 if angle >= 0 else -1)
        if not points:
            points.append(Vector((base[0], y, base[1])))
            continue
        step = length / (pairs + 1)
        points.append(points[-1] + Vector((-math.sin(bend) * step, 0, math.cos(bend) * step)))
    tube(name, rachis, points, [0.025 * (1 - i / len(points)) + 0.006 for i in range(len(points))],
         sides=6, subdivide=0)
    for i in range(1, pairs + 1):
        p = points[i]
        t = i / (pairs + 1)
        direction = points[i + 1] - points[i - 1]
        heading = math.degrees(math.atan2(-direction.x, direction.z))
        size = leaflet_length * (0.45 + 0.75 * math.sin(math.pi * (t * 0.85 + 0.1)))
        for side in (-1, 1):
            leaf(name, leaflet, (p.x, p.z), heading + side * rng.uniform(55, 75), size,
                 width * (0.7 + 0.5 * size / leaflet_length), tilt=tilt + side * rng.uniform(15, 35),
                 y=p.y + side * 0.01, droop=0.25)


def _bounds():
    """The box around every mesh in the scene as the camera sees it, in
    Blender units: `(x0, z0, x1, z1)`."""
    depsgraph = bpy.context.evaluated_depsgraph_get()
    xs, zs = [], []
    for obj in bpy.context.scene.objects:
        if obj.type != "MESH":
            continue
        evaluated = obj.evaluated_get(depsgraph)
        mesh = evaluated.to_mesh()
        for v in mesh.vertices:
            p = evaluated.matrix_world @ v.co
            xs.append(p.x)
            zs.append(p.z)
        evaluated.to_mesh_clear()
    return min(xs), min(zs), max(xs), max(zs)


def render_set(name, props, samples=128):
    """Renders the set `name` from the command line `tools/render-art.sh`
    gives: `props` lists `(prop name, build)`, `build(rng)` making the
    model in an empty scene lit by the suns."""
    content, quick = cr.args()
    folder = os.path.join(content, "props", name)
    os.makedirs(folder, exist_ok=True)
    tmp = tempfile.mkdtemp(prefix="clamber-props-")
    listing = {}
    for prop, build in props:
        scene = cr.reset()
        cr.suns(scene)
        build(random.Random(prop))
        x0, z0, x1, z1 = _bounds()
        # A world unit to spare all round, the origin across the middle.
        half = math.ceil(max(-x0, x1) / cr.UNIT) + 1
        top = math.ceil(z1 / cr.UNIT) + 1
        bottom = math.floor(z0 / cr.UNIT) - 1
        frame = (2 * half, top - bottom)
        cr.camera(scene, frame, (half, top))
        view = scene.view_settings
        scene.cycles.samples = 16 if quick else samples
        view.view_transform = "Standard"
        image = cr.render(scene, os.path.join(tmp, f"{prop}.png"))
        scene.view_layers[0].material_override = cr.normal_material()
        scene.cycles.samples = 16
        view.view_transform = "Raw"
        normals = cr.render(scene, os.path.join(tmp, f"{prop}-normals.png"))
        path = os.path.join(folder, f"{prop}.png")
        cr.write_png(path, image)
        cr.write_png(os.path.join(folder, f"{prop}-normals.png"), normals)
        listing[prop] = {"size": list(frame), "ground": -bottom}
        print(f"wrote {path}: {image.w}x{image.h}")
    out = os.path.join(content, "props", f"{name}.json")
    with open(out, "w") as f:
        json.dump({"scale": cr.SCALE, "props": listing}, f, indent=2, sort_keys=True)
        f.write("\n")
    print(f"wrote {out}: {len(listing)} props")
