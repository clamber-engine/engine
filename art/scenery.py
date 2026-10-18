"""More background layers, for the jungle, the treetops and the cave, as
stills in `backgrounds/` that tile horizontally like those of
`backgrounds.py`, whose pieces they share:

- `jungle-far.png`: rows of jungle crowns on trunks seen through the
  air, the farther paler. Transparent above.
- `jungle-near.png`: nearer, darker trunks rising out of the top, with
  vines, and bushes along the bottom. Transparent between.
- `light-shafts.png`: slanting shafts of sunlight, brightest at the top
  and fading out downwards, for a layer drawn in front at part opacity.
- `canopy.png`: the tops of the forest seen from up in the trees, rolling
  away into the haze. Transparent above.
- `cave-far.png`: the cave's far wall of rock, dim, with a few crystals
  glowing in it. Opaque.
- `cave-near.png`: a nearer ceiling with stalactites and a floor with
  stalagmites, dark. Transparent between.

Run by `tools/render-art.sh`, or on its own:

    blender -b --factory-startup -P art/scenery.py -- <content folder> [--quick]
"""

import math
import os
import random
import sys

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(here, "lib"))
sys.path.insert(0, here)

import bmesh  # noqa: E402
import bpy  # noqa: E402

import backgrounds as bg  # noqa: E402
import clamber_props as cp  # noqa: E402
import clamber_render as cr  # noqa: E402

JUNGLE_FAR, JUNGLE_NEAR, SHAFTS, CANOPY, CAVE = 150, 180, 216, 120, 216
CAVE_HAZE = "#1c1e2e"


def crowns(name, rng, depth, count, base, size, fog, colors, haze=bg.HAZE, trunk=True):
    """A row of round crowns `depth` from the camera, `base` up, about
    `size` across, spread evenly over the period."""
    mats = [cr.hazy(f"{name} {c}", haze, c, fog, roughness=0.8) for c in colors]
    bark = cr.hazy(f"{name} bark", haze, "#5a4030", fog)
    for i in range(count):
        x0 = bg.PERIOD * (i + rng.uniform(-0.3, 0.3)) / count
        z0 = base * rng.uniform(0.8, 1.2)
        r0 = size * rng.uniform(0.8, 1.2)
        blobs = [(rng.uniform(-0.6, 0.6) * r0, rng.uniform(-0.2, 0.4) * r0, r0 * rng.uniform(0.55, 0.8))
                 for _ in range(4)]
        shade = rng.randrange(len(mats))
        for x in bg.repeats(x0, reach=2 * r0):
            if trunk:
                cr.lathe(f"{name} trunk", bark, [(0.12 * size, 0), (0.08 * size, z0)],
                         (x, depth + 0.5, 0), segments=10)
            for k, (dx, dz, r) in enumerate(blobs):
                cp.leafy(f"{name} crown", mats[(shade + k) % len(mats)],
                         (x + dx, depth - k * 0.1, z0 + dz), (r, r * 0.8, r * 0.75),
                         random.Random(i * 10 + k), scale=0.12 * size + 0.1, strength=0.3 * size)


def understory(name, depth, height, fog, color, haze=bg.HAZE):
    """Rolling growth along the bottom, `height` high, so nothing stands in
    the air."""
    crest = bg.waves(int(depth * 10), 12, 0.25)
    mat = cr.hazy(name, haze, color, fog, roughness=0.85)
    bg.heightfield(name, mat, lambda x, y: height + crest(x), (depth - 0.5, depth + 0.5))


def jungle_far(quick):
    scene = bg.setup(JUNGLE_FAR, quick)
    rng = random.Random(3)
    for depth, count, base, size, fog, colors in [
        (34.0, 14, 9.5, 1.6, 0.6, ("#4f8a62", "#5f9a6a")),
        (24.0, 11, 7.0, 1.9, 0.42, ("#3f7f55", "#4f9058")),
        (15.0, 9, 4.5, 2.2, 0.25, ("#2f7048", "#3f8550", "#357a4a")),
    ]:
        crowns(f"crowns {depth}", rng, depth, count, base, size, fog, colors)
        understory(f"floor {depth}", depth - 2, base * 0.35, fog, colors[0])
    return scene


def jungle_near(quick):
    scene = bg.setup(JUNGLE_NEAR, quick)
    rng = random.Random(7)
    top = JUNGLE_NEAR * cr.UNIT
    fog = 0.12
    bark = cr.hazy("near bark", bg.HAZE, "#4e3828", fog, roughness=0.9)
    dark = cr.hazy("near dark bark", bg.HAZE, "#3e2c20", fog, roughness=0.9)
    vine = cr.hazy("near vine", bg.HAZE, "#2f6a32", fog)
    leaf = cr.hazy("near leaf", bg.HAZE, "#2f7a3d", fog)
    bushes = [cr.hazy(f"near bush {c}", bg.HAZE, c, fog) for c in ("#2a6a3a", "#357a3e")]
    for i, x0 in enumerate((3.0, 12.5, 21.0, 31.5)):
        r = rng.uniform(0.7, 1.0)
        for x in bg.repeats(x0, reach=3.0):
            cp.tube("trunk", (bark, dark)[i % 2],
                    cp.curve((x, 6.0, -0.5), (x + rng.uniform(-0.5, 0.5), 6.0, top + 1.0),
                             steps=10, wobble=0.1, seed=i),
                    [r * 1.3] + [r] * 10, sides=14)
            for k in range(3):
                vx = x + rng.uniform(-3.0, 3.0)
                length = rng.uniform(4.0, 10.0)
                points = cp.curve((vx, 5.0, top + 0.5), (vx + rng.uniform(-0.4, 0.4), 5.0,
                                                         top - length),
                                  steps=12, wobble=0.1, seed=i * 7 + k)
                cp.tube("vine", vine, points, 0.06, sides=6)
                for p in points[2::2]:
                    cp.leaf("vine leaf", leaf, (p.x, p.z), rng.choice((-1, 1)) * rng.uniform(100, 150),
                            0.5, 0.26, tilt=rng.uniform(-30, 30), y=p.y - 0.05)
            # The canopy overhead, its underside showing at the top.
            for k in range(4):
                cp.leafy("overhead", bushes[k % 2], (x + rng.uniform(-3, 3), 6.5,
                                                     top - rng.uniform(0.2, 0.8)),
                         (2.4, 1.5, 1.8), random.Random(i * 5 + k), scale=0.3, strength=0.5)
    for i in range(12):
        x0 = bg.PERIOD * (i + rng.uniform(-0.3, 0.3)) / 12
        r = rng.uniform(1.0, 1.8)
        for x in bg.repeats(x0, reach=2 * r):
            cp.leafy("bush", bushes[i % 2], (x, 4.0, r * 0.3), (r * 1.3, r, r * 0.8),
                     random.Random(i), scale=0.25, strength=0.5)
    return scene


def light_shafts(quick):
    scene = bg.setup(SHAFTS, quick)
    top = SHAFTS * cr.UNIT
    mat = bpy.data.materials.new("shaft")
    mat.use_nodes = True
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    nodes.remove(nodes["Principled BSDF"])
    position = nodes.new("ShaderNodeNewGeometry")
    z = nodes.new("ShaderNodeSeparateXYZ")
    fade = nodes.new("ShaderNodeMapRange")
    fade.inputs["From Min"].default_value = 0.0
    fade.inputs["From Max"].default_value = top
    fade.inputs["To Min"].default_value = 0.0
    fade.inputs["To Max"].default_value = 0.55
    light = nodes.new("ShaderNodeEmission")
    light.inputs["Color"].default_value = (*cr.linear("#fff4c8"), 1.0)
    light.inputs["Strength"].default_value = 1.0
    clear = nodes.new("ShaderNodeBsdfTransparent")
    mix = nodes.new("ShaderNodeMixShader")
    links.new(position.outputs["Position"], z.inputs["Vector"])
    links.new(z.outputs["Z"], fade.inputs["Value"])
    links.new(fade.outputs["Result"], mix.inputs["Fac"])
    links.new(clear.outputs["BSDF"], mix.inputs[1])
    links.new(light.outputs["Emission"], mix.inputs[2])
    links.new(mix.outputs["Shader"], nodes["Material Output"].inputs["Surface"])
    rng = random.Random(5)
    lean = math.radians(-18)
    for x0, width in [(4.0, 1.2), (8.5, 0.6), (17.0, 1.8), (26.0, 0.8), (29.5, 1.3),
                      (35.0, 0.5)]:
        for x in bg.repeats(x0, reach=6.0):
            mesh = bpy.data.meshes.new("shaft")
            bm = bmesh.new()
            # Wider at the bottom, where the light has spread.
            spread = width * rng.uniform(1.3, 1.8)
            shift = math.tan(lean) * top
            corners = [(x - width / 2, top + 0.5), (x + width / 2, top + 0.5),
                       (x - shift + spread / 2, -0.5), (x - shift - spread / 2, -0.5)]
            verts = [bm.verts.new((cx, 10.0, cz)) for cx, cz in corners]
            bm.faces.new(verts)
            bm.to_mesh(mesh)
            bm.free()
            mesh.materials.append(mat)
            obj = bpy.data.objects.new("shaft", mesh)
            obj.visible_shadow = False
            scene.collection.objects.link(obj)
    return scene


def canopy(quick):
    scene = bg.setup(CANOPY, quick)
    rng = random.Random(11)
    for depth, count, base, size, fog, colors in [
        (36.0, 18, 6.5, 1.5, 0.55, ("#5f9a6a", "#6aa572")),
        (26.0, 14, 5.0, 1.9, 0.38, ("#4f9058", "#5aa05e")),
        (17.0, 11, 3.2, 2.3, 0.2, ("#3f8550", "#4b9a4a", "#357a4a")),
    ]:
        crowns(f"tops {depth}", rng, depth, count, base, size, fog, colors, trunk=False)
        understory(f"under {depth}", depth + 1.0, base, fog + 0.05, colors[0])
    return scene


def wall(name, mat, depth, relief, height):
    """A wall of rock facing the camera `depth` away, over a period and
    `height` Blender units up, bulging towards the camera by
    `relief(x, z)`."""
    step = 0.1
    xs = [-2.0 + i * step for i in range(int((bg.PERIOD + 4.0) / step) + 1)]
    zs = [-0.5 + j * step for j in range(int((height + 1.0) / step) + 1)]
    mesh = bpy.data.meshes.new(name)
    bm = bmesh.new()
    grid = [[bm.verts.new((x, depth - relief(x, z), z)) for x in xs] for z in zs]
    for j in range(len(zs) - 1):
        for i in range(len(xs) - 1):
            bm.faces.new((grid[j][i], grid[j + 1][i], grid[j + 1][i + 1], grid[j][i + 1]))
    bm.to_mesh(mesh)
    bm.free()
    for poly in mesh.polygons:
        poly.use_smooth = True
    mesh.materials.append(mat)
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def cave_setup(quick):
    scene = bg.setup(CAVE, quick)
    scene.world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.05
    for light in scene.objects:
        if light.type == "LIGHT":
            light.data.energy *= 0.35
    return scene


def spikes(name, mat, rng, depth, count, top, length, down):
    for i in range(count):
        x0 = bg.PERIOD * (i + rng.uniform(-0.35, 0.35)) / count
        size = length * rng.uniform(0.4, 1.0)
        width = size * rng.uniform(0.18, 0.28)
        profile = [(width * (1 - t / 8) ** 1.3 * (1 + 0.15 * math.sin(t * 1.7 + i)) if t < 8
                    else 0, size * t / 8) for t in range(9)]
        for x in bg.repeats(x0, reach=width):
            cr.lathe(name, mat, profile, (x, depth + rng.uniform(-1, 1), top),
                     (math.pi if down else 0, 0, 0), segments=12)


def cave_far(quick):
    scene = cave_setup(quick)
    height = CAVE * cr.UNIT
    rough = bg.bumps(31, 30, 0.015)
    swell = bg.waves(37, 6, 1.2)
    ledges = bg.waves(39, 9, 0.5)

    def relief(x, z):
        # Bulges of rock, in bands that rise and fall along the wall.
        band = math.sin(z * 1.1 + ledges(x)) * 0.6 + math.sin(z * 0.37 + swell(x)) * 0.8
        return band + swell(x) * 0.5 + rough(x, z)

    rock = cr.hazy("far rock", CAVE_HAZE, None, 0.55, roughness=0.9)
    obj = wall("far wall", rock, 40.0, relief, height)
    # Lighter where the rock stands out, darker in the hollows.
    cr.paint(obj, lambda x, y, z: bg.mix("#2e2c42", "#5e5a76", 0.5 + (40.0 - y) * 0.4), 0.5)
    rng = random.Random(13)
    mid = cr.hazy("mid rock", CAVE_HAZE, "#4a4762", 0.4, roughness=0.9)
    spikes("far stalactite", mid, rng, 30.0, 16, height + 0.3, 5.0, True)
    spikes("far stalagmite", mid, rng, 30.0, 12, -0.3, 3.5, False)
    for i in range(9):
        x0 = bg.PERIOD * (i + rng.uniform(-0.3, 0.3)) / 9
        z0 = rng.uniform(0.15, 0.85) * height
        glow, _ = cr.glow(f"far crystal {i}", ("#3fb8e0", "#7a5ce0", "#4fd89a")[i % 3], 1.2)
        for x in bg.repeats(x0):
            for k in range(3):
                cr.lathe("crystal", glow, [(0.12, 0), (0.12, 0.35), (0, 0.5)],
                         (x + (k - 1) * 0.15, 38.0, z0), (0, math.radians((k - 1) * 30), 0),
                         segments=6, smooth=False)
    return scene


def cave_near(quick):
    scene = cave_setup(quick)
    height = CAVE * cr.UNIT
    rng = random.Random(17)
    rock = cr.hazy("near rock", CAVE_HAZE, "#34324a", 0.2, roughness=0.9)
    ceiling = bg.waves(43, 8, 1.0)
    floor = bg.waves(47, 8, 0.6)
    # The ceiling, hanging down from the top.
    top = bg.heightfield("ceiling", rock, lambda x, y: 2.0 + ceiling(x), (8.0, 14.0))
    top.scale.z = -1.0
    top.location.z = height + 0.5
    bg.heightfield("floor", rock, lambda x, y: 1.4 + floor(x), (8.0, 14.0))
    spikes("stalactite", rock, rng, 10.0, 14, height - 1.0, 6.0, True)
    spikes("stalagmite", rock, rng, 10.0, 10, 1.0, 4.0, False)
    return scene


bg.write([("jungle-far", jungle_far), ("jungle-near", jungle_near),
          ("light-shafts", light_shafts), ("canopy", canopy), ("cave-far", cave_far),
          ("cave-near", cave_near)])
