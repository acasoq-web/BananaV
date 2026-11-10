# BananaV — instructions for agents

BananaV is a free, open-source, clean-room video editor written in Rust,
for Windows and Linux (x64 and ARM64) and macOS (Apple's chips and
Intel, one app). Its workflow and layout follow VEGAS Pro; everything the
user sees is BananaV's own.

## Start every session here

1. **[AGENTS.md](AGENTS.md)** first: its rules override everything here.
2. The part of **[docs/architecture.md](docs/architecture.md)** about what
   you touch — the crates and their layers, the project model, frames on
   the processor and on the graphics card, the effects, the plug-in hosts,
   audio, the interface's windows, the platforms.
3. **[docs/testing.md](docs/testing.md)** for how a change is checked, and
   [CONTRIBUTING.md](CONTRIBUTING.md) for building.
4. [docs/ROADMAP.md](docs/ROADMAP.md) for what is done and what comes next;
   [docs/STANDARDS.md](docs/STANDARDS.md) before adding a codec, a plug-in
   format or a brand name to the interface.

## Non-negotiables

- **English first.** Code, comments, commits and docs are in English, and
  the interface is English by default; Brazilian Portuguese is a
  translation, picked in Options › Preferences › Language. Older code may
  have Portuguese comments: convert the parts you touch, never whole files
  only for that.
- **Every text the user sees** goes through `crates/ui-egui/src/i18n`:
  `tr("Save")`, `trf!("Saved to {}", path.display())`,
  `trn!(n, "{} file imported.", "{} files imported.", n)` (never
  "file(s)") and `trc("key", "End")`. Add or update its pt-BR entry in
  `i18n/pt_br.rs` (sorted, keyed by the English text; counted texts in
  `PLURALS`): the tests fail on a text without one and on an entry nothing
  uses, and `cargo xtask i18n` lists what a language lacks. Names
  translated by value (`tr(effect.name())`) are whole string literals;
  lower crates return English names and the interface translates them.
  Never translate identifiers or anything stored (command ids, the keys of
  `teclado.json`, serde names, file names, egui ids). Numbers shown go
  through `crate::fmt`; layouts measure their texts (Portuguese runs a
  third longer). A new language is a catalog module and a line of
  `languages!` in `i18n/mod.rs`. The eighteen partial languages
  (`zh_cn.rs`, `ru.rs`, `es_es.rs`, `de.rs`, `ja.rs`, `fr.rs`, `pl.rs`,
  `ko.rs`, `tr_tr.rs`, `zh_tw.rs`, `es_419.rs`, `th.rs`, `uk.rs`,
  `it.rs`, `cs.rs`, `hu.rs`, `pt_pt.rs`, `sv.rs`) are not kept up with
  new texts — the maintainer's decision: those show in English there;
  only pt-BR gets every new text.
- **VEGAS is the spec, never the source.** The maintainer's VEGAS
  screenshots ("the user's prints" in the docs) give each window's layout;
  VEGAS's public Help gives the behaviour: read it first and name the
  pages used. Never reverse engineer, decompile or disassemble VEGAS's
  binaries (its EULA forbids it), nor copy its icons or assets. Comments
  may say a behaviour follows VEGAS's; they never describe how VEGAS is
  built inside.
- **BananaV is its own software.** Nothing the user sees names VEGAS,
  MAGIX, Sony, Boris FX or their services ("Wave64", not "Sony Wave64").
  Features BananaV lacks stay visible but greyed out, with "Coming soon to
  BananaV" (`soon()` and `soon_toggle()` in `menus.rs`, `Action::Soon` in
  the keyboard map).
- **Layers.** Each crate declares a layer and depends only on lower ones
  (`cargo xtask layers`; the table is in
  [docs/architecture.md](docs/architecture.md)). The interface talks to
  the rest only through the project model (`bv_timeline::Project`, edited
  through `History`) and `bv_engine::Engine`.
- **Exact time:** ticks, 254,016,000,000 a second.
- **Frames:** `bv_timeline::scene::frame_scene` says what a frame shows;
  `bv-render` (the processor, the reference) and `bv-gpu` (wgpu kernels in
  `crates/gpu/src/shaders`) draw it alike, and the parity tests compare
  them. Kernels compile with Direct3D's old FXC compiler: small, no
  `switch` over effects, no dynamic indexing of vectors, every thread
  through the same rounds around a barrier; `var<workgroup>` tables are
  not zeroed (zeroing them cost FXC tens of seconds): write before reading.
- **BananaV's own effects are OpenFX plug-ins** (`crates/effects`): their
  settings and on-screen controls described through OpenFX, never
  special-cased in the interface, and a path on the graphics card
  (`accel` plans, the `kit_*` kernels) that draws as the processor does.
- **Dialogs** are OS windows made with `crate::floating::Window`, never
  `egui::Window` directly; nothing draws into a hidden OS window.
- **Checks:** `cargo xtask ci` stays green (rustfmt, layers, clippy with
  `-D warnings`, every test, the wasm build). The interface is checked
  headlessly, never by driving the user's mouse or keyboard, and tests
  never press OK or Apply on a settings window (that would write the
  user's settings).
- **Performance:** nothing heavy is worked out at every frame of the
  interface — keep it, or work it out when it changes (the LUT looks'
  tables written out every frame took a 4K playback from 60 frames a
  second to 40). After changing what the interface does every frame, the
  preview, the engine's playback or the compositor, `cargo xtask perf`
  passes (the frame budget of every docked window while the preview
  plays; with `BV_PLAY_FILE=<video>`, the engine playing it), and its
  numbers are compared with [docs/testing.md](docs/testing.md)'s.
- **Commits** say in English what changed, on green states only.

## Running and looking at the app

- `cargo run --release -p bananav` runs it; `BANANAV_OPEN="Options.Preferences;…"`
  runs keyboard-map commands once it started.
- `cargo test -p bv-ui-egui --test telas -- --ignored` (and `telas_inicio`,
  `telas_menus`, `telas_pt_br`) draws the interface into `target/telas`;
  `tools/capture_windows.ps1` captures the OS windows of a BananaV it
  starts itself.
- `BV_PREVIEW_TRACE=1`, `BV_FRAME_TRACE=1`, `BV_GPU_TRACE=1`,
  `BV_OFX_TRACE=1` and `BV_EFFECTS_TRACE=1` print what the preview, the
  interface, the kernels' compiler, the plug-in host and BananaV's effects
  do; `tools/stacks.cs` samples where a running BananaV spends its time.
- From Windows, `sh tools/linux_check.sh` runs clippy for Linux and `sh
  tools/mac_check.sh` for macOS (`MAC_TARGET=x86_64-apple-darwin` for
  Intel Macs); `gh workflow run package.yml` makes the packages (macOS's
  on GitHub's Macs), `gh workflow run ci.yml -f macos=true` checks and
  tests on them.
