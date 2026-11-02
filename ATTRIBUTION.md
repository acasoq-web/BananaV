# Attribution

Every asset in BananaV, with its author, source and license. The rules are in
[AGENTS.md](AGENTS.md) §2: open licenses compatible with GPL-3.0-or-later or
original work only, nothing from proprietary editing programs, and every asset
file with a `<file>.attribution` sidecar beside it giving the full details.
A pull request that adds an asset adds its line here too.

BananaV's own code and assets are licensed under the
[GNU General Public License, version 3 or later](LICENSE). The name and the
logo as marks are covered by [TRADEMARKS.md](TRADEMARKS.md) (a draft).

## Asset files

| Asset | Author | Source | License |
|---|---|---|---|
| `packaging/icons/bananav-icon.svg` | @acasoq-web, BananaV's owner | Original work: the application icon, the logo on a dark rounded square | GPL-3.0-or-later; the mark: TRADEMARKS.md |
| `packaging/icons/bananav-logo.svg` | @acasoq-web, BananaV's owner | Original work: the logo on its dark square | GPL-3.0-or-later; the mark: TRADEMARKS.md |
| `packaging/icons/bananav-logo-transparent.svg` | @acasoq-web, BananaV's owner | Original work: the logo on a transparent background | GPL-3.0-or-later; the mark: TRADEMARKS.md |
| `packaging/icons/bananav.png` | @acasoq-web, BananaV's owner | Rendered from `bananav-icon.svg` at 256 px (the AppImage's and the main window's icon) | GPL-3.0-or-later; the mark: TRADEMARKS.md |
| `docs/brand/bananav-icon.svg`, `bananav-logo.svg`, `bananav-logo-transparent.svg`, `bananav.png` | @acasoq-web, BananaV's owner | Copies of the files in `packaging/icons`, for the README and the website | GPL-3.0-or-later; the mark: TRADEMARKS.md |
| `docs/brand/social-preview.svg` | @acasoq-web, BananaV's owner (logo); BananaV contributors (layout) | Original work: the logo, the name and a line of text set in Inter (OFL-1.1, not embedded) | GPL-3.0-or-later; the mark: TRADEMARKS.md |
| `docs/brand/social-preview.png` | @acasoq-web, BananaV's owner (logo); BananaV contributors (layout) | Rendered from `social-preview.svg` (GitHub's social preview, 1280 × 640) | GPL-3.0-or-later; the mark: TRADEMARKS.md |
| `site/assets/img/bananav-icon.svg`, `bananav-logo-transparent.svg`, `bananav.png`, `social-preview.png` | @acasoq-web, BananaV's owner | Copies of the files in `docs/brand`, for the website | GPL-3.0-or-later; the mark: TRADEMARKS.md |
| `site/assets/img/screen-main.png`, `screen-color.png`, `screen-mixer.png`, `screen-timeline.png` | BananaV contributors | Original work: screenshots of BananaV from its headless screen tests (`crates/ui-egui/tests/telas*.rs`), showing only BananaV and media it generates (color bars, gradient, checkerboard, tones) | GPL-3.0-or-later |
| `crates/effects/src/arimo.rs` (glyph outlines) | The Arimo Project Authors | <https://github.com/googlefonts/arimo>: 16 glyphs of Arimo Bold, for the Timecode effect | OFL-1.1 (`crates/effects/fonts/OFL.txt`) |
| `crates/effects/src/lens_correction/lenses.rs` (lens table) | The lensfun project and its contributors | <https://github.com/lensfun/lensfun>: the distortion coefficients of 636 lenses of its database (version 0.3.95), for Lens Correction | CC-BY-SA-4.0, adapted from CC-BY-SA-3.0 (`crates/effects/licenses/lensfun.md`) |
| `crates/ocio/configs/bananav-aces-1.3.ocio` | BananaV contributors | Original work, written by `tools/ocio_config.py` from published primaries | GPL-3.0-or-later |
| `crates/png/testdata/` (20 files) | BananaV contributors | Original work: synthetic PNG fixtures written by ffmpeg, Pillow and an Adam7 generator, with their decoded pixels | GPL-3.0-or-later |
| `crates/image/testdata/exr/` (16 files) | BananaV contributors | Original work: a synthetic pattern saved by Blender with each OpenEXR compression | GPL-3.0-or-later |
| `crates/wav/testdata/` (5 files) | BananaV contributors | Original work: a sine tone written by ffmpeg in each WAV sample format | GPL-3.0-or-later |
| `crates/deflate/testdata/` (4 files) | BananaV contributors | Original work: generated data and its zlib streams | GPL-3.0-or-later |

## Assets drawn or generated in code

Original work by BananaV's contributors under GPL-3.0-or-later. None is traced
or derived from another program's artwork. They have no sidecars (they are
code), but they are assets all the same.

| Asset | Where |
|---|---|
| The interface's icons | `crates/ui-egui/src/icons.rs`, `crates/ui-egui/src/icons/glyphs.rs` (SVG path text on a 24-unit grid) and `icons/path.rs` (their reader) |
| The interface's colors and layout metrics | `crates/ui-egui/src/theme.rs`: values only, no artwork |
| Media generators (Checkerboard, Color Gradient, Noise Texture, Test Pattern, Credit Roll, Titles & Text) and the older built-in generators | `crates/effects/src/generators.rs`, `credit_roll.rs`, `titles.rs`; `crates/media/src/generator.rs` |
| The looks of the LUT Filter (written as `.cube` files when first needed) and of the Looks effect | `crates/effects/src/lut_filter.rs` (`looks`), `crates/effects/src/looks.rs` |
| The calibration card (made on demand, not stored) | `tools/calibration_card.py` |
| The Brazilian Portuguese translation of the interface | `crates/ui-egui/src/i18n/pt_br.rs` |

## Factory presets

Settings only: each factory preset of BananaV's effects and transitions is
a short name and parameter values written into the code
(`crates/effects/src/*.rs`, each effect's `presets()`, and
`crates/effects/src/transitions*`). Many follow the factory presets of the
program whose behaviour an effect follows — their names and values, never
its preset files — as [AGENTS.md](AGENTS.md) §2.2 allows; the others are
BananaV's own. GL Transition's presets are the GL Transitions collection's
code, listed below with its license.

## Third-party code and data in the repository

Not assets, but listed here so every outside source is in one place. Each
keeps its copyright notice and license text, and the packages ship the texts
their licenses ask for.

| What | Where in BananaV | Author | License |
|---|---|---|---|
| OpenFX 1.5's C headers (unchanged, the reference `crates/ofx-sys` follows) | `vendor/openfx/` | OpenFX and contributors to the OpenFX project | BSD-3-Clause (`vendor/openfx/LICENSE.md`) |
| egui_dock 0.21.1, with BananaV's changes | `vendor/egui_dock/` (`BANANAV.md` lists the changes) | Adam Gąsior and contributors | MIT (`vendor/egui_dock/LICENSE`) |
| egui-winit 0.36.2, with BananaV's change | `vendor/egui-winit/` (`BANANAV.md` lists the change) | Emil Ernerfeldt and contributors | MIT OR Apache-2.0 (`vendor/egui-winit/LICENSE-MIT`) |
| VST 3's interface declarations, written from the SDK's headers | `crates/vst3-sys/` | Steinberg Media Technologies GmbH | MIT (`crates/vst3-sys/licenses/vst3-pluginterfaces.txt`) |
| ACES 1.3's reference rendering and output transforms and Looks, ported from the Academy's CTL (aces-dev) | `crates/ocio/src/aces.rs` | Academy of Motion Picture Arts and Sciences (A.M.P.A.S.) | ACES license (`crates/ocio/licenses/ACES.md`) |
| 62 transitions of the GL Transitions collection, their GLSL with its uniforms renamed and their default settings | `crates/effects/src/transitions/gl/presets.rs` | gl-transitions contributors (each author listed in the license file) | MIT (`crates/effects/licenses/gl-transitions.md`) |
| Robertson's isotemperature table (correlated color temperature) | `crates/effects/src/white_balance.rs` | A. R. Robertson (1968), as published | Scientific data |

## Fonts and other assets inside the dependencies

Not in this repository, but built into the program by its dependencies:

| Asset | Comes with | Author | License |
|---|---|---|---|
| Ubuntu Light (the interface's default font) | `epaint_default_fonts` (egui) | Canonical Ltd. | Ubuntu Font Licence 1.0 |
| Hack (the monospace font) | `epaint_default_fonts` (egui) | Source Foundry Authors; based on DejaVu and Bitstream Vera | MIT, with the Bitstream Vera License |
| Noto Emoji | `epaint_default_fonts` (egui) | Google | OFL-1.1 |
| emoji-icon-font | `epaint_default_fonts` (egui) | John Slegers | MIT |

The licenses of the Rust crates themselves are checked by `cargo deny check
licenses` against [deny.toml](deny.toml).
