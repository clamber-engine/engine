"""The background layers, as `backgrounds/sky.png`, `mountains.png` and
`hills.png`: from farthest to nearest, each a still that tiles
horizontally.

- The sky: a gradient, paler towards the horizon, with clouds, the
  farther ones smaller and fainter.
- The mountains: two ridges of rock with snow on the heights, seen
  through more air the farther they are. Transparent above.
- The hills: two bands of rolling grass with round trees. Transparent
  above.

Everything is built from code. Shapes repeat every `PERIOD` across
(ridges as waves that fit it a whole number of times, clouds and trees
copied a period away), so each still's left edge meets its right. The
farther something is, the more of what's seen is haze, the colour of the
sky at the horizon. Lit by distant suns, from the same directions as the
sprites. Run by `tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/backgrounds.py -- <content folder> [--quick]
"""

import math
import os
import random
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))

import bmesh  # noqa: E402
import bpy  # noqa: E402

import clamber_render as cr  # noqa: E402

# Each still's size in world units, as wide as the view.
WIDTH = 384
SKY, MOUNTAINS, HILLS = 216, 160, 120
# Blender units across before the still repeats.
PERIOD = WIDTH * cr.UNIT
HAZE = "#b4dcf0"


def waves(seed, count, amplitude, falloff=1.0):
    """A height that repeats every period: `count` waves fitting it 1 to
    `count` times, the k-th `amplitude / k ** falloff` high."""
    rng = random.Random(seed)
    terms = [
        (k, amplitude / k ** falloff, rng.uniform(0, 2 * math.pi)) for k in range(1, count + 1)
    ]
    return lambda x: sum(a * math.cos(2 * math.pi * k * x / PERIOD + p) for k, a, p in terms)


def heightfield(name, mat, height, depth, step=0.05):
    """A grid over a period and a little each side across, and `depth`,
    `(near, far)`, away from the camera, at `height(x, y)` but never below
    0."""
    near, far = depth
    xs = [-2.0 + i * step for i in range(int((PERIOD + 4.0) / step) + 1)]
    rows = max(2, int((far - near) / (4 * step)) + 1)
    ys = [near + j * (far - near) / (rows - 1) for j in range(rows)]
    mesh = bpy.data.meshes.new(name)
    bm = bmesh.new()
    grid = [[bm.verts.new((x, y, max(0.0, height(x, y)))) for x in xs] for y in ys]
    for j in range(len(ys) - 1):
        for i in range(len(xs) - 1):
            bm.faces.new((grid[j][i], grid[j][i + 1], grid[j + 1][i + 1], grid[j + 1][i]))
    bm.to_mesh(mesh)
    bm.free()
    for poly in mesh.polygons:
        poly.use_smooth = True
    mesh.materials.append(mat)
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def repeats(x, reach=3.0):
    """`x` and its copies a period away that are near enough to be seen."""
    return [x + d for d in (-PERIOD, 0, PERIOD) if -reach < x + d < PERIOD + reach]


def mix(a, b, t):
    """Linear RGB between `#rrggbb` colours."""
    t = min(max(t, 0.0), 1.0)
    return tuple(p + (q - p) * t for p, q in zip(cr.linear(a), cr.linear(b)))


def setup(height, quick):
    """A fresh scene framing a period across and `height` world units up
    from z = 0."""
    scene = cr.reset()
    cr.suns(scene)
    cr.camera(scene, (WIDTH, height), (0, height))
    scene.cycles.samples = 16 if quick else 128
    scene.view_settings.view_transform = "Standard"
    return scene


def sky(quick):
    scene = setup(SKY, quick)
    top = SKY * cr.UNIT
    backdrop = bpy.data.materials.new("backdrop")
    backdrop.use_nodes = True
    nodes, links = backdrop.node_tree.nodes, backdrop.node_tree.links
    nodes.remove(nodes["Principled BSDF"])
    position = nodes.new("ShaderNodeNewGeometry")
    z = nodes.new("ShaderNodeSeparateXYZ")
    span = nodes.new("ShaderNodeMapRange")
    span.inputs["From Min"].default_value = 0.0
    span.inputs["From Max"].default_value = top
    ramp = nodes.new("ShaderNodeValToRGB")
    stops = ramp.color_ramp.elements
    stops[0].color = (*cr.linear(HAZE), 1.0)
    stops[1].position = 1.0
    stops[1].color = (*cr.linear("#5aaee0"), 1.0)
    middle = stops.new(0.45)
    middle.color = (*cr.linear("#8ccaec"), 1.0)
    light = nodes.new("ShaderNodeEmission")
    links.new(position.outputs["Position"], z.inputs["Vector"])
    links.new(z.outputs["Z"], span.inputs["Value"])
    links.new(span.outputs["Result"], ramp.inputs["Fac"])
    links.new(ramp.outputs["Color"], light.inputs["Color"])
    links.new(light.outputs["Emission"], nodes["Material Output"].inputs["Surface"])
    mesh = bpy.data.meshes.new("backdrop")
    mesh.from_pydata([(-10, 60, -2), (PERIOD + 10, 60, -2), (PERIOD + 10, 60, top + 2),
                      (-10, 60, top + 2)], [], [(0, 1, 2, 3)])
    mesh.materials.append(backdrop)
    plane = bpy.data.objects.new("backdrop", mesh)
    plane.visible_shadow = False
    scene.collection.objects.link(plane)

    # (x, z, size, fog): near clouds high and large, far ones low and small.
    for n, (x, z, size, fog) in enumerate([
        (5.5, 16.5, 1.0, 0.0), (19.5, 12.5, 0.65, 0.2), (31.5, 18.0, 1.25, 0.0),
        (13.0, 7.0, 0.45, 0.4), (26.5, 6.0, 0.38, 0.45), (36.5, 9.5, 0.5, 0.35),
    ]):
        cloud(f"cloud{n}", x, z, size, fog, n)
    return scene


def cloud(name, x, z, size, fog, seed):
    """A heap of puffs, flat below, about `size` * 5 across."""
    mat = cr.hazy(name, HAZE, "#ffffff", fog, roughness=1.0)
    shader = mat.node_tree.nodes["Principled BSDF"]
    shader.inputs["Sheen Weight"].default_value = 0.6
    shader.inputs["Subsurface Weight"].default_value = 0.2
    rng = random.Random(seed)
    puffs = []
    for i in range(9):
        across = (i / 8 - 0.5) * 4.4
        r = (1.25 - abs(across) / 3.4) * rng.uniform(0.75, 1.05)
        puffs.append((across + rng.uniform(-0.25, 0.25), r * 0.55 + rng.uniform(-0.1, 0.2), r))
    for at in repeats(x, reach=4.0 * size):
        for px, pz, r in puffs:
            cr.ball(name, mat, (r * size, r * size * 0.6, r * size * 0.82),
                    (at + px * size, 20.0 + rng.uniform(-1, 1), z + pz * size))
        cr.ball(name, mat, (2.4 * size, 0.6 * size, 0.45 * size), (at, 20.0, z + 0.1 * size))


def peaks(seed, count, depth, tall, wide):
    """A range of `count` peaks, `depth` `(near, far)` from the camera,
    `tall` `(least, most)` high and `wide` across their foot: the height
    at `(x, y)` is the highest of the peaks there, so they meet in
    valleys, made rough and with a low foot under them all."""
    rng = random.Random(seed)
    tops = [
        (PERIOD * (i + rng.uniform(-0.3, 0.3)) / count, rng.uniform(*depth),
         rng.uniform(*tall), rng.uniform(*wide))
        for i in range(count)
    ]
    rough = bumps(seed + 1, 40, 0.1)

    def height(x, y):
        h = 0.0
        for px, py, ph, pw in tops:
            dx = (x - px + PERIOD / 2) % PERIOD - PERIOD / 2
            d = math.hypot(dx, (y - py) * 0.7) / pw
            if d < 1.0:
                h = max(h, ph * (1.0 - d) ** 1.7)
        foot = 0.22 * tall[0]
        return max(h, foot) + rough(x, y) * (0.3 + h / tall[1])
    return height


def bumps(seed, count, amplitude):
    """Small roughness every way, repeating every period across: waves
    from `count` random directions, finer ones lower."""
    rng = random.Random(seed)
    terms = []
    for _ in range(count):
        k = rng.randint(8, 90)
        angle = rng.uniform(-1.2, 1.2)
        across = max(1, round(k * math.cos(angle)))
        terms.append((2 * math.pi * across / PERIOD, 2 * math.pi * k * math.sin(angle) / PERIOD,
                      amplitude * (20 / k) ** 0.8, rng.uniform(0, 2 * math.pi)))
    return lambda x, y: sum(a * math.sin(fx * x + fy * y + p) for fx, fy, a, p in terms)


def mountains(quick):
    scene = setup(MOUNTAINS, quick)
    for name, seed, depth, tall, wide, fog, rock, snowline in [
        ("far ridge", 11, (30.0, 46.0), (9.0, 14.5), (9.0, 13.0), 0.55, "#6a7290", 10.4),
        ("near ridge", 23, (12.0, 20.0), (4.5, 8.0), (6.5, 9.5), 0.22, "#56607e", 99.0),
    ]:
        height = peaks(seed, 9, depth, tall, wide)
        mat = cr.hazy(name, HAZE, roughness=0.9)
        obj = heightfield(name, mat, height, (depth[0] - wide[1], depth[1] + wide[1]),
                          step=0.06)
        middle = sum(depth) / 2
        # Snow where it's high and not too steep, its edge wavering.
        wobble = waves(51, 8, 0.35)

        def color(x, y, z, height=height, rock=rock, snowline=snowline):
            dz = (height(x + 0.05, y) - height(x - 0.05, y)) / 0.1
            steep = min(abs(dz) / 3.0, 1.0)
            snow = (z - snowline - wobble(x)) / 0.5 - steep * 0.7
            return mix(rock, "#eef2fa", snow)

        cr.paint(obj, color, lambda x, y, z, middle=middle, fog=fog: fog + (y - middle) * 0.01)
    return scene


def hills(quick):
    scene = setup(HILLS, quick)
    for name, seed, middle, half, base, swell, fog, grass, trees in [
        ("far hills", 61, 16.0, 8.0, 4.6, 1.3, 0.22, "#73bf6c", [3.0, 9.5, 11.0, 22.0, 30.5]),
        ("near hills", 67, 6.0, 5.0, 2.2, 0.9, 0.04, "#5aa955", [6.5, 15.0, 26.0, 34.0]),
    ]:
        crest = waves(seed, 5, swell)

        def height(x, y, crest=crest, middle=middle, half=half, base=base):
            t = (y - middle) / half
            return (base + crest(x)) * math.sqrt(max(0.0, 1.0 - t * t))

        mat = cr.hazy(name, HAZE, roughness=0.85)
        obj = heightfield(name, mat, height, (middle - half, middle + half))
        patches = waves(seed + 1, 16, 0.5, falloff=0.5)
        cr.paint(obj, lambda x, y, z, grass=grass: mix(grass, "#3f8f45", 0.35 + patches(x + y)),
                 fog)
        leaves = cr.hazy(f"{name} leaves", HAZE, "#2f7a3d", fog, roughness=0.8)
        light_leaves = cr.hazy(f"{name} light leaves", HAZE, "#4a9a4a", fog, roughness=0.8)
        bark = cr.hazy(f"{name} bark", HAZE, "#6e4630", fog)
        size = 1.0 if name == "near hills" else 0.75
        for n, tx in enumerate(trees):
            ty = middle - 0.55 * half
            for x in repeats(tx):
                tree(x, ty, height(x, ty), size, bark, leaves, light_leaves, n)
    return scene


def tree(x, y, z, size, bark, leaves, light_leaves, seed):
    """A round tree, `size` about 1.6 tall, standing at `(x, y, z)`."""
    rng = random.Random(seed)
    cr.lathe("trunk", bark, [(0.1 * size, -0.2), (0.07 * size, 0.7 * size), (0, 0.8 * size)],
             (x, y, z), segments=12)
    for dx, dz, r, mat in [(0.0, 1.05, 0.5, leaves), (-0.3, 0.85, 0.36, leaves),
                           (0.3, 0.88, 0.38, leaves), (-0.12, 1.22, 0.3, light_leaves)]:
        r *= size * rng.uniform(0.92, 1.08)
        cr.ball("canopy", mat, (r, r, r * 0.92), (x + dx * size, y - 0.1, z + dz * size))


def main():
    content, quick = cr.args()
    folder = os.path.join(content, "backgrounds")
    os.makedirs(folder, exist_ok=True)
    tmp = os.path.join(os.environ.get("TMPDIR", "/tmp"), "clamber-backgrounds")
    os.makedirs(tmp, exist_ok=True)
    for name, build in [("sky", sky), ("mountains", mountains), ("hills", hills)]:
        scene = build(quick)
        image = cr.render(scene, os.path.join(tmp, f"{name}.png"))
        path = os.path.join(folder, f"{name}.png")
        cr.write_png(path, image)
        print(f"wrote {path}: {image.w}x{image.h}")


main()
