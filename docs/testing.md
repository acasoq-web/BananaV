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
- Why it feels slow: `BV_FRAME_TRACE=1` gives each frame of the window (how
  long after the last one, how long it took, the preview's picture, what
  asked for it), `BV_PREVIEW_TRACE=1` each frame drawn, `BV_GPU_TRACE=1` each
  kernel's compile time (`cargo test --release -p bv-gpu --test warmup --
  --ignored --nocapture`: a debug build compiles kernels without
  optimizing them, far faster than a release build does).
  `tools/stacks.cs` samples the threads of a running BananaV, its first
  lines telling how to build it and a copy with symbols. While playing, the
  window should paint about 60 times a second, its pictures evenly apart,
  and a frame's time stay well under that: compare with these (Ryzen
  16 threads, RTX 4070, a 1080p60 H.264 event with two effects):
  60.0 pictures a second, 16.2–17.1 ms apart, none skipped, a little over
  a processor core in all. Measure with BananaV in front and nobody using
  the computer: with another window in front or the screen locked the same
  build gives 15–50 pictures a second.
- **The performance guards: `cargo xtask perf`**, after any change to what
  the interface does every frame, to the preview or to the compositor.
  It builds an optimized copy of its own in `target/perf` (no link-time
  optimization, line tables for `tools/stacks.cs`) and runs
  `frame_budget` (`bv-ui-egui`): the demo project plays while each docked
  window is shown in turn beside the preview, and every window's median
  frame of the interface must stay within 6 ms (`BV_BUDGET_MS`), its 90th
  percentile within half as much again. A debug build only prints. On the
  reference computer every window takes 1.0–1.8 ms; with the LUT Filter's
  presets written out at every frame again (the slowdown of 2026-11-10)
  Color Grading took 12.7 ms and the test failed, as it should.
  `BV_BUDGET_FILE=<video>` plays a file instead of the demo project.
- With `BV_PLAY_FILE=<video>` set, `cargo xtask perf` also plays it in the
  engine (`play_file` in `bv-engine`, without the interface): different
  frames shown a second and the longest wait between two, the card's
  kernels compiled first (`BV_PLAY_WARM`). `BV_PLAY_PROJECT=3840x2160@59.94`
  (the project), `BV_PLAY_SIZE` (the size frames are drawn at: Auto's room
  or, at Full, the project's), `BV_PLAY_ROOM=1280x720` (the screen's
  pixels: frames drawn larger come back reduced), `BV_PLAY_EFFECTS=<ids>`
  (stacked on the events), `BV_PLAY_TRANSITION=dissolve|<id>` (two events
  of the file in a transition all along), `BV_PLAY_ANIMATE=1` (a Pan/Crop
  zoom and turn). Reference (a 4K H.264 trailer at 70 Mbit/s with
  keyframes up to 8 s apart, 4K 59.94 project at Best (Full) in a
  1280×720 room): plain, Pan/Crop animated with Glow, and in a 1080p
  project Push with Border and Gaussian Blur on both events, 59 frames a
  second, the longest wait 34 ms; four effects stacked (Color Corrector,
  Gaussian Blur, Glow, Vignette) 53–56; a 4K dissolve between two events
  of the file about 40 (two 4K decoders and twice the card's work: the
  card and the decoders are the limit, with no stalls); Push with Border
  and Gaussian Blur on two 4K events about 19.
- The whole program, as the user plays: `APPDATA`/`LOCALAPPDATA` on a
  copy of the settings (with `"match_first_media": false` and a
  `"new_project"` of the project wanted), `BV_FRAME_TRACE=1
  BANANAV_OPEN=Transport.Play bananav <video>` for half a minute, then
  the window's frames and the pictures shown a second worked out from
  the trace. Reference, the 4K trailer in a 4K 59.94 project at Best
  (Full): 59.9 pictures a second, the interface 3.4 ms a frame (before
  this round: 53.2 and 10.7 ms); opened alone (a 4K 29.97 project), 29.9
  pictures a second at 1.2 ms (before: 19.2 ms, the window at 44 frames
  a second).
- Against another program's files, none of them in the repository:
  `BV_VEG_SAMPLES=<first samples' folder> BV_VEG_SAMPLES2=<second samples'
  folder> BV_PANCROP_SAMPLES=<Pan/Crop cases' folder> cargo test -p bv-timeline
  --lib veg::
  -- --include-ignored` reads each sample as it says and writes it back,
  structure by structure; `BV_VEG_SAMPLES2=… cargo test -p bv-ui-egui --lib
  the_samples_transitions -- --ignored` brings its 27 transitions in;
  `BV_PANCROP_SAMPLES=<folder> BV_PANCROP_OUT=<folder> cargo test -p bv-engine
  --test pan_crop_renders -- --ignored --nocapture > out.txt` draws its
  Pan/Crop cases and compares them with the program's renders, frame by
  frame (`BV_PANCROP_FRAMES=1` lists each frame).
- macOS: `sh tools/mac_check.sh` runs clippy for Apple's chips from
  Windows or Linux (`MAC_TARGET=x86_64-apple-darwin` for Intel Macs); the
  kernels are checked against Metal there too
  (`every_kernel_translates_to_metal` in `bv-gpu`: each one written in
  Metal's language, within what Metal can do). The tests themselves run on
  GitHub's Macs: `gh workflow run ci.yml -f macos=true` (both processors,
  the interface's pictures drawn by Metal kept as the run's artifacts).
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
