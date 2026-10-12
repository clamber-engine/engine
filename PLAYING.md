# Playing Clamber

Clamber comes with one level so far, *First Steps*: runs and jumps, a
climb, a slope to roll down through enemies, a launcher and some islands
over a long drop, with a checkpoint halfway. Reach the goal at the end.
All the art and sound are placeholders.

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
| Run (hold) | X | Left or right face button |
| Roll | Down and X together while running | Down and run together while running |
| Pause; back in menus | Enter or Escape | Start |
| Collision and region view | F1 | |
| Fullscreen | F11 | |

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
playing another level file, choosing a physics preset or a game pack, and
starting with the collision view on.

Warnings and errors show in the terminal, and are also written to
`~/.config/clamber/clamber.log`.
