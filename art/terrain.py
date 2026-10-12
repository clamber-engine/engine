"""The terrain tiles, as `tiles/terrain.png` and `tiles/terrain-normals.png`.

Sixteen 16x16 tiles, eight to a row, numbered from 0 (Tiled's numbers, in
level data, are one more):

     0 grass, left end    1 grass                2 grass, right end
     3 earth              4 earth with stones     5-7 plank: left, middle,
                                                  right
     8 slope up (1:1)     9 slope down (1:1)
    10-11 slope up (1:2): low half, high half
    12-13 slope down (1:2): high half, low half
    14 spikes            15 stone block

Each tile is one 16x16 window onto a model built from code. The models
repeat every tile, across and down (the soil's relief, its pebbles, the
grass blades, the nails), and are lit by distant suns, so tiles meet
without seams. Slopes are cut from the same soil as the earth beneath
them. Tiles are drawn at `SCALE` texels per world unit, with `PAD` texels
around each repeating its edges, so smooth filtering doesn't bleed one
tile into the next. Run by `tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/terrain.py -- <content folder> [--quick]
"""

import math
import os
import random
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import bmesh  # noqa: E402
import bpy  # noqa: E402

import clamber_render as cr  # noqa: E402

# A tile, in world units, and in Blender units.
TILE = 16
T = TILE * cr.UNIT
# Tiles across the sheet.
COLUMNS = 8
# Texels around each tile repeating its edges: the level's `margin`, and
# half its `spacing`.
PAD = 4
# Where tiles are built, along x, so they don't shade each other.
APART = 8.0

SOIL = "#8a5a3b"
PEBBLES = ["#a07a5c", "#6e4630", "#b58f6e", "#7d5338"]
GRASS = "#5bb35b"
BLADE = "#7cc46e"
FRINGE = "#3f8f45"
WOOD = "#c08a52"
STEEL = "#c9c3b5"
STONE = "#8d8a9b"
MORTAR = "#3b3746"

# The soil's relief, as waves that repeat every tile both ways:
# (across, down, depth in Blender units, phase).
_rng = random.Random(4)
RELIEF = [
    (m, n, 0.012 / math.hypot(m, n), _rng.uniform(0, 2 * math.pi))
    for m, n in [(1, 2), (2, -1), (3, 1), (-2, 3), (4, 3), (5, -2), (-3, 5), (6, 1), (2, 7)]
]
# Pebbles in the soil, one tile's worth: (x, z, radius), x from the tile's
# middle, z up from its bottom, in Blender units.
PEBBLE_SPOTS = [
    (-0.52, 1.12, 0.07), (0.18, 1.38, 0.05), (0.55, 0.92, 0.08), (-0.12, 0.66, 0.06),
    (-0.6, 0.28, 0.05), (0.32, 0.22, 0.07), (0.7, 0.5, 0.04), (-0.3, 1.5, 0.04),
]


def relief(x, z):
    return sum(
        a * math.sin(2 * math.pi * (m * x + n * z) / T + p) for m, n, a, p in RELIEF
    )


def copies(x, z, reach=0.3):
    """`(x, z)` and its repeats a tile away each way, those near enough
    the tile's window to be seen or cast a shadow into it."""
    for dx in (-T, 0, T):
        for dz in (-T, 0, T):
            if abs(x + dx) < T / 2 + reach and -reach < z + dz < T + reach:
                yield x + dx, z + dz


def cut(bm, point, normal):
    """Removes what's beyond the plane through `point` facing `normal`."""
    geom = bm.verts[:] + bm.edges[:] + bm.faces[:]
    bmesh.ops.bisect_plane(bm, geom=geom, plane_co=point, plane_no=normal, clear_outer=True)


class Tile:
    """A tile's model, built about `x0` along x: its window runs from
    `x0 - T/2` to `x0 + T/2`, and from 0 up to `T`."""

    def __init__(self, x0, materials):
        self.x0 = x0
        self.m = materials

    def place(self, obj):
        obj.location.x += self.x0
        return obj

    def soil(self, surface=None, left=False, right=False):
        """The earth's face, below `surface(x)` if given, `(a, b)` for
        `z = a + b x`, and ending at the window's left or right edge."""
        mesh = bpy.data.meshes.new("soil")
        bm = bmesh.new()
        steps = 64
        size = T / steps
        cols, rows = 3 * steps + 1, 3 * steps + 1
        verts = [
            [
                bm.verts.new((x, -relief(x, z), z))
                for x in (-1.5 * T + i * size for i in range(cols))
            ]
            for z in (-T + j * size for j in range(rows))
        ]
        for j in range(rows - 1):
            for i in range(cols - 1):
                bm.faces.new((verts[j][i], verts[j][i + 1], verts[j + 1][i + 1], verts[j + 1][i]))
        if surface:
            a, b = surface
            cut(bm, (0, 0, a), (-b, 0, 1))
        if left:
            cut(bm, (-T / 2, 0, 0), (-1, 0, 0))
        if right:
            cut(bm, (T / 2, 0, 0), (1, 0, 0))
        bm.to_mesh(mesh)
        bm.free()
        for poly in mesh.polygons:
            poly.use_smooth = True
        mesh.materials.append(self.m["soil"])
        obj = bpy.data.objects.new("soil", mesh)
        bpy.context.scene.collection.objects.link(obj)
        self.place(obj)
        under = (lambda x: surface[0] + surface[1] * x - 0.6) if surface else (lambda x: 9.0)
        for n, (px, pz, r) in enumerate(PEBBLE_SPOTS):
            for x, z in copies(px, pz):
                if z + r > under(x) or (left and x - r < -T / 2) or (right and x + r > T / 2):
                    continue
                mat = self.m["pebbles"][n % len(self.m["pebbles"])]
                self.place(cr.ball("pebble", mat, (r, r * 0.6, r * 0.75), (x, 0.0, z),
                                   (0, math.radians(n * 47 % 180), 0)))

    def grass(self, surface, left=False, right=False):
        """A band of turf along `surface`, `(a, b)`, its top just below
        the line, with blades poking up behind it, up to the line, and a
        fringe hanging below."""
        a, b = surface
        x0 = -T / 2 + 0.01 if left else -1.5 * T
        x1 = T / 2 - 0.01 if right else 1.5 * T
        line = lambda x: a + b * x  # noqa: E731
        outline = [(x0, line(x0) - 0.1), (x1, line(x1) - 0.1),
                   (x1, line(x1) - 0.54), (x0, line(x0) - 0.54)]
        self.place(cr.prism("turf", self.m["grass"], outline, 0.3, (0, 0.05, 0), bevel=0.07))
        rng = random.Random(9)
        count = 18
        for k in range(count):
            px = -T / 2 + (k + 0.5 + rng.uniform(-0.3, 0.3)) * T / count
            tuft = [(rng.uniform(-0.03, 0.03), rng.uniform(0.04, 0.08), rng.uniform(-30, 30),
                     rng.uniform(-0.08, 0.0)) for _ in range(3)]
            hang = rng.uniform(0.05, 0.11)
            sway = rng.uniform(-15, 15)
            for x in (px - T, px, px + T):
                if x < x0 + 0.05 or x > x1 - 0.05:
                    continue
                for dx, tall, lean, depth in tuft:
                    blade = cr.lathe("blade", self.m["blade"], [(0.024, 0), (0.0, tall + 0.08)],
                                     (x + dx, depth, line(x + dx) - 0.16),
                                     (0, math.radians(lean), 0), segments=6)
                    blade.scale = (1.0, 0.4, 1.0)
                    self.place(blade)
                self.place(cr.lathe("fringe", self.m["fringe"],
                                    [(0.024, 0), (0.0, hang + 0.04)], (x, -0.1, line(x) - 0.5),
                                    (0, math.radians(180 + sway), 0), segments=6))


def materials():
    steel = cr.material("steel", STEEL, roughness=0.3)
    steel.node_tree.nodes["Principled BSDF"].inputs["Metallic"].default_value = 0.7
    return {
        "soil": cr.material("soil", SOIL, roughness=0.95),
        "pebbles": [cr.material(f"pebble{i}", c, roughness=0.7) for i, c in enumerate(PEBBLES)],
        "grass": cr.material("grass", GRASS, roughness=0.8, sheen=0.4),
        "blade": cr.material("blade", BLADE, roughness=0.7, sheen=0.4),
        "fringe": cr.material("fringe", FRINGE, roughness=0.8),
        "wood": cr.material("wood", WOOD, roughness=0.7),
        "dark wood": cr.material("dark wood", "#7a5230", roughness=0.8),
        "nail": cr.material("nail", "#4a4658", roughness=0.4),
        "steel": steel,
        "stone": cr.material("stone", STONE, roughness=0.85),
        "rock": cr.material("rock", "#a59a8a", roughness=0.8),
        "mortar": cr.material("mortar", MORTAR, roughness=0.95),
    }


# The tiles, in order: each builds its model on a `Tile`.


def grass(left=False, right=False):
    def build(t):
        t.soil(left=left, right=right)
        t.grass((T, 0), left=left, right=right)
    return build


def earth(t):
    t.soil()


def stones(t):
    t.soil()
    for x, z, r, turn in [(-0.32, 1.02, 0.22, 20), (0.36, 0.4, 0.17, -35)]:
        t.place(cr.ball("rock", t.m["rock"], (r, r * 0.55, r * 0.72), (x, 0.0, z),
                        (0, math.radians(turn), 0)))


def plank(left=False, right=False):
    def build(t):
        x0 = -T / 2 + 0.04 if left else -1.5 * T
        x1 = T / 2 - 0.04 if right else 1.5 * T
        width, middle = x1 - x0, (x0 + x1) / 2
        t.place(cr.box("backing", t.m["dark wood"], (width - 0.1, 0.1, 0.5),
                       (middle, 0.15, 1.33), bevel=0.2))
        for z in (1.46, 1.18):
            t.place(cr.box("board", t.m["wood"], (width, 0.3, 0.27), (middle, 0, z), bevel=0.3))
            for px in (-T / 4, T / 4):
                for x in (px - T, px, px + T):
                    if x0 + 0.1 < x < x1 - 0.1:
                        t.place(cr.ball("nail", t.m["nail"], (0.028, 0.02, 0.028),
                                        (x, -0.15, z)))
        for end, edge in ((left, x0 + 0.2), (right, x1 - 0.2)):
            if end:
                t.place(cr.box("peg", t.m["dark wood"], (0.14, 0.16, 0.42), (edge, 0.02, 0.86),
                               bevel=0.4))
    return build


def slope(a, b):
    def build(t):
        t.soil((a, b))
        t.grass((a, b))
    return build


def spikes(t):
    t.place(cr.box("base", t.m["nail"], (3 * T, 0.4, 0.2), (0, 0.05, 0.08), bevel=0.3))
    for x in (-1.2, -0.4, 0.4, 1.2):
        t.place(cr.lathe("spike", t.m["steel"],
                         [(0.3, 0.1), (0.26, 0.22), (0.0, 1.12)], (x, 0, 0), segments=24))


def block(t):
    t.place(cr.box("mortar", t.m["mortar"], (T, 0.1, T), (0, 0.3, T / 2), bevel=0.01))
    t.place(cr.box("stone", t.m["stone"], (1.5, 0.5, 1.5), (0, 0, T / 2), bevel=0.14))


TILES = [
    grass(left=True), grass(), grass(right=True), earth, stones,
    plank(left=True), plank(), plank(right=True),
    # z = a + b x across the window, x from its middle: the slopes run
    # between the corners and the middles of the tiles' sides.
    slope(T / 2, 1), slope(T / 2, -1),
    slope(T / 4, 0.5), slope(3 * T / 4, 0.5),
    slope(3 * T / 4, -0.5), slope(T / 4, -0.5),
    spikes, block,
]


def main():
    content, quick = cr.args()
    scene = cr.reset()
    cr.suns(scene)
    cam = cr.camera(scene, (TILE, TILE), (TILE // 2, TILE))
    m = materials()
    for i, build in enumerate(TILES):
        build(Tile(i * APART, m))

    tmp = os.path.join(os.environ.get("TMPDIR", "/tmp"), "clamber-terrain")
    os.makedirs(tmp, exist_ok=True)
    size = TILE * cr.SCALE
    cell = size + 2 * PAD
    rows = math.ceil(len(TILES) / COLUMNS)
    sheets = {"": cr.Image(COLUMNS * cell, rows * cell),
              "-normals": cr.Image(COLUMNS * cell, rows * cell)}
    view = scene.view_settings
    for suffix, sheet in sheets.items():
        if suffix == "-normals":
            scene.view_layers[0].material_override = cr.normal_material()
            scene.cycles.samples = 32
            view.view_transform = "Raw"
        else:
            scene.cycles.samples = 16 if quick else 256
            view.view_transform = "Standard"
        for i in range(len(TILES)):
            cam.location.x = i * APART
            image = cr.render(scene, os.path.join(tmp, f"tile{suffix}{i}.png"))
            at = (PAD + i % COLUMNS * cell, PAD + i // COLUMNS * cell)
            sheet.paste(image, (0, 0, size, size), at)
            sheet.extrude((*at, size, size), PAD)
        path = os.path.join(content, "tiles", f"terrain{suffix}.png")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        cr.write_png(path, sheet)
        print(f"wrote {path}: {len(TILES)} tiles, {sheet.w}x{sheet.h}")


main()
