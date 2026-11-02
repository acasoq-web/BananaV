# Testing BananaV

How a change is checked: the checks every change passes, the screen
captures that show the interface, the media oracles, the comparisons
with the user's prints and the graphics card's parity tests.

- `cargo xtask ci` must stay green: rustfmt, layer rules, clippy with
  `-D warnings`, all tests and the wasm build of the portable crates.
- UI is verified **headlessly**, never by driving the user's mouse or
  keyboard:
  - `cargo test -p bv-ui-egui --test telas -- --ignored` → English captures
    in `target/telas/*.png` (`--test telas_inicio`: the main window's
    docked windows; `--test telas_menus`: the shortcut menus, the
    right-drag's menu, the preview zoomed with the wheel, stretched and
    out-of-sync events, the selection's colors, the track meters and
    their ranges, and the welcome screen). The user's settings may open
    the welcome screen over the window: `welcome_for_review("closed")`
    keeps it shut;
  - `cargo test -p bv-ui-egui --test telas_pt_br -- --ignored` → pt-BR
    captures in `target/telas/pt_br_*.png`.
  Tests that open dialogs must not press OK/Apply on settings windows: that
  would write the user's real settings files. In the Render window that
  means no Render…, Cancel, favorites, Customize OK or Save Template.
  Screen tests change settings only through the app's `*_for_review`
  hooks, on settings that are never saved (`UiSettings::in_memory`).
- The tests embed the dialogs; to see them as OS windows, run
  `tools/capture_windows.ps1` (starts its own BananaV with
  `BANANAV_OPEN="Options.Preferences;…"`, captures only that process's
  windows, closes it — it never touches the user's mouse or keyboard).
  `BANANAV_OPEN`'s commands run once BananaV started, the project given
  opened, any context's (`Edit.Select.All;Tools.Video.VideoEventFX;
  Transport.Play` plays with the FX window open): with `BV_FRAME_TRACE`
  and `BV_PREVIEW_TRACE`, and `APPDATA` and `LOCALAPPDATA` pointing to
  copies, it measures the interface and the preview without touching the
  user's settings.
- Media oracles: ffmpeg/ffprobe on PATH (development only; on Linux they
  test FFmpeg's backend too, and `bv-mf --test ffmpeg` checks it without
  them: a movie written and read back). Fixtures live in
  `crates/*/testdata`; generated media in `test-media/generated` (ignored by
  git), e.g. `cargo test -p bv-mf --test oraculo -- --ignored`. Every
  encoder has one: `bv-mf --test encoders` and `--test mux` (each encoder of
  the computer), `bv-image`, `bv-flac` and `bv-prores --test oraculo`, and
  `bv-engine --test exportar` (every format through the engine). Every
  reader too: `bv-media --test importar` (each video and audio format the
  editor imports: frames, and audio start found with a chirp),
  `bv-image --test leitura`, `bv-wav --test leitura`, `bv-ac3 --test
  oraculo`, `bv-prores --test leitura` and `bv-wic --test encode`.
- Effects against the user's prints: `cargo test --release -p bv-engine
  --test effect_frames -- --ignored` draws a picture (`BV_FRAMES_PICTURE`)
  through each factory preset of the effects `BV_FRAMES_EFFECTS` names, or
  the settings a file lists (`BV_FRAMES_SCENARIOS`: a line each — name,
  plug-in id, preset or `-`, `param=values;…`), in a new project
  `BV_FRAMES_SIZE` big (`BV_FRAMES_FORMAT=32`: 32-bit linear;
  `BV_FRAMES_VIEW=sRGB/ACES`: under ACES through that view;
  `BV_FRAMES_GAMMA=video`: 32-bit with the 2.222 gamma), as the
  Video Preview shows it, into `BV_FRAMES_OUT` (`BV_FRAMES_BINARIES`:
  other makers' plug-in binaries loaded to draw them too, the libraries
  they need on `PATH`); compare in the prints'
  project format (theirs are 8-bit). A print's unaffected picture, each
  pixel doubled, goes in and comes back exactly through a 2 × 2 box.
  Better than screenshots (compressed, scaled, overlaid): the calibration
  card of `tools/calibration_card.py` (a Hald CLUT — every color of a
  64-step lattice, so an effect's output there is its whole color table —
  ramps, a slanted edge, single pixels, lines, a zone plate) put through
  the effect in VEGAS at the card's size and saved losslessly (the
  preview's snapshot at Best (Full), or a PNG/EXR render); BananaV returns
  it bit-exact without effects.
- The graphics card: `bv-gpu --test parity` draws every transition, effect,
  framing and Y′CbCr layout on the card and on the processor and compares
  them, on every card of the computer (it passes without a card;
  `WGPU_BACKEND=vulkan`, `gl`… tries other interfaces). `bv-mf --test
  hardware` compares decoding on the card with the processor's, and
  `bv-engine --test desempenho` measures the preview of a crossfade of one
  1080p60 recording (`--release`). `BV_HARDWARE_DECODING=auto` runs the
  other tests with decoding on the card.
