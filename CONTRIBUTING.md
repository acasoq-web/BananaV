# Contributing to BananaV

Thank you for helping! Bug reports, ideas, translations, tests and code are
all welcome.

Before you start, read **[AGENTS.md](AGENTS.md)**. Its rules are absolute and
apply to people and AI agents alike: never crash, open or original assets
only (each with an attribution sidecar), clean-room code, contributions
under GPL-3.0-or-later with a `Signed-off-by:` line. The architecture and
the project's working conventions are in [CLAUDE.md](CLAUDE.md).

## Building

BananaV is a Cargo workspace. You need **Rust 1.95 or newer** (edition 2024)
from [rustup](https://rustup.rs), with the components and target the checks
use:

```bash
rustup component add rustfmt clippy
rustup target add wasm32-unknown-unknown
```

### Windows

Windows 10 or 11, x64 or ARM64.

1. Install the Microsoft C++ Build Tools (the "Desktop development with C++"
   workload of Visual Studio or of its Build Tools) for the MSVC linker.
2. Build and run:

   ```bash
   cargo run --release -p bananav
   ```

Media are read and written through Media Foundation, part of Windows (on the
"N" editions, install the Media Feature Pack).

### Linux

x64 or ARM64, glibc 2.35 or newer. BananaV reads and writes media through
FFmpeg's libraries (5.1 or newer), plays sound through ALSA and draws with
Vulkan or OpenGL.

1. Install the development packages. On Ubuntu and Debian:

   ```bash
   sudo apt-get install libasound2-dev pkg-config libclang-dev \
       libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev
   ```

   On other distributions, install their equivalents (ALSA, pkg-config,
   libclang and FFmpeg's development files). For the graphics card tests
   without a card, Mesa's software Vulkan helps (`mesa-vulkan-drivers` on
   Ubuntu).
2. Build and run:

   ```bash
   cargo run --release -p bananav
   ```

`RUSTFLAGS="--cfg bv_no_ffmpeg"` builds without FFmpeg (no media on Linux),
to check the code on a computer that lacks its development files.

## Before every pull request: `cargo xtask ci`

```bash
cargo xtask ci
```

It runs, in order: `rustfmt` (`cargo xtask fmt --fix` formats), the layer
rules between crates (`cargo xtask layers`), clippy on all targets with
warnings as errors, every test of the workspace, and the WebAssembly build of
the portable crates. **A pull request must keep it green.** Run it on your
own computer before opening the pull request and after each change you push.

Also useful:

- `cargo deny check licenses` (after `cargo install cargo-deny --locked`):
  every dependency's license is one [deny.toml](deny.toml) allows. Run it when
  you add or update a dependency.
- `cargo xtask i18n`: the interface texts a translation lacks.
- `cargo test -p bv-ui-egui --test telas -- --ignored`: captures of the
  interface, drawn off-screen, in `target/telas/*.png`. The interface is
  checked this way, never by driving the mouse or the keyboard.
- Tests marked `#[ignore]` compare BananaV with the `ffmpeg` and `ffprobe`
  tools (media oracles) or need hardware; run them when you change that
  area. [CLAUDE.md](CLAUDE.md) lists them.

## Making a change

- **One subject per pull request**, as small as it can be. For a large
  change or a new feature, open an issue first so the design can be talked
  over before the work.
- **English** for code, identifiers, comments, documentation and commit
  messages.
- **Interface texts** go through the translation functions (`tr`, `trf!`,
  `trn!`, `trc`) of `crates/ui-egui/src/i18n`, and every new or changed text
  gets its Brazilian Portuguese entry in `crates/ui-egui/src/i18n/pt_br.rs`
  (the tests check it). Numbers shown to the user go through `crate::fmt`.
- **Tests**: a bug fix comes with a test that failed before it; a new
  feature with tests of what it does.
- **Assets** (images, icons, fonts, presets, media…) follow
  [AGENTS.md](AGENTS.md) §2: an open license or your original work, a
  `<file>.attribution` sidecar and a line in [ATTRIBUTION.md](ATTRIBUTION.md).
- **Dependencies**: only under the licenses [deny.toml](deny.toml) allows.
  Prefer writing a small part yourself to adding a large dependency.

## Commits

- A subject line in English, in the imperative ("Add …", "Fix …", "Keep …"),
  about 50–72 characters, no period at the end.
- A blank line, then a body saying what changed and why, wrapped at about 72
  characters.
- One subject per commit; a pull request may have several commits.
- Every commit ends with your sign-off (see below). Credit an AI agent that
  helped with a `Co-Authored-By:` or `Assisted-by:` trailer; agents never
  sign off for you.

```text
Keep a trimmed event's fades inside it

Trimming an event shorter than its fades left the fade-out past the
event's end. The fades now shrink with the event, in the same undo.

Signed-off-by: Your Name <you@example.org>
```

## Developer Certificate of Origin

BananaV uses the Developer Certificate of Origin instead of a contributor
license agreement. Signing off a commit (`git commit -s`, which adds a
`Signed-off-by: Your Name <your email>` line) means you certify the
following:

```text
Developer Certificate of Origin
Version 1.1

Copyright (C) 2004, 2006 The Linux Foundation and its contributors.

Everyone is permitted to copy and distribute verbatim copies of this
license document, but changing it is not allowed.


Developer's Certificate of Origin 1.1

By making a contribution to this project, I certify that:

(a) The contribution was created in whole or in part by me and I
    have the right to submit it under the open source license
    indicated in the file; or

(b) The contribution is based upon previous work that, to the best
    of my knowledge, is covered under an appropriate open source
    license and I have the right under that license to submit that
    work with modifications, whether created in whole or in part
    by me, under the same open source license (unless I am
    permitted to submit under a different license), as indicated
    in the file; or

(c) The contribution was provided directly to me by some other
    person who certified (a), (b) or (c) and I have not modified
    it.

(d) I understand and agree that this project and the contribution
    are public and that a record of the contribution (including all
    personal information I submit with it, including my sign-off) is
    maintained indefinitely and may be redistributed consistent with
    this project or the open source license(s) involved.
```

Forgot to sign off? `git commit --amend -s` fixes the last commit, and
`git rebase --signoff main` every commit of your branch (then push with
`--force-with-lease`).

## Reporting bugs

1. Search the [issues](https://github.com/acasoq-web/BananaV/issues) first:
   it may be known already.
2. Open a **Bug report** with:
   - BananaV's version (the package's name, or the commit you built);
   - your system (Windows or Linux and its version, x64 or ARM64) and
     graphics card;
   - the steps that make it happen, what you expected and what happened;
   - when you can, a small project file and media that show it. Share only
     media you made or that is openly licensed.
3. A crash or a lost project is the most serious kind of bug: say so in the
   title.
4. **Security problems** are not reported in public issues: see
   [SECURITY.md](SECURITY.md).

Questions and help are on the community chat: see [SUPPORT.md](SUPPORT.md).

## Credits

Everyone who contributes is credited by their GitHub username. To be credited
by a real or display name too, add **your own** entry to
[contributors/people.toml](contributors/people.toml) in your pull request
([AGENTS.md](AGENTS.md) §5).

## Code of conduct

Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md).
