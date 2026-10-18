#!/usr/bin/env python3
"""Builds content/base/levels/first.level, the first level.

The level is laid out here on a 16 px grid, left to right, and this script
writes its collision and its terrain tiles together, so the two always
match. Edit the layout below, then run:

    python3 tools/levels/first.py

Rows count down from the top of the level; a ground height is the row
whose top edge is the ground's surface. Tile numbers are terrain.png's
(see art/terrain.py), plus one.
"""

import os
import sys

T = 16
ROWS = 60
# Texels per world unit in the art (see art/), and around each tile in
# terrain.png.
SCALE = 5
PAD = 4
BASE = 46

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
        self.spikes = []  # (col, row, cols): spikes standing on `row`
        self.entities = []  # (comment, type, x, y, params)
        self.regions = []  # (name, first col, end col): camera locked sideways

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

    def cliff(self, rows):
        """A sheer drop of `rows`, or a wall when negative."""
        self.row += rows

    def pit(self, cols, rows=0):
        """A bottomless gap; the ground beyond is `rows` lower."""
        self.col += cols
        self.row += rows
        self.pieces.append([])

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

    def spike(self, col, cols):
        self.spikes.append((col, self.ground(col) // T, cols))

    def thing(self, kind, col, rows_up=0, params=None, comment=None):
        """An entity over column `col`, `rows_up` above the ground (at its
        feet for walkers, checkpoints and the goal; its centre otherwise)."""
        x = col * T + T // 2
        y = self.ground(col) - rows_up * T
        self.entities.append((comment, kind, x, y, params))

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

    # Start: a short run, a ledge to hop up, and a crawler to meet.
    c.flat(30)
    c.gems(10, 5, comment="Start: a run, a ledge to hop up, a crawler to meet.")
    c.cliff(-2)
    c.flat(14)
    c.gems(c.col - 12, 4)
    c.cliff(2)
    c.flat(16)
    c.thing("crawler", c.col - 4)
    c.flat(8)

    # A gentle hill up and a steep one down, then two pits to jump.
    c.up(3, gentle=True)
    c.flat(8)
    c.gems(c.col - 6, 3)
    c.down(3)
    c.flat(10)
    c.pit(5)
    c.flat(10)
    c.arc(c.col - 2, 5, 2, comment="Gems over the second pit.")
    c.pit(7)
    c.flat(14)

    # A bigger hill with spikes in a dip on top.
    c.up(4)
    c.flat(6)
    c.down(2, gentle=True)
    c.flat(6)
    c.spike(c.col - 4, 2)
    c.up(2, gentle=True)
    c.flat(6)
    c.thing("crawler", c.col - 3)
    c.down(4)
    c.flat(8)

    # A climb up one-way platforms to a plateau; a relic waits up high.
    climb = c.col
    c.flat(18)
    c.platform(climb + 3, 3, 4)
    c.platform(climb + 9, 6, 4)
    c.platform(climb + 3, 9, 4)
    c.thing("relic", climb + 4, 11, comment="The first relic, above the climb.")
    c.platform(climb + 13, 9, 3)
    c.gems(climb + 9, 2, rows_up=7)
    c.cliff(-9)
    c.flat(12)
    c.spike(c.col - 6, 2)
    c.gems(c.col - 11, 3)
    c.pit(6)
    c.thing("flutter", c.col - 3, 1, comment="A flutter rising out of the gap.")
    c.flat(14)
    c.thing("crawler", c.col - 5)
    c.flat(6)
    c.thing("checkpoint", c.col - 3, comment="The first checkpoint, before the shaft.")
    c.flat(4)

    # A shaft to climb on platforms, left and right in turn, while the
    # camera holds still sideways. A flutter crosses it halfway up.
    shaft = c.col
    c.flat(20)
    for i in range(6):
        left = i % 2 == 0
        c.platform(shaft + (2 if left else 12), 3 * (i + 1), 6)
        c.gems(shaft + (3 if left else 13), 2, rows_up=3 * (i + 1) + 1, step=3)
    c.thing("flutter", shaft + 4, 10, params={"path": [160, 0], "period": 200})
    c.region("shaft", shaft, c.col)
    c.cliff(-20)

    # Along the ridge at the top, over gaps.
    c.flat(12)
    c.pit(5)
    c.flat(10)
    c.thing("crawler", c.col - 3)
    c.pit(6)
    c.thing("flutter", c.col - 3, 2, params={"path": [0, -32], "period": 120})
    c.flat(14)
    c.gems(c.col - 12, 5)
    c.pit(4, 1)
    c.flat(8)

    # The long slope down to a row of crawlers: roll through them.
    c.down(10, gentle=True)
    c.gems(c.col - 18, 4, rows_up=2, step=4, comment="Roll down the slope and through the crawlers.")
    c.flat(40)
    for i in range(4):
        c.thing("crawler", c.col - 34 + i * 7)
    c.thing("tonic", c.col - 10, 1, comment="A tonic past the crawlers, for a hit taken on the way.")
    c.gems(c.col - 8, 4)
    c.down(4)
    c.flat(6)
    c.down(4)
    c.flat(6)
    c.down(4, gentle=True)
    c.flat(8)
    c.thing("crawler", c.col - 4)
    c.flat(8)

    # A launcher in front of a wall too high to jump.
    c.thing("launcher", c.col - 3, comment="The launcher: the only way up the wall.")
    c.cliff(-13)
    c.flat(20)
    c.gems(c.col - 18, 6)
    c.thing("relic", c.col - 4, 5, comment="The second relic, high above the wall.")
    c.platform(c.col - 6, 3, 4)
    c.thing("crawler", c.col - 8)

    # Islands over a long drop, with flutters guarding the gaps.
    c.pit(5, 4)
    c.flat(6)
    c.pit(6, -1)
    c.thing("flutter", c.col - 3, 1, params={"period": 150})
    c.flat(5)
    c.pit(6, 2)
    c.thing("flutter", c.col - 3, 2, params={"path": [0, -48]})
    c.flat(8)
    c.gems(c.col - 7, 3)

    # Down a steep run of steps to the second checkpoint.
    c.down(3)
    c.flat(4)
    c.down(3)
    c.flat(10)
    c.thing("checkpoint", c.col - 4, comment="The second checkpoint.")
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
    c.thing("tonic", c.col - 5, 1, comment="A tonic after the hills.")

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

    # Platforms across a long drop, with flutters rising and falling
    # between them.
    span = c.col
    c.pit(34)
    for i in range(4):
        c.platform(span + 2 + i * 8, 1 + (i % 2) * 2, 4)
        c.gems(span + 3 + i * 8, 2, rows_up=3 + (i % 2) * 2)
    for i in range(3):
        c.thing("flutter", span + 7 + i * 8, 0, params={"path": [0, -80], "period": 120 + 30 * i})
    c.flat(10)

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

    # A second launcher, up to a high shelf with the last relic.
    lift = c.col
    c.flat(30)
    c.thing("launcher", lift + 3, comment="A launcher up to the last relic.")
    c.platform(lift + 1, 12, 8)
    c.thing("relic", lift + 6, 13)
    c.gems(lift + 2, 3, rows_up=13, step=2)
    c.thing("crawler", lift + 16)
    c.thing("crawler", lift + 24, params={"facing": "right"})

    # The last stretch: a pit with a platform over it and a final rise.
    c.pit(8)
    c.platform(c.col - 6, 2, 4)
    c.flat(12)
    c.thing("crawler", c.col - 4)
    c.up(2)
    c.flat(4)
    c.up(2)
    c.flat(26)
    c.gems(c.col - 24, 5, step=3)
    c.thing("goal", c.col - 6, comment="The goal.")
    c.flat(6)
    return c


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
    for col, row, n in c.platforms:
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


def write(c, path):
    cols = c.col + 2
    width, height = cols * T, ROWS * T
    grid = tiles(c, cols)
    # The tiles start at the first row with anything but the walls in it.
    top = next(r for r in range(ROWS) if any(grid[r][2:-2]))
    out = []
    w = out.append
    w("# clamber level v1")
    w("#")
    w("# The first level: runs and jumps, hills, pits, a climb up one-way")
    w("# platforms, a slope to roll down through crawlers, a launcher, and")
    w("# islands over a long drop, with a checkpoint halfway.")
    w("#")
    w("# Made by tools/levels/first.py, which lays out the collision and the")
    w("# terrain tiles together. Edit that and run it again instead of")
    w("# editing this file.")
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
    w("[[layers]]")
    w('name = "sky"')
    w('image = "../backgrounds/sky.png"')
    w(f"scale = {SCALE}")
    w("parallax = [0.05, 0]")
    w("repeat = [true, false]")
    w("")
    w("[[layers]]")
    w('name = "mountains"')
    w('image = "../backgrounds/mountains.png"')
    w(f"scale = {SCALE}")
    w("parallax = [0.2, 0]")
    w("offset = [0, 56]")
    w("repeat = [true, false]")
    w("")
    w("[[layers]]")
    w('name = "hills"')
    w('image = "../backgrounds/hills.png"')
    w(f"scale = {SCALE}")
    w("parallax = [0.4, 0]")
    w("offset = [0, 96]")
    w("repeat = [true, false]")
    w("")
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
        return "[" + ", ".join(str(x) for x in v) + "]"
    return str(v)


if __name__ == "__main__":
    root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(root, "content/base/levels/first.level")
    write(build(), out)
