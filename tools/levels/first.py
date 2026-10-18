#!/usr/bin/env python3
"""Builds content/base/levels/first.level, the first level.

The level is laid out here on a 16 px grid, left to right, and this script
writes its collision and its terrain tiles together, so the two always
match. The collision stays simple; the art over it doesn't have to
follow it: props are placed along the collision line, overhanging it,
and around it, in layers behind and in front of the player. Edit the
layout below, then run:

    python3 tools/levels/first.py

Rows count down from the top of the level; a ground height is the row
whose top edge is the ground's surface. Tile numbers are terrain.png's
(see art/terrain.py), plus one. Props are listed in props/<set>.json
(see art/jungle.py, art/treetops.py and art/cave.py).
"""

import json
import os
import sys

T = 16
ROWS = 60
# Texels per world unit in the art (see art/), and around each tile in
# terrain.png.
SCALE = 5
PAD = 4
BASE = 46
# The view, in world units.
VIEW = (384, 216)

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
PROPS = os.path.join(ROOT, "content/base/props")

# Tiles, as numbered in level data (the tileset's index plus one).
GRASS_LEFT, GRASS, GRASS_RIGHT, EARTH, STONES = 1, 2, 3, 4, 5
PLANK_LEFT, PLANK, PLANK_RIGHT = 6, 7, 8
STEEP_UP, STEEP_DOWN = 9, 10
GENTLE_UP_LOW, GENTLE_UP_HIGH = 11, 12
GENTLE_DOWN_HIGH, GENTLE_DOWN_LOW = 13, 14
SPIKES, BLOCK = 15, 16

# How a ground column's surface runs, by tile.
SURFACE_TILE = {
    "flat": GRASS,
    "up": STEEP_UP,
    "down": STEEP_DOWN,
    "up-low": GENTLE_UP_LOW,
    "up-high": GENTLE_UP_HIGH,
    "down-high": GENTLE_DOWN_HIGH,
    "down-low": GENTLE_DOWN_LOW,
}


class Course:
    """Ground laid out left to right, and what stands on it."""

    def __init__(self, row):
        self.col = 0
        self.row = row
        # Ground pieces between pits: lists of (col, kind, left y, right y).
        self.pieces = [[]]
        self.platforms = []  # (col, row, cols)
        self.blocks = []  # (col, row, cols, rows)
        self.ceilings = []  # (col, cols, top row, bottom row): rock overhead
        self.spikes = []  # (col, row, cols): spikes standing on `row`
        self.entities = []  # (comment, type, x, y, params)
        self.regions = []  # (name, first col, end col): camera locked sideways
        self.zones = [(0, "jungle")]  # (first col, props set): what it looks like
        self.lights = []  # (x, y, colour, radius)
        self.extra = []  # (layer, set, name, x, y, flip): props placed by hand

    # Ground.

    def column(self, kind, left, right):
        self.pieces[-1].append((self.col, kind, left, right))
        self.col += 1

    def flat(self, n):
        for _ in range(n):
            y = self.row * T
            self.column("flat", y, y)

    def up(self, rows, gentle=False):
        """Rises `rows`, one a column, or one every two when gentle."""
        for _ in range(rows):
            y = self.row * T
            if gentle:
                self.column("up-low", y, y - 8)
                self.column("up-high", y - 8, y - 16)
            else:
                self.column("up", y, y - T)
            self.row -= 1

    def down(self, rows, gentle=False):
        for _ in range(rows):
            y = self.row * T
            if gentle:
                self.column("down-high", y, y + 8)
                self.column("down-low", y + 8, y + 16)
            else:
                self.column("down", y, y + T)
            self.row += 1

    def steps(self, rows, cols):
        """A flight of steps, `rows` up (down when negative), each `cols`
        deep."""
        for _ in range(abs(rows)):
            self.cliff(-1 if rows > 0 else 1)
            self.flat(cols)

    def cliff(self, rows):
        """A sheer drop of `rows`, or a wall when negative."""
        self.row += rows

    def pit(self, cols, rows=0):
        """A bottomless gap; the ground beyond is `rows` lower."""
        self.col += cols
        self.row += rows
        self.pieces.append([])

    def zone(self, name):
        """From here on, the level looks like `name`: one of the props
        sets."""
        self.zones.append((self.col, name))

    # Things on the ground, relative to where the course has got to.

    def ground(self, col):
        """The surface height in px at `col`, or over a pit, at the last
        ground before it."""
        best = None
        for piece in self.pieces:
            for c, _, left, right in piece:
                if c <= col:
                    best = min(left, right)
        if best is None:
            raise ValueError(f"no ground at or before column {col}")
        return best

    def platform(self, col, rows_up, cols):
        """A one-way platform `rows_up` above the ground at `col`."""
        self.platforms.append((col, self.ground(col) // T - rows_up, cols))

    def block(self, col, row, cols, rows):
        self.blocks.append((col, row, cols, rows))

    def ceiling(self, col, cols, rows_up, thickness):
        """Rock overhead from `col`, its underside `rows_up` above the
        ground at `col`, `thickness` rows deep, grassed on top."""
        bottom = self.ground(col) // T - rows_up
        self.ceilings.append((col, cols, bottom - thickness, bottom))

    def spike(self, col, cols):
        self.spikes.append((col, self.ground(col) // T, cols))

    def thing(self, kind, col, rows_up=0, params=None, comment=None):
        """An entity over column `col`, `rows_up` above the ground (at its
        feet for walkers, checkpoints, the goal, springs, restart barrels
        and hidden objects; its centre otherwise)."""
        x = col * T + T // 2
        y = self.ground(col) - rows_up * T
        self.entities.append((comment, kind, x, y, params))

    def rope(self, col, row, comment=None):
        """A rope hanging over column `col`, its end at the top of `row`."""
        self.entities.append((comment, "rope", col * T + T // 2, row * T, None))

    def light(self, col, rows_up, colour, radius):
        self.lights.append((col * T + T // 2, self.ground(col) - rows_up * T, colour, radius))

    def prop(self, layer, kit, name, col, rows_up=0, flip=False):
        """A prop placed by hand, standing on the ground at `col`."""
        self.extra.append((layer, kit, name, col * T + T // 2, self.ground(col) - rows_up * T, flip))

    def region(self, name, col, end):
        self.regions.append((name, col, end))

    def gems(self, col, count, rows_up=1, step=2, kind="gem", comment=None):
        """A line of gems from `col`, `step` columns apart."""
        for i in range(count):
            c = col + i * step
            self.thing(kind, c, rows_up, comment=comment if i == 0 else None)

    def arc(self, col, count, rows_up, kind="gem", comment=None):
        """Gems along a jump's arc."""
        for i in range(count):
            lift = min(i, count - 1 - i) + rows_up
            self.thing(kind, col + i * 2, lift, comment=comment if i == 0 else None)


def build():
    c = Course(BASE)

    # The jungle floor. Start: a calm run past a sign, steps up, a
    # gentle rise that turns steep, and a crawler to meet.
    c.flat(24)
    c.prop("behind", "jungle", "sign", 13)
    c.gems(10, 5, comment="Start: a calm run, steps up and a crawler to meet.")
    c.steps(3, 4)
    c.gems(c.col - 11, 3, rows_up=2, step=4)
    c.flat(8)
    c.thing("crawler", c.col - 4)
    c.flat(6)
    c.up(2, gentle=True)
    c.up(1)
    c.flat(8)
    c.gems(c.col - 6, 3)
    c.down(1)
    c.down(2, gentle=True)
    c.flat(10)

    # Two pits to jump, the second with gems over it.
    c.pit(5)
    c.flat(10)
    c.arc(c.col - 2, 5, 2, comment="Gems over the second pit.")
    c.pit(6)
    c.flat(12)
    c.thing("crawler", c.col - 4)
    c.flat(4)

    # A bank to climb and a drop from it: landing from that high finds
    # a bunch hidden in the ground.
    c.up(3)
    c.flat(3)
    c.up(2, gentle=True)
    c.flat(5)
    c.cliff(5)
    c.flat(16)
    c.thing(
        "hidden",
        c.col - 10,
        params={"fall_height": 64, "across": [[-96, 0], [0, 96]], "above": [-4, 32]},
        comment="A bunch hidden where the drop lands: found by falling on it.",
    )
    c.thing("crawler", c.col - 3)
    c.flat(4)

    # A hill with spikes in a dip on top.
    c.up(2, gentle=True)
    c.flat(5)
    c.down(1, gentle=True)
    c.flat(4)
    c.spike(c.col - 3, 2)
    c.up(1, gentle=True)
    c.flat(5)
    c.down(2)
    c.flat(8)
    c.thing("checkpoint", c.col - 4, comment="The first checkpoint, at the foot of the trees.")
    c.flat(6)

    # Up into the treetops: boughs left and right in turn, while the
    # camera holds still sideways. A flutter crosses halfway up.
    c.zone("treetops")
    shaft = c.col
    c.flat(20)
    for i in range(6):
        left = i % 2 == 0
        c.platform(shaft + (2 if left else 11), 3 * (i + 1), 7)
        c.gems(shaft + (3 if left else 13), 2, rows_up=3 * (i + 1) + 1, step=3)
    c.thing("flutter", shaft + 4, 10, params={"path": [160, 0], "period": 200})
    c.region("shaft", shaft, c.col)
    c.cliff(-20)
    c.flat(8)
    top = c.row

    # Along the canopy: boughs over a long drop, a rope to swing across
    # the widest gap, and flutters between the branches.
    walk = c.col
    c.pit(68)
    c.platform(walk + 4, 0, 6)
    c.platform(walk + 14, 1, 5)
    c.thing("flutter", walk + 12, 2, params={"path": [0, -48], "period": 150})
    c.platform(walk + 23, 0, 4)
    c.gems(walk + 24, 2, rows_up=2, step=2)
    c.rope(walk + 33, top + 2, comment="A rope over the widest gap: jump to it, then on.")
    c.gems(walk + 31, 3, rows_up=3, step=2)
    c.platform(walk + 39, 0, 5)
    c.thing("relic", walk + 41, 6, comment="The first relic, above the bough past the rope.")
    c.platform(walk + 40, 4, 3)
    c.platform(walk + 48, 1, 4)
    c.thing("flutter", walk + 46, 1, params={"path": [0, -64], "period": 130})
    c.platform(walk + 56, 0, 5)
    c.gems(walk + 57, 2, rows_up=1, step=2)
    c.platform(walk + 63, 2, 3)

    # Down out of the trees to the jungle floor again.
    c.cliff(top + 4 - c.row)
    c.zone("jungle")
    c.flat(10)
    c.thing("tonic", c.col - 5, 1, comment="A tonic after the treetops.")
    c.down(4, gentle=True)
    c.flat(6)
    c.down(3)
    c.flat(6)

    # Into the cave. A restart barrel at its mouth; then crawlers,
    # spikes and a pit in the dark, lit by lanterns and crystals.
    c.thing("keg", c.col - 3, comment="A restart barrel at the cave's mouth.")
    c.zone("cave")
    cave = c.col
    c.flat(14)
    c.down(2)
    c.flat(12)
    c.thing("crawler", c.col - 4)
    c.light(cave + 6, 2, "#ffb060", 96)
    c.prop("behind", "cave", "lantern", cave + 6)
    c.steps(-2, 4)
    c.flat(8)
    c.spike(c.col - 6, 3)
    c.gems(c.col - 7, 3, rows_up=3, step=2)
    c.flat(6)
    c.light(c.col - 4, 1, "#70e0ff", 64)
    c.prop("ground", "cave", "crystals", c.col - 4)
    c.pit(6)
    c.flat(4)
    c.up(1)
    c.flat(10)
    c.thing("crawler", c.col - 3)
    c.flat(4)
    c.light(c.col - 2, 2, "#ffb060", 96)
    c.prop("behind", "cave", "lantern", c.col - 2)
    c.up(2, gentle=True)
    c.flat(8)
    c.thing("relic", c.col - 4, 1, comment="The second relic, deep in the cave.")
    c.light(c.col - 6, 1, "#b080ff", 64)
    c.prop("ground", "cave", "crystals", c.col - 6, flip=True)
    c.down(2)
    c.flat(12)
    c.thing("crawler", c.col - 5)
    c.flat(4)
    # A spring under the way out, up a wall too high to jump.
    c.thing("spring", c.col - 3, comment="A spring: jump from it up the wall out of the cave.")
    c.light(c.col - 6, 2, "#ffb060", 96)
    c.prop("behind", "cave", "lantern", c.col - 6)
    out = c.col
    c.ceiling(cave, out - cave - 8, 7, 12)
    c.cliff(-8)
    c.zone("jungle")
    c.flat(10)
    c.gems(c.col - 8, 3)
    c.down(4, gentle=True)
    c.flat(4)
    c.down(4)
    c.flat(10)
    c.thing("checkpoint", c.col - 4, comment="The second checkpoint, out of the cave.")
    c.flat(4)

    # Rolling hills: quick ups and downs, with crawlers in the dips and
    # flutters over the tops.
    for i in range(3):
        c.down(2, gentle=True)
        c.flat(6)
        c.thing("crawler", c.col - 3)
        c.up(3, gentle=True)
        c.flat(4)
        c.thing("flutter", c.col - 2, 3, params={"path": [0, -40], "period": 140 + 20 * i})
        c.gems(c.col - 4, 2, rows_up=1)
        c.down(1, gentle=True)
    c.flat(10)

    # The long slope down to a row of crawlers: roll through them.
    c.down(8, gentle=True)
    c.gems(c.col - 14, 4, rows_up=2, step=4, comment="Roll down the slope and through the crawlers.")
    c.flat(36)
    for i in range(4):
        c.thing("crawler", c.col - 30 + i * 7)
    c.thing("tonic", c.col - 6, 1, comment="A tonic past the crawlers, for a hit taken on the way.")
    c.flat(6)

    # A tower of stone blocks to climb, zigzagging up, with crawlers
    # pacing some of them. The camera holds still sideways again.
    tower = c.col
    c.flat(20)
    floor = c.ground(tower) // T
    for i, (dx, crawler) in enumerate(
        [(2, False), (8, False), (14, True), (8, False), (2, True), (8, False), (14, False)]
    ):
        row = floor - 3 * (i + 1)
        c.block(tower + dx, row, 4, 1)
        if crawler:
            c.entities.append((None, "crawler", (tower + dx + 2) * T, row * T, None))
        else:
            c.entities.append((None, "gem", (tower + dx + 2) * T, row * T - 12, None))
    c.region("tower", tower, c.col)
    c.cliff(-23)

    # From the top, a long way back down.
    c.flat(8)
    c.gems(c.col - 7, 3, comment="The top of the tower, and the way down.")
    c.down(5)
    c.flat(6)
    c.thing("crawler", c.col - 3)
    c.down(6, gentle=True)
    c.flat(4)
    c.down(4)
    c.flat(8)
    c.down(4, gentle=True)
    c.flat(6)

    # Stone blocks to hop across, over spikes.
    hop = c.col
    c.flat(30)
    c.spike(hop + 4, 22)
    c.block(hop + 3, c.ground(hop) // T - 3, 3, 1)
    c.block(hop + 10, c.ground(hop) // T - 4, 3, 1)
    c.block(hop + 17, c.ground(hop) // T - 3, 3, 1)
    c.gems(hop + 4, 3, rows_up=4, step=7, comment="Stepping stones over spikes.")
    c.thing("flutter", hop + 14, 6, params={"path": [64, 0], "period": 160})
    c.flat(6)
    c.thing("tonic", c.col - 3, 1, comment="A tonic past the spikes.")

    # A launcher up to a high shelf with the last relic, and a wall too
    # high to jump.
    lift = c.col
    c.flat(30)
    c.thing("launcher", lift + 3, comment="A launcher up to the last relic.")
    c.platform(lift + 1, 12, 8)
    c.thing("relic", lift + 6, 13)
    c.gems(lift + 2, 3, rows_up=13, step=2)
    c.thing("crawler", lift + 16)
    c.thing("crawler", lift + 24, params={"facing": "right"})
    c.thing("launcher", c.col - 3, comment="The launcher: the only way up the wall.")
    c.cliff(-13)
    c.flat(16)
    c.gems(c.col - 14, 5)

    # The last stretch: islands down from the top of the wall, and a
    # pit with a platform over it.
    c.pit(5, 4)
    c.flat(6)
    c.pit(6, -1)
    c.thing("flutter", c.col - 3, 1, params={"period": 150})
    c.flat(5)
    c.pit(6, 2)
    c.thing("flutter", c.col - 3, 2, params={"path": [0, -48]})
    c.flat(8)
    c.down(3)
    c.flat(4)
    c.down(3)
    c.flat(12)
    c.thing("crawler", c.col - 4)
    c.pit(8)
    c.platform(c.col - 6, 2, 4)
    c.flat(8)
    c.up(1, gentle=True)
    c.flat(4)
    c.up(1)
    c.flat(26)
    c.gems(c.col - 24, 5, step=3)
    c.thing("goal", c.col - 6, comment="The goal.")
    c.flat(6)
    return c


# Decoration: props along and around the collision, chosen by a hash of
# where they go, so the level comes out the same every time.


def noise(*keys):
    """A number from 0 to 1, the same for the same keys."""
    h = 2166136261
    for k in keys:
        for b in str(k).encode():
            h = ((h ^ b) * 16777619) & 0xFFFFFFFF
    return h / 0xFFFFFFFF


def load_kits():
    kits = {}
    for kit in ("jungle", "treetops", "cave"):
        with open(os.path.join(PROPS, f"{kit}.json")) as f:
            kits[kit] = json.load(f)["props"]
    return kits


class Dresser:
    """Props for the level's props layers, placed by where each one's
    line meets the ground (see props/<set>.json)."""

    def __init__(self, c):
        self.c = c
        self.kits = load_kits()
        self.layers = {name: [] for name in LAYERS}

    def zone(self, col):
        name = "jungle"
        for start, z in self.c.zones:
            if start <= col:
                name = z
        return name

    def put(self, layer, kit, name, x, line, flip=False):
        """`name` from `kit` with the line it grows from at `line`, its
        middle at `x`."""
        prop = self.kits[kit][name]
        self.layers[layer].append((kit, name, x, line + prop["ground"], flip))

    def width(self, kit, name):
        return self.kits[kit][name]["size"][0]

    def dress(self):
        for piece in self.c.pieces:
            for run in flat_runs(piece):
                self.flat_run(*run)
            for col, kind, left, right in piece:
                if kind != "flat":
                    self.slope(col, left, right)
        for col, row, n in self.c.platforms:
            self.platform(col, row, n)
        for col, n, top, bottom in self.c.ceilings:
            self.ceiling(col, n, top, bottom)
        for col, row, w, _ in self.c.blocks:
            if self.zone(col) == "jungle" and noise("block", col, row) < 0.5:
                self.put("ground", "jungle", "roots", (col + w / 2) * T, (row + 1) * T - 15)
        for layer, kit, name, x, y, flip in self.c.extra:
            prop = self.kits[kit][name]
            self.layers[layer].append((kit, name, x, y + prop["ground"], flip))
        self.backdrops()
        for layer in self.layers.values():
            layer.sort(key=lambda p: (p[2], p[3]))
        return self.layers

    def flat_run(self, x0, x1, y, open_left, open_right, zone=None):
        """Dresses flat ground from `x0` to `x1` at `y`, in `zone` or the
        one it's in; an open end has a drop beside it."""
        zone = zone or self.zone(int(x0 // T))
        kit = "cave" if zone == "cave" else "jungle"
        # Lips along the edge, overhanging it, and ends curling over drops.
        step = 44
        xs = list(range(int(x0) + 22, int(x1) - 22, step))
        if not xs or xs[-1] < x1 - 40:
            xs.append(x1 - 22)
        for x in xs:
            if kit == "cave":
                # A little low, over the grass at the top of the tiles.
                self.put("ground", "cave", "lip", x, y + 2, noise("lip", x) < 0.5)
            else:
                name = "lip" if noise("lip", x) < 0.6 else "lip-short"
                self.put("ground", "jungle", name, x, y, noise("flip", x) < 0.5)
        if kit == "jungle":
            if open_right:
                self.put("ground", "jungle", "lip-end", x1 - 14, y)
                self.put("ground", "jungle", "roots", x1 - 8, y + 6)
            if open_left:
                self.put("ground", "jungle", "lip-end", x0 + 14, y, flip=True)
                self.put("ground", "jungle", "roots", x0 + 8, y + 6, flip=True)
        # Small things on the ground, and big ones behind.
        x = x0 + 24
        while x < x1 - 24:
            n = noise("ground", x, y)
            if kit == "cave":
                choices = ["rocks", "stalagmite", "stalagmites", "glowcaps", None, None]
                name = choices[int(n * len(choices)) % len(choices)]
                if name:
                    self.put("ground", "cave", name, x, y, noise("f", x) < 0.5)
            else:
                choices = ["fern", "flowers", "rocks", "bush", "mushrooms", "fern", None]
                name = choices[int(n * len(choices)) % len(choices)]
                if name:
                    layer = "rocks" if name in ("rocks", "mushrooms") else "foliage"
                    self.put(layer, "jungle", name, x, y, noise("f", x) < 0.5)
            x += 24 + int(noise("gap", x) * 40)
        if kit == "jungle" and zone == "jungle":
            self.big_things(x0, x1, y)
            self.front(x0, x1, y)

    def big_things(self, x0, x1, y):
        """Trees, palms, logs and ruins behind the player."""
        x = x0 + 40 + noise("tree0", x0) * 60
        while x < x1 - 40:
            n = noise("tree", x)
            name = ("tree", "palm", "tree", "log", "pillar", "old-crates", "arch", "tree")[int(n * 8) % 8]
            self.put("behind", "jungle", name, x, y, noise("tf", x) < 0.5)
            x += 120 + noise("tgap", x) * 160

    def front(self, x0, x1, y):
        """Leaves and ferns in front of the player, now and then."""
        x = x0 + 80 + noise("front0", x0) * 120
        while x < x1 - 40:
            name = "front-leaves" if noise("fl", x) < 0.6 else "front-fern"
            # Low, so they hide only feet, never what's ahead.
            self.put("front", "jungle", name, x, y + 14, noise("ff", x) < 0.5)
            x += 360 + noise("fgap", x) * 300

    def slope(self, col, left, right):
        zone = self.zone(col)
        if zone == "cave" or noise("slope", col) < 0.7:
            return
        x = col * T + T // 2
        line = (left + right) // 2 + 2
        name = ("fern", "flowers", "fern", "bush")[int(noise("sl", col) * 4) % 4]
        self.put("foliage", "jungle", name, x, line, noise("sf", col) < 0.5)

    def platform(self, col, row, n):
        x0, x1, y = col * T, (col + n) * T, row * T
        if self.zone(col) == "treetops":
            # Boughs carry the platform; a trunk behind its middle reaches
            # down out of sight.
            x = x0 + 24
            while x < x1 - 8:
                self.put("ground", "treetops", "bough", x, y, noise("bough", x) < 0.5)
                x += 40
            for i in range(3):
                self.put("behind", "treetops", "trunk", (x0 + x1) / 2, y + 2 + 128 * (i + 1), i % 2 == 1)
            if noise("moss", col) < 0.6:
                self.put("ground", "treetops", "hanging-moss", x0 + 20 + noise("mx", col) * (x1 - x0 - 40), y + 4)
            if noise("canopy", col) < 0.35:
                self.put("front", "treetops", "front-canopy", (x0 + x1) / 2, y - 100)
        elif self.zone(col) == "jungle":
            self.put("ground", "jungle", "vine", x0 + 12, y + 4)

    def ceiling(self, col, n, top, bottom):
        """Stalactites under the rock, a fringe at its mouths, and a
        jungle on top."""
        x0, x1 = col * T, (col + n) * T
        under = bottom * T
        x = x0 + 30
        while x < x1 - 20:
            name = "stalactite" if noise("st", x) < 0.5 else "stalactites"
            self.put("ground", "cave", name, x, under, noise("stf", x) < 0.5)
            x += 30 + noise("stg", x) * 50
        self.put("ground", "jungle", "vine", x0 + 10, under)
        self.put("ground", "jungle", "roots", x0 + 24, under, True)
        self.put("front", "cave", "front-curtain", x0 + 36, under)
        self.put("front", "cave", "front-curtain", x1 - 36, under, flip=True)
        self.flat_run(x0, x1, top * T, True, True, zone="jungle")

    def backdrops(self):
        """The cave's walls behind it, as far as the rock overhead goes."""
        w, h = VIEW
        for col, n, top, bottom in self.c.ceilings:
            x0, x1 = col * T, (col + n) * T
            # From just inside the rock overhead, so no sky shows above.
            foot = bottom * T - T + h
            lefts = list(range(x0, x1 - w, w)) + [x1 - w]
            for x in lefts:
                # Twice over for the far wall, so it goes on down pits.
                for layer, image, y in (
                    ("cave far", "cave-far", foot),
                    ("cave far", "cave-far", foot + h),
                    ("cave near", "cave-near", foot),
                ):
                    self.layers[layer].append(("../backgrounds", image, x + w / 2, y, False))


def flat_runs(piece):
    """Each run of flat columns at one height in a piece of ground:
    (left x, right x, y, open on the left, open on the right)."""
    runs = []
    i = 0
    while i < len(piece):
        col, kind, left, _ = piece[i]
        if kind != "flat":
            i += 1
            continue
        j = i
        while j + 1 < len(piece) and piece[j + 1][1] == "flat" and piece[j + 1][2] == left:
            j += 1
        before = piece[i - 1][3] if i > 0 else None
        after = piece[j + 1][2] if j + 1 < len(piece) else None
        runs.append(
            (
                col * T,
                (piece[j][0] + 1) * T,
                left,
                before is None or before > left,
                after is None or after > left,
            )
        )
        i = j + 1
    return runs


# Props layers, back to front, and how each is drawn. Those before
# "terrain" are behind the ground.
LAYERS = {
    "cave far": {},
    "cave near": {},
    "behind": {},
    "ground": {},
    "rocks": {},
    "foliage": {"sway": (0.6, 3.5)},
    "front": {"depth": "front", "opacity": 0.75, "sway": (1.2, 4.5)},
}
BEHIND_TERRAIN = ("cave far", "cave near", "behind")


def surface_tiles(piece):
    """The tile for each column's surface, with grass ends where an edge
    is exposed."""
    out = []
    for i, (col, kind, left, right) in enumerate(piece):
        tile = SURFACE_TILE[kind]
        if kind == "flat":
            before = piece[i - 1][3] if i > 0 else None
            after = piece[i + 1][2] if i + 1 < len(piece) else None
            if before is None or before > left:
                tile = GRASS_LEFT
            elif after is None or after > right:
                tile = GRASS_RIGHT
        out.append((col, min(left, right) // T, tile))
    return out


def earth(col, row):
    return STONES if (col * 7 + row * 13) % 11 == 0 else EARTH


def tiles(c, cols):
    grid = [[0] * cols for _ in range(ROWS)]
    for piece in c.pieces:
        for col, row, tile in surface_tiles(piece):
            grid[row][col] = tile
            for r in range(row + 1, ROWS):
                grid[r][col] = earth(col, r)
    for col, n, top, bottom in c.ceilings:
        for i in range(n):
            grid[top][col + i] = GRASS_LEFT if i == 0 else GRASS_RIGHT if i == n - 1 else GRASS
            for r in range(top + 1, bottom):
                grid[r][col + i] = earth(col + i, r)
    for col, row, n in c.platforms:
        zone = "jungle"
        for start, z in c.zones:
            if start <= col:
                zone = z
        if zone == "treetops":
            # Boughs, drawn as props, carry these.
            continue
        for i in range(n):
            grid[row][col + i] = PLANK_LEFT if i == 0 else PLANK_RIGHT if i == n - 1 else PLANK
    for col, row, w, h in c.blocks:
        for r in range(row, row + h):
            for i in range(w):
                grid[r][col + i] = BLOCK
    for col, row, n in c.spikes:
        for i in range(n):
            grid[row - 1][col + i] = SPIKES
    # The walls at either end.
    for r in range(ROWS):
        for col in (0, 1, cols - 2, cols - 1):
            if grid[r][col] == 0:
                grid[r][col] = BLOCK
    return grid


def outline(piece):
    """The piece's surface from left to right, without points in the
    middle of straight runs."""
    points = []
    for col, _, left, right in piece:
        x = col * T
        for p in ((x, left), (x + T, right)):
            if points and points[-1] == p:
                continue
            points.append(p)
            if len(points) >= 3:
                (ax, ay), (bx, by), (cx, cy) = points[-3:]
                if (bx - ax) * (cy - ay) == (by - ay) * (cx - ax):
                    del points[-2]
    return points


def toml_points(points):
    return "[" + ", ".join(f"[{x}, {y}]" for x, y in points) + "]"


def num(v):
    return str(int(v)) if v == int(v) else str(v)


def props_layer(w, name, props, style):
    w("[[layers]]")
    w(f'name = "{name}"')
    if "depth" in style:
        w(f'depth = "{style["depth"]}"')
    w(f"scale = {SCALE}")
    if "opacity" in style:
        w(f"opacity = {style['opacity']}")
    if "sway" in style:
        amount, period = style["sway"]
        w(f"sway = {{ amount = {amount}, period = {period} }}")
    w("props = [")
    for kit, image, x, y, flip in props:
        if kit == "../backgrounds":
            path, normals = f"../backgrounds/{image}.png", None
        else:
            path, normals = f"../props/{kit}/{image}.png", f"../props/{kit}/{image}-normals.png"
        fields = [f'image = "{path}"']
        if normals:
            fields.append(f'normals = "{normals}"')
        fields.append(f"at = [{num(round(x * 4) / 4)}, {num(round(y * 4) / 4)}]")
        if flip:
            fields.append("flip = true")
        w("  { " + ", ".join(fields) + " },")
    w("]")
    w("")


def background(w, name, image, parallax, offset, opacity=None):
    w("[[layers]]")
    w(f'name = "{name}"')
    w(f'image = "../backgrounds/{image}.png"')
    w(f"scale = {SCALE}")
    w(f"parallax = [{parallax[0]}, {parallax[1]}]")
    if offset != (0, 0):
        w(f"offset = [{offset[0]}, {offset[1]}]")
    if opacity is not None:
        w(f"opacity = {opacity}")
    w("repeat = [true, false]")
    w("")


def write(c, path):
    cols = c.col + 2
    width, height = cols * T, ROWS * T
    grid = tiles(c, cols)
    props = Dresser(c).dress()
    # The tiles start at the first row with anything but the walls in it.
    top = next(r for r in range(ROWS) if any(grid[r][2:-2]))
    out = []
    w = out.append
    w("# clamber level v1")
    w("#")
    w("# The first level: the jungle floor, with steps, slopes, pits and a")
    w("# bunch hidden under a drop; a climb into the treetops and a rope")
    w("# across the canopy; a cave with a restart barrel at its mouth and a")
    w("# spring up out of it; then hills, a slope to roll down through")
    w("# crawlers, a tower, launchers, and the goal.")
    w("#")
    w("# Made by tools/levels/first.py, which lays out the collision and the")
    w("# terrain tiles together, and places the props along them. Edit that")
    w("# and run it again instead of editing this file.")
    w("format = 1")
    w('name = "First Steps"')
    w(f"size = [{width}, {height}]")
    w(f"start = [{6 * T + T // 2}, {c.pieces[0][0][2]}]")
    w("")
    w("[camera]")
    w("look_ahead = [32, 24]")
    w("")
    w("[music]")
    w('file = "../music/grasslands-theme.mp3"')
    w("")
    for x, y, colour, radius in c.lights:
        w("[[lights]]")
        w(f"at = [{x}, {y}]")
        w(f'color = "{colour}"')
        w(f"radius = {radius}")
        w("")
    background(w, "sky", "sky", (0.05, 0), (0, 0))
    background(w, "far jungle", "jungle-far", (0.15, 0), (0, 70))
    background(w, "light shafts", "light-shafts", (0.25, 0), (0, 0), opacity=0.6)
    background(w, "near jungle", "jungle-near", (0.4, 0), (0, 36))
    # Leaves overhead, over the near jungle's top edge.
    background(w, "canopy", "canopy", (0.45, 0), (0, -39))
    for name in BEHIND_TERRAIN:
        props_layer(w, name, props[name], LAYERS[name])
    w("[[layers]]")
    w('name = "terrain"')
    w(f"offset = [0, {top * T}]")
    w(f"scale = {SCALE}")
    w("")
    w("[layers.tiles]")
    w('tileset = "../tiles/terrain.png"')
    w('normals = "../tiles/terrain-normals.png"')
    w(f"tile_size = [{T * SCALE}, {T * SCALE}]")
    w(f"margin = {PAD}")
    w(f"spacing = {2 * PAD}")
    w(f"columns = {cols}")
    w("data = [")
    for row in grid[top:]:
        w("  " + ",".join(str(t) for t in row) + ",")
    w("]")
    w("")
    for name in LAYERS:
        if name not in BEHIND_TERRAIN:
            props_layer(w, name, props[name], LAYERS[name])
    w("# The walls at either end.")
    for x in (0, width - 2 * T):
        w("[[collision]]")
        w('shape = "rect"')
        w(f"rect = [{x}, 0, {2 * T}, {height}]")
        w("")
    w("# The ground, one piece between each pair of pits.")
    for piece in c.pieces:
        if not piece:
            continue
        points = outline(piece)
        (x0, _), (x1, _) = points[0], points[-1]
        points += [(x1, height), (x0, height)]
        w("[[collision]]")
        w('shape = "polygon"')
        w(f"points = {toml_points(points)}")
        w("")
    w("# Rock overhead.")
    for col, n, top_row, bottom in c.ceilings:
        w("[[collision]]")
        w('shape = "rect"')
        w(f"rect = [{col * T}, {top_row * T}, {n * T}, {(bottom - top_row) * T}]")
        w("")
    w("# One-way platforms.")
    for col, row, n in c.platforms:
        w("[[collision]]")
        w('shape = "segment"')
        w(f"from = [{col * T}, {row * T}]")
        w(f"to = [{(col + n) * T}, {row * T}]")
        w("one_way = true")
        w("")
    w("# Stone blocks.")
    for col, row, cw, ch in c.blocks:
        w("[[collision]]")
        w('shape = "rect"')
        w(f"rect = [{col * T}, {row * T}, {cw * T}, {ch * T}]")
        w("")
    w("# Spikes: a little lower than they're drawn, so grazing a tip is safe.")
    for col, row, n in c.spikes:
        w("[[collision]]")
        w('shape = "rect"')
        w(f"rect = [{col * T + 2}, {row * T - 10}, {n * T - 4}, 10]")
        w('surface = "hazard"')
        w("")
    for name, col, end in c.regions:
        w(f"# The {name}: the camera stays centred on it sideways.")
        w("[[regions]]")
        w(f'name = "{name}"')
        w(f"rect = [{col * T}, 0, {(end - col) * T}, {height}]")
        w(f"camera = {{ lock_x = {(col + end) * T // 2} }}")
        w("")
    for comment, kind, x, y, params in c.entities:
        if comment:
            w(f"# {comment}")
        w("[[entities]]")
        w(f'type = "{kind}"')
        w(f"at = [{x}, {y}]")
        if params:
            fields = ", ".join(f"{k} = {toml_value(v)}" for k, v in params.items())
            w(f"params = {{ {fields} }}")
        w("")
    with open(path, "w") as f:
        f.write("\n".join(out).rstrip() + "\n")


def toml_value(v):
    if isinstance(v, str):
        return f'"{v}"'
    if isinstance(v, list):
        return "[" + ", ".join(toml_value(x) for x in v) + "]"
    return str(v)


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "content/base/levels/first.level")
    write(build(), out)
