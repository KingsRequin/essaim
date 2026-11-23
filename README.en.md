<div align="center">

<img src="docs/images/logo.svg" width="96" alt="Essaim logo">

# Essaim

**Comets and a swarm of satellites dancing around your mouse pointer, to the beat of the
music.**

An overlay for Windows that listens to what your PC plays (Spotify, browser, games…): every kick
reverses the rotation and releases satellites, the bass speeds up and widens the orbits, cymbals
throw sparks, and on the drop, everything bursts.

[![MIT license](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)
![Windows 11](https://img.shields.io/badge/Windows-11-0078D4?style=flat-square&logo=windows)
![Rust](https://img.shields.io/badge/Rust-Direct2D-orange?style=flat-square&logo=rust)

[**Install**](#install) ·
[Features](#features) ·
[FAQ](#faq) ·
[Full guide (French)](docs/guide.md) ·
[Français](README.md)

<img src="docs/images/hero.gif" alt="Four Essaim presets around the pointer: circle, triangle, hexagon and figure eight, pulsing on the kicks" width="100%">

</div>

*Essaim* is French for *swarm*. The app and its settings window are in French.

## Why Essaim?

- **It really listens.** Kicks are recognized by a small model trained on real tracks in
  42 styles (from techno to jazz, rap, rock, reggae, salsa…), about 25 ms after they hit and at any
  volume: the swarm lands on the beat, not after it. It also hears the snare, the build-up before a drop, the tempo and which side is
  playing. Nothing to plug in: Essaim listens to what Windows plays.
- **Your pointer stays yours.** It is the real Windows cursor, with no lag; Essaim only draws
  around it, in a click-through layer.
- **Light.** Drawn by the GPU: 0.31 ms per frame on the benchmark, about 3% of one CPU core in
  use. It goes to sleep when a game or video goes fullscreen.
- **Your way.** 14 path shapes, the outlines of an image, a text, an emoji or the icon of the
  app in front; 41 bundled presets, animated live;
  triggers ("on the drop, a crack", "on the snare, sparks") and modulations ("the rotation spins
  up on the drop", "the orbit opens on low notes") to build your own swarm; "if the artist is…
  then this preset" rules; every setting applied instantly, with its cost shown.

## Features

<table>
<tr>
<td width="33%" valign="top"><img src="docs/images/kick.gif" alt="Intense preset pulsing on the kicks"></td>
<td width="33%" valign="top"><img src="docs/images/drop.gif" alt="Festival preset: the swarm tightens during a build-up, then bursts on the drop"></td>
<td width="33%" valign="top"><img src="docs/images/clic.gif" alt="Shockwave and shards on click"></td>
</tr>
<tr>
<td valign="top"><b>It follows the music</b>: the kick reverses the rotation and adds satellites,
the bass speeds it up, a low note widens the orbits, the snare opens a square.</td>
<td valign="top"><b>Build-up and drop</b>: during the build-up the swarm tightens and speeds up;
on the drop it bursts into a big wave and a spray of sparks.</td>
<td valign="top"><b>It reacts to clicks</b>: shockwave, shards and cracks that close again; on
right-click, a wave that closes in.</td>
</tr>
<tr>
<td valign="top"><img src="docs/images/trace.gif" alt="Comets on the four outlines of a smiling face: the face, two eyes, the mouth"></td>
<td valign="top"><img src="docs/images/constellation.gif" alt="Constellation preset: satellites linked by thin lines, two-hue palette"></td>
<td valign="top"><img src="docs/images/stream.png" alt="Stream window: the swarm right after a drop, on a transparent background (checkerboard)"></td>
</tr>
<tr>
<td valign="top"><b>The path of an image</b>: a logo, a text, an emoji (or drop an image on the
window); each ring of comets follows one of its outlines. Or 14 shapes, which can spin on
themselves.</td>
<td valign="top"><b>Constellation and palette</b>: thin lines link nearby satellites; comets and
satellites share up to three hues.</td>
<td valign="top"><b>Stream window</b>: the swarm in the middle, on a transparent background for
OBS (or a solid one); it follows your music like the swarm around the pointer.</td>
</tr>
</table>

### The music

| | |
|---|---|
| 🥁 **Kick** | Reverses the rotation, adds satellites, flash under the pointer, lights up the border |
| 🔊 **Bass** | Speeds up the rotation; the lower the note, the wider the orbits (two default modulations, to tune or remove) |
| ✨ **Highs** | Hi-hats and cymbals throw sparks |
| 🟦 **Snare** | A glowing square opens under the pointer on every snare or clap |
| 🚀 **Build-up and drop** | The swarm tightens and speeds up during the build-up, then bursts on the drop |
| ⏱️ **Tempo** | The swarm keeps pulsing through breaks without a kick drum; optionally, the heads pulse on the off-beats |
| 🎧 **Stereo** | Optionally, the swarm leans towards the louder side |
| ⚡ **Triggers** | When an event happens (kick, snare, hi-hat, drop, beat, off-beat, break, track change, click, right click), an action: flash, shockwave, shards, sparks, square, crack, implosion, satellites, explosion, burst, reversed rotation, border lighting up; with a minimum delay and a chance ("one kick in three") |
| 🎛️ **Modulations** | A part of the sound (bass, low or high note, kick, snare, hi-hat, build-up, drop, off-beat, tempo, brightness, stereo) drives the path rotation, speed, acceleration, radius, size, hue, offset, glow, trail or opacity, of the comets, the satellites or both; with a threshold, smoothing, and inverted if you like |
| 🎵 **Now playing** | Album-art colour, a burst on track change, title in the tray tooltip and settings |

### The mouse and the screen

| | |
|---|---|
| 🖱️ **Clicks** | Shockwave, shards, cracks; on right-click, a wave that closes in |
| 💨 **Movement** | A dust trail when the mouse moves fast; optionally, the swarm lags behind it like on a spring |
| 🪟 **Active window** | Its border takes the swarm's color and lights up on every kick; sparks bounce off its edges |
| 🎨 **Colors** | Fixed or cycling hue, a palette of two or three hues, color of the foreground app, of the website open, of the album art, or drifting with the music |
| 🔦 **Find the pointer** | Shake the mouse (or press <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>F</kbd>) to dim the screen around the pointer (adjustable shake sensitivity, size and dimming); optional crosshair and spectrum ring (which fades out when the music stops) |

### On its own, and out of the way

| | |
|---|---|
| 🤖 **If … then …** | Rules pick the preset: `artiste = Skrillex` → *Delta*, `bpm > 140 et heure >= 22:00` → *Intense*…; or by foreground app, or time of day |
| 🌙 **Stays out of the way** | Pauses for full-screen apps, presentations, on battery or in front of chosen apps; lowers its frame rate when the PC struggles; can be hidden from screenshots and streams |
| 🫶 **No flashes** | For people sensitive to flashing: softer flashes, three per second at most, a border that no longer blinks (automatic when Windows reduces animations) |
| 📺 **Stream window** | The swarm in the middle of a window with a transparent background, to add in OBS as a *Game Capture* |
| 🌀 **[Horizon](https://github.com/KingsRequin/horizon)** | A Horizon plugin pauses Essaim, switches presets and opens its settings from the bar |

**Also:** global hotkeys (<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>→</kbd> next preset,
<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>P</kbd> pause), presets in the tray menu, GIF export of the
preview, *Surprends-moi* (random settings), light or dark theme like Windows, sleep while you're
away, automatic updates (downloaded, verified, applied on restart). The **Dotter** cursor scheme
(a simple dot) is included.

## The settings window

**Presets.** A grid of 41 bundled presets, all different, each with its own reactions to the music (zen, neon, storm, disco, ghost, vortex, album art…), each animated live with
your music: one click applies it. Save your own, compare two side by side, filter by shape.

<img src="docs/images/presets.png" alt="Presets tab: the bundled presets, animated" width="100%">

<table>
<tr>
<td width="50%" valign="top"><img src="docs/images/reglages.png" alt="Settings tab: large preview, one sub-tab per effect, here the modulations"></td>
<td width="50%" valign="top"><img src="docs/images/curseurs.png" alt="Cursors tab: installed cursor schemes"></td>
</tr>
<tr>
<td valign="top"><b>Settings</b> sorted by family (the swarm, the effects, the reactions,
listening), each with its basic and advanced options, applied live on a large preview that
follows your music; one button simulates a build-up and a drop, another tests each trigger. Next
to each setting, ten squares tell its power cost.</td>
<td valign="top"><b>Cursors</b>: your Windows cursor schemes, animated, applied in one click. A
preset can carry its own.</td>
</tr>
</table>

**Auto**: the rules that pick the preset by track, tempo, app or time of day, and the hotkeys.

**Application** (the gear, right of the tabs): the installed version, checking for an update,
starting with Windows, pauses, captures and streaming.

## Install

**Easiest:** download `essaim.exe` from the
[latest release](https://github.com/KingsRequin/essaim/releases/latest) and run it. Nothing to
install, no administrator rights: the settings window opens on first launch.

- First put the file where it will stay (for example `%LOCALAPPDATA%\Programs\Essaim`), then
  tick *Démarrer Essaim avec Windows* in the *Application* view (the gear, top right).
- Windows may show « Windows protected your PC »: the executable isn't signed. *More info* →
  *Run anyway*.
- The **Dotter** cursor sits next to it: `curseur-Dotter.zip` (see step 3 below).
- Essaim then updates itself: a notification tells you about the download, then offers to
  restart. To skip the wait: *Chercher une mise à jour* in the *Application* view.

**From source:** Essaim builds with one command.

1. Install [Rust](https://rustup.rs) and [Git](https://git-scm.com).
2. In PowerShell:
   ```powershell
   git clone https://github.com/KingsRequin/essaim
   cd essaim
   powershell -ExecutionPolicy Bypass -File scripts\install.ps1
   ```
   Essaim is built, copied to `%LOCALAPPDATA%\Programs\Essaim`, starts with Windows and shows up
   in the Start menu (« Essaim » and « Essaim - réglages »). No administrator rights needed.
3. *(Recommended)* Install the **Dotter** cursor: right-click
   `curseurs\Dotter\_install.inf` → *Install*, then pick it in the *03 Curseurs* tab.

Then: Essaim's tray icon → *Réglages…*

Uninstall: `scripts\uninstall.ps1` (your settings are kept).

## FAQ

<details>
<summary><b>Does it slow down my PC or my games?</b></summary>

With the GPU, a frame costs 0.31 ms on the benchmark; in use, about 3% of one CPU core. When a
window goes fullscreen (game, video), everything stops; at rest (mouse still, no music), the
frame rate drops. The settings window shows the power in use, measured live.
</details>

<details>
<summary><b>Does Essaim replace my cursor?</b></summary>

No. The pointer stays a real Windows cursor (otherwise it would lag): Essaim only draws around
it. It changes the cursor scheme only when you ask (*Curseurs* tab, or a preset applied with the
option checked).
</details>

<details>
<summary><b>Does Essaim watch my keyboard or my browsing?</b></summary>

No. No keyboard or mouse hook: clicks are read once per frame, and hotkeys go through
`RegisterHotKey` (Windows notifies Essaim of those keys only). The website color comes from a
thin strip at the top of the page on screen; the window title is only used to notice a page
change. The address is not read, and nothing is logged or sent: Essaim only connects to GitHub,
for the preset library and updates.
</details>

<details>
<summary><b>The swarm doesn't react to sound.</b></summary>

Essaim listens to Windows' *default* audio output. If your music plays on another output, pick it
in *Réglages › Écoute › Avancé › Sortie son écoutée*. The header of the settings window shows what Essaim hears (kick, highs, bass, note).
</details>

<details>
<summary><b>The flashes bother me (photosensitivity).</b></summary>

*Réglages › Cet ordinateur › Sans flashs*: *Toujours*. Kick flashes are softened and limited to three per
second, with no white halo, and the window border no longer blinks. By default (*Comme Windows*),
this mode turns on by itself when animation effects are off in Windows (*Accessibility › Visual
effects*).
</details>

<details>
<summary><b>How do I show the swarm in OBS?</b></summary>

Tray icon → *Fenêtre pour le stream* (or `essaim.exe --stream`). The swarm dances in the middle,
at a size that always fits in the window. In OBS: *Game Capture* in *Capture specific window* mode on
« Essaim — stream », with *Allow Transparency* and *Premultiplied Alpha* checked: the background is
transparent, glows included. Otherwise, a solid background (selectable in the *Application*
view) to remove with a *Chroma Key* on a *Window Capture*. It stays capturable even when the
swarm around the pointer is hidden from captures.
</details>

<details>
<summary><b>How do I switch presets by artist or tempo?</b></summary>

*04 Auto* tab, *Si … alors …* section: write a condition (`artiste = Skrillex`, `titre ~ remix`,
`bpm > 140`, `appli = obs64.exe`, `heure >= 22:00`, or several joined by « et ») and pick the
preset. The *Artiste en cours* button fills in the condition for the track that is playing. Tick
*Choisir le preset automatiquement*: when no rule applies anymore, your previous settings come
back.
</details>

<details>
<summary><b>What about Windows 10?</b></summary>

Essaim is developed and tested on Windows 11. The colored border needs it; the rest should work on
Windows 10 (with WebView2, installed with Edge), but hasn't been tried there.
</details>

<details>
<summary><b>Where are my settings?</b></summary>

In `%APPDATA%\Essaim`: `config.toml` (commented, editable by hand), your presets in `profils\`,
and the log `essaim.log`.
</details>

## Roadmap

- **Shared preset library** on GitHub: publish yours, install others'.
- **More dot cursors**, designed to go with the swarm.

## Made with Claude Code

Essaim was written entirely with [Claude Code](https://claude.com/claude-code), Anthropic's coding
agent. [KingsRequin](https://github.com/KingsRequin) designed it and supervised all the work:
every feature was decided, reviewed and tried for real on a four-monitor PC before being kept.

## Contributing

Bugs, ideas and questions: a [form to fill in](https://github.com/KingsRequin/essaim/issues/new/choose)
(your version is shown next to the title of the settings window). Internals, building and audio analysis (French): [docs/technique.md](docs/technique.md).

## License

[MIT](LICENSE). The Archivo font and the Dotter cursors come under their own licenses: see
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
