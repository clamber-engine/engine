# Playing Clamber

Clamber comes with one level so far, *First Steps*: runs and jumps, a
climb, a slope to roll down through enemies, a launcher and some islands
over a long drop, with a checkpoint halfway. Reach the goal at the end.
The music is the *Platformer Game Music Pack* by CodeManu (CC-BY 3.0);
**Credits** on the title screen has the details, and they're also in
`content/base/CREDITS.md`. The sound effects are placeholders.

Sound files can be Ogg Vorbis (`.ogg`), MP3, FLAC or WAV. Clamber's own
sound effects and the music of imported games are Ogg Vorbis.

## On Windows

CI builds the game for Windows from every push to `main`.

1. Go to the [CI runs on `main`](https://github.com/clamber-engine/engine/actions/workflows/firewall.yml?query=branch%3Amain+event%3Apush)
   and open the newest run with a green check.
2. Under **Artifacts** at the bottom of the run's page, download
   `clamber-windows`. You have to be signed in to GitHub to download
   artifacts, and each one expires 14 days after its run.
3. Unzip it somewhere. Keep `clamber-game.exe` and the `content` folder
   next to each other; the game finds its levels, art and sound there.
4. Double-click `clamber-game.exe`. The build isn't signed, so Windows
   SmartScreen may stop it the first time: choose **More info**, then
   **Run anyway**.

Warnings, for example when there's no sound device, go to a log file,
`%APPDATA%\Clamber\clamber.log`, which starts afresh each time the game
runs. If the game can't start, for example because the `content` folder
isn't next to it, a message box says why. To see warnings as they
happen, run `clamber-game.exe --console` from a Command Prompt in the
unzipped folder.

## Controls

The game starts at the title screen. Pick **Start** to play.

| | Keyboard | Gamepad |
|---|---|---|
| Move | Arrow keys | D-pad or left stick |
| Jump | Z or Space | Bottom face button (A on Xbox, Cross on PlayStation) |
| Action: run (hold) and roll | X | Left face button (X on Xbox, Square on PlayStation) |
| Swap characters | C | Select (View on Xbox, Share on PlayStation) |
| Tandem | V | Right face button (B on Xbox, Circle on PlayStation) |
| Pause | Escape | Start |
| Collision and region view | F1 | |
| Fullscreen | F11 or Alt+Enter | |

The gamepad layout is the classic titles' own. The top face button and
the shoulders do nothing.

Menus have controls of their own, which the controls screen doesn't
change. The bottom of each menu shows them, as keys or as the buttons of
the gamepad you used last:

| In menus | Keyboard | Gamepad |
|---|---|---|
| Move through the menu | Arrow keys | D-pad or left stick |
| Pick | Enter | Bottom face button |
| Back | Escape | Right face button, or Start |

Space does nothing in menus. Escape or Start closes the pause menu
again.

**Rolling** depends on the movement picked (below):

- **Classic 1:** press Action to roll, standing, walking or running.
  Holding Action runs, so to roll while running, let go of Action and
  press it again. Holding Down when you press Action starts the ground
  action instead, except with the second character, which rolls. Press
  Action just before
  landing and keep it held to roll as you land. A fast roll, or one
  without a direction held, ends with the climber getting back on their
  feet for a moment, and you can't move until they're up.
- **Default, Snappy and Floaty:** hold Down and Action together while
  moving. Keep a direction held and the roll carries on into a run
  (Action held) or a walk once it slows down; let go and it slows to a
  stop.

**Swap** and **Tandem** only matter with two characters, in Classic 1:
both swap the characters. Tandem only counts on the frame you press it,
and on the map it enters a level as Jump does.

Hold jump longer to jump higher. Land on an enemy, or roll into it, to
defeat it. Touching one any other way, or spikes, costs a hit point: in
Default, Snappy and Floaty a life has three, shown as pips next to the
gem count. A hit knocks you back, and you blink for a moment, safe from
another. A green tonic gives a hit point back. Losing the last one, or
falling into a pit, costs a life; in the Classic presets every hit does.
You have three lives. After losing one, you come back at the start, or
at the checkpoint once you've touched it, with all your hit points.

**Controls** on the title or pause screen remaps the keys and buttons:
Up and Down pick a row, Left and Right pick the player or the keyboard or
gamepad column, jump starts a remap, and pause goes back. Your bindings
are saved to `%APPDATA%\Clamber\input.toml` on Windows, and
`~/.config/clamber/input.toml` on Linux and
`~/Library/Application Support/Clamber/input.toml` on macOS. Delete that file to go back to
the defaults.

**Movement** on the title screen picks how the game feels: Default,
Snappy (quicker and heavier), Floaty (light, long jumps) or Classic 1
(the first classic title's movement). Picking one starts the level over
with it. Classic 2 and Classic 3 are listed but can't be picked yet. Your choice is saved to `prefs.toml` in the same folder
as `input.toml`, and the game starts with it next time.

**Options** on the title or pause screen turns fullscreen on and off,
sets how frames are synced to the display, and sets the music and
effects volumes. On a volume, Left turns it down and Right or Pick turns
it up, in steps of 10%; Pick at 100% goes back to silent. All of these,
and the window's size, are saved to `prefs.toml` too. The game pauses when its
window loses focus.

**Sync** on the options screen sets how frames wait for the display.
Left, and Right or Pick, go through the settings. Whichever you pick,
the game itself always moves in steps of a sixtieth of a second; only when frames are
shown changes, so it plays the same.

- **Auto**, where the game starts: VRR on a display that supports it,
  and VSync on others. The game can't yet tell whether a display
  supports VRR, so for now Auto is always VSync (#307); pick VRR yourself if
  yours does.
- **VSync**: each frame waits for the display's next refresh, so the
  picture never tears. On a 60, 120 or 240 Hz display every step is
  shown for the same number of refreshes; at other rates, such as
  144 Hz, frames are drawn between steps.
- **VRR**, for G-Sync, FreeSync and HDMI VRR displays: frames are shown
  as soon as they're drawn, without tearing, and the display refreshes
  as each one arrives. The game draws them just under the display's
  highest refresh rate (about 140 a second on a 144 Hz display), in
  between steps. If it can't tell the display's rate, or that's 60 Hz,
  it draws exactly one frame a step instead, 60 even frames a second.
  On a display without VRR this setting judders; use VSync there.
- **Off**: frames don't wait for the display and the picture may tear.
  The game draws up to 240 frames a second.

VRR only takes effect where the system lets it. On Windows, play
fullscreen (F11 or Alt+Enter), which uses borderless fullscreen with a
flip-model swap chain, and turn on G-Sync or FreeSync in the graphics
driver's settings (for G-Sync, also for windowed and borderless games
if you play in a window). On Linux, VRR needs a compositor that
supports it, with it turned on (for example KDE Plasma on Wayland, or
GNOME's variable refresh rate setting), and usually needs the game
fullscreen.

To check the pacing by eye, press F1 during play: under the collision
view, the bottom-left corner shows a graph of the time between the last
120 frames, a unit high for each millisecond, with a line at the time
they're meant to take. Green bars are on time, orange ones are off by
more than a tenth, and red ones are half as long again or more. Above
it are the frame rate and how far frame times spread from their mean.

**Widescreen** on the options screen sets how the game being played
shows a classic movement's view, which is narrower than a wide window.
Each game has its own setting; Left, and Right or Pick, go through the
modes:

- **Original activation**, where imported games start: the window shows
  more than the original view, and things are drawn as soon as they're
  in the window, but they only move and act once they're inside the
  original view, so play goes as it did there. Until then they stand
  where they'll start, and can't be touched. Things that go while
  outside the original view, as when it leaves them behind, fade out
  where they were, and fade in where they start if the window still
  shows that. The counters and lives stay over the original view's
  width, in the middle of the window.
- **Wide activation**, where Clamber's own game starts: things appear,
  act and are removed across the whole window, coming in from outside
  it, and balls and thrown things leave by its edges. Play goes
  differently from the original. At the level's ends the window stops
  at the level's edge, but you're still stopped where the original
  view would stop you. The counters and lives stay as in original
  activation.
- **Original framing**: only the original view's width is shown, with
  borders at the sides, and play goes as it did there.

In every mode, nothing appears out of nowhere inside the window. How
much the window shows is set by the game, not by the window's size:
resizing the window only scales it, so changing it mid-level changes
nothing in play. Changing the mode mid-level takes effect from the next
frame. Movement without a view of its own
(Default, Snappy, Floaty) fills the window in every mode.

**Rendering** on the options screen, shown while playing an imported
game, sets how its levels are drawn. Each imported game has its own
setting; Left, Right or Pick switch between the two modes:

- **Original**, where imported games start: the game view is drawn at
  the original game's resolution (or as wide as the widescreen mode
  makes it), then scaled up as one picture by the largest whole number
  that fits the window, with sharp pixels. Everything in the game view,
  including any of Clamber's own art standing in for the original's, is
  on that pixel grid, so it looks like the original.
- **Enhanced**: the game view is drawn at the window's full resolution
  and fills as much of it as it can. The original's art is scaled up
  with sharp pixels, while Clamber's own art is drawn in full detail.

Only drawing differs: the game plays exactly the same in both. Menus
are always drawn at the window's full resolution, including those over
the game view, and Clamber's own game is always drawn at full
resolution, so the setting isn't shown for it.

**Import a game…** on the title screen turns your own copy of a
supported game into a personal pack for Clamber to play. Choose the file
in the window that opens; it starts in the folder you imported from last
time. You can also drop the file onto Clamber's window, from any screen.
If you're using a gamepad, or the window doesn't open, Clamber shows its
own list of folders instead (press Back while the window is open to
switch to it): Up and Down to move, Pick to open a folder or pick the
file, Left to go up a folder, and Back to go back. It lists only files
the size of a supported game, and marks with a star the ones that are
one. On a keyboard, you can also type a path there or paste one with
Ctrl+V, then press Enter to open that folder or import that file
(Escape stops typing). A path that doesn't start at a drive or `/` is
taken from the folder shown. The pack goes into
`%LOCALAPPDATA%\Clamber\packs` on Windows,
`~/.local/share/clamber/packs` on Linux and
`~/Library/Application Support/Clamber/packs` on macOS. After each import, Clamber
writes a report on it to `import-report.txt` in that folder (the screen
shows where) and replaces the one from the import before. It has only
counts, steps and states, nothing from the game itself, so it's safe to
share. `clamber-import <file>` does the same from a command line, and
writes the report only if asked to, with `--import-report <file>`.
It writes the music as Ogg Vorbis at quality 6, which
`--audio-quality <q>` changes (from -1, smallest, to 10, best). A personal import is yours alone: it's
marked as one wherever it's shown, and Clamber won't export it or
capture it for sharing. This version recognizes the one game image
it supports (with or without a copier header) and converts its first
level's ground and background, where its objects are, and its music
(without the echo the game adds, which Clamber can't make yet). Most of the
objects don't appear or act yet, you start near the level's left end,
and there's no way to finish it yet. For any other file, it says it
doesn't look like a supported game image, and why.

**Games** on the title screen lists Clamber's own game and the games
you've imported. Pick one to play it from the title screen.

## On Linux, from source

You need a current stable Rust ([rustup](https://rustup.rs)) and, for sound
and gamepads, the ALSA and udev development packages. On Debian or
Ubuntu:

```
sudo apt install libasound2-dev libudev-dev
git clone https://github.com/clamber-engine/engine.git
cd engine
cargo run --release -p clamber-game --features audio,gamepad
```

The `spec` submodule is only needed for the tests, not to play. Without
`--features audio,gamepad` the game builds without those packages, and
plays silently with the keyboard only.

`cargo run --release -p clamber-game -- --help` lists the other options:
playing another level file, choosing a movement preset (which overrides
the one picked in the game) or a game pack, and starting with the
collision view on.

Warnings and errors show in the terminal, and are also written to
`~/.config/clamber/clamber.log`.

## On macOS, from source

You need a current stable Rust ([rustup](https://rustup.rs)) and the
Xcode command line tools (`xcode-select --install`). Sound and gamepads
need nothing else, so the game builds with both on Apple silicon:

```
git clone https://github.com/clamber-engine/engine.git
cd engine
cargo run --release -p clamber-game --features audio,gamepad
```

Run it from the checkout, so it finds the `content` folder. As on Linux,
the `spec` submodule is only needed for the tests, and
`cargo run --release -p clamber-game -- --help` lists the other options.

Warnings and errors show in the terminal, and are also written to
`~/Library/Application Support/Clamber/clamber.log`, along with
`input.toml` and `prefs.toml`.
