# Replacement art packs

A replacement art pack gives an imported game new pictures: sprites,
tiles and backgrounds drawn at a higher resolution. Clamber draws them
in place of the original's pictures when the game's **Rendering** option
is set to Enhanced (see [`PLAYING.md`](PLAYING.md)); in Original mode the
game is drawn as it was. Anything a pack has no picture for is drawn as
the original.

A pack holds only new art, never anything from the game, so it works
with anyone's copy of the game and can be shared.

## Where packs go

Each pack is a folder in the `art` folder next to the imported packs:

- Windows: `%LOCALAPPDATA%\Clamber\art`
- Linux: `~/.local/share/clamber/art`
- macOS: `~/Library/Application Support/Clamber/art`

All the packs there are used, for every imported game. Where two packs
have a picture for the same original, the one whose folder name sorts
first wins, so names like `1-characters` and `2-everything` set the
order. Folders starting with `.` are skipped. Clamber reads the packs
each time a level starts, so a change shows when you next enter one.

## What's in a pack

```
art/
  1-my-pack/
    art.toml
    pictures/
      3f9c…e1.png
      characters/
        77ab…0c.png
        77ab…0c.other.png
```

`art.toml` says what the pack is:

```toml
format = 1
name = "My pack"
```

`pictures` holds the pictures, in any subfolders you like. Each is a PNG
(with transparency if it needs it) named after the **picture hash** of
the original it replaces, 64 hex digits, then `.png`. Other files are
ignored.

A replacement is drawn in its original's place, at its original's size,
so it has to be a whole number of times the original's size (from 1 to
16 times), the same number across and down: a 16 × 24 picture at four
times is 64 × 96. Pictures in one sheet or tileset can use different
multiples; they're all scaled to one, sharply. A replacement of the wrong
size is skipped, with a warning.

## Picture hashes

Each kind of picture has its own hash:

- **Sprite frames** use the picture hash of `spec/formats/sprites.md`
  ("Picture hashes"): the SHA-256 of the frame's box (the smallest
  rectangle holding every opaque pixel, as its left and top offsets from
  the frame's origin, each a signed 16-bit number, then its width and
  height, each an unsigned 16-bit number, all low byte first), then the
  colour number of each pixel of the box (0 for transparent), row by row.
  Colours don't count, so a frame's replacement is used in every palette
  it's drawn in. To give one palette its own picture, add
  `<hash>.<palette>.png` next to `<hash>.png`, with the palette's name
  from the sheet. The replacement covers the box: the frame trimmed to
  its opaque pixels.
- **Tiles** of a level's tile layers, and **whole images** a level's
  layers draw (backgrounds and props), use Clamber's own hash of their
  colours: the SHA-256 of the picture's width and height, each an
  unsigned 32-bit number low byte first, then each pixel's red, green,
  blue and alpha bytes, row by row from the top and left to right, with
  a fully transparent pixel as four 0 bytes. Colours count here, so the
  same tile in other colours is another picture.

You don't need to work these out yourself.

## Templates

`clamber-import --dump-pictures <folder> <your game file>` imports the
game as usual, then writes every picture of the imported pack into
`<folder>`, each already named after its hash, at its original size:
paint over a copy at the size you want, keep its name, and put it in a
pack's `pictures` folder. `pictures.tsv` there lists each picture's hash,
kind (`frame`, `tile` or `image`), size and where it's from (the sheet
and animation, or the level and layer, with the tile's number). Add
`--replace` if you've imported the game before.

The dumped pictures come from your copy of the game: keep them to
yourself, and only share the pictures you've made.
