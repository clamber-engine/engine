# Playing Clamber

Clamber comes with one level so far, *First Steps*: runs and jumps, a
climb, a slope to roll down through enemies, a launcher and some islands
over a long drop, with a checkpoint halfway. Reach the goal at the end.
The music is the *Platformer Game Music Pack* by CodeManu (CC-BY 3.0);
**Credits** on the title screen has the details, and they're also in
`content/base/CREDITS.md`. The sound effects are placeholders.

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
| Jump; pick in menus | Z or Space | Bottom face button (A on Xbox, Cross on PlayStation) |
| Action: run (hold) and roll | X | Left face button (X on Xbox, Square on PlayStation) |
| Swap characters | C | Select (View on Xbox, Share on PlayStation) |
| Tandem | V | Right face button (B on Xbox, Circle on PlayStation) |
| Pause; back in menus | Enter or Escape | Start |
| Collision and region view | F1 | |
| Fullscreen | F11 or Alt+Enter | |

The gamepad layout is the classic titles' own. The top face button and
the shoulders do nothing.

**Rolling** depends on the movement picked (below):

- **Classic 1:** press Action to roll, standing, walking or running.
  Holding Action runs, so to roll while running, let go of Action and
  press it again. Holding Down when you press Action starts the ground
  action instead, except with the second character, which rolls. Press
  Action just before
  landing and keep it held to roll as you land.
- **Default, Snappy and Floaty:** hold Down and Action together while
  moving. Keep a direction held and the roll carries on into a run
  (Action held) or a walk once it slows down; let go and it slows to a
  stop.

**Swap** and **Tandem** only matter with two characters, in Classic 1:
both swap the characters. Tandem only counts on the frame you press it,
and on the map it enters a level as Jump does.

Hold jump longer to jump higher. Land on an enemy, or roll into it, to
defeat it; touching one any other way, or falling into a pit, costs a
life. You have three. After losing one, you come back at the start, or
at the checkpoint once you've touched it.

**Controls** on the title or pause screen remaps the keys and buttons:
Up and Down pick a row, Left and Right pick the player or the keyboard or
gamepad column, jump starts a remap, and pause goes back. Your bindings
are saved to `%APPDATA%\Clamber\input.toml` on Windows, and
`~/.config/clamber/input.toml` on Linux. Delete that file to go back to
the defaults.

**Movement** on the title screen picks how the game feels: Default,
Snappy (quicker and heavier), Floaty (light, long jumps) or Classic 1
(the first classic title's movement). Picking one starts the level over
with it. Classic 2 and Classic 3 are listed but can't be picked yet. Your choice is saved to `prefs.toml` in the same folder
as `input.toml`, and the game starts with it next time.

**Options** on the title or pause screen turns fullscreen and vsync on
and off, and sets the music and effects volumes. Vsync is on to start
with, so the picture doesn't tear; with it off, the game draws up to 240
frames a second. On a volume, Left turns it down and Right or Jump turns
it up, in steps of 10%; Jump at 100% goes back to silent. All of these,
and the window's size, are saved to `prefs.toml` too. The game pauses when its
window loses focus.

**Widescreen** on the options screen sets how the game being played
shows a classic movement's view, which is narrower than a wide window.
Each game has its own setting; Left, and Right or Jump, go through the
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

**Import a game…** on the title screen turns your own copy of a
supported game into a personal pack for Clamber to play. Find the file
with Up and Down, Jump to open a folder or pick the file, Left to go up a
folder, and pause to go back. The pack goes into
`%LOCALAPPDATA%\Clamber\packs` on Windows, and
`~/.local/share/clamber/packs` on Linux; `clamber-import <file>` does
the same from a command line. A personal import is yours alone: it's
marked as one wherever it's shown, and Clamber won't export it or
capture it for sharing. This version recognizes the one game image
it supports (with or without a copier header) and converts its first
level's ground and background, where its objects are, and its music
(without the echo the game adds, which Clamber can't make yet). Most of the
objects don't appear or act yet, you start near the level's left end,
and there's no way to finish it yet. For any other file, it says it can't
import it, and why.

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
