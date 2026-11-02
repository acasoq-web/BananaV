# BananaV — rules for contributors and agents

These rules apply to everyone who changes BananaV: people and AI agents
alike. [CLAUDE.md](CLAUDE.md) and any other instruction file for agents hold
the working instructions (architecture, conventions, how to test); this file
holds the rules that must never be broken. **When the two disagree, this file
wins.**

A pull request that breaks one of these rules is not merged, however good the
rest of it is.

## 1. Never crash

People trust a video editor with hours of work. A damaged file, an odd value
in a project, a plug-in that misbehaves, a full disk or an unexpected click
must end in a message the user can act on, never in a crash and never in lost
work. A crash is fixed before anything is built on top of it.

### Required today

These are enforced by the compiler through the workspace's lints and checked
by `cargo xtask ci`, which every contributor runs before opening a pull
request. A change that breaks them does not go in:

1. **No `unsafe` outside FFI.** `unsafe_code = "deny"` is set for the whole
   workspace. Only the modules that talk to the operating system, codecs,
   graphics drivers or plug-ins through their C interfaces (BananaV's own
   effects are OpenFX plug-ins too) opt in, locally, with
   `#[allow(unsafe_code)]`.
2. **`cargo xtask ci` is green**: rustfmt, the layer rules between crates,
   clippy on all targets (libraries, binaries, tests) with `-D warnings`,
   every test of the workspace, and the WebAssembly build of the crates
   marked portable.
3. **No placeholders or debugging leftovers.** The workspace's lints, made
   errors by `-D warnings`, refuse `todo!()`, `dbg!()` and printing to
   standard output, except where a module allows one explicitly (the command
   line tools print).

### Goals (not verified yet)

The rules below are where BananaV is heading. They are **not** checked by any
tool today, and the current code does not meet all of them (it still has
`unwrap()` and `expect()` calls on paths that could fail, and no panic hook).
New code should follow them, and a pull request that removes a panic path is
always welcome:

1. **Fail with `Result`.** Return an error with context and show it to the
   user (a message, the status bar) instead of `unwrap()`, `expect()`,
   `panic!`, `unreachable!` or `unimplemented!` outside tests. A feature
   that is not ready is shown greyed out ("Coming soon"), never reached
   through a panic.
2. **Treat every input as hostile.** Media files, projects, interchange
   files, presets, fonts, plug-in answers and settings files can be damaged
   or crafted. Check bounds, use checked or saturating arithmetic for sizes
   and offsets, never divide by a rate or a size that can be zero, and cap
   allocations sized by input.
3. **Bound recursion and loops** over data that can be cyclic or deeply
   nested.
4. **Don't cascade.** A poisoned lock or a failed worker thread (decoding,
   rendering, analysis) reports an error instead of taking the program down
   or leaving a job "running" forever.
5. **A last-resort guard.** A panic hook that logs the panic and keeps the
   unsaved project, as a safety net, not a substitute for the rules above.
   (Autosave already keeps a copy of a changed project every few minutes
   when it is turned on in Options › Preferences › General.)
6. **Prove it.** A crash fix comes with a small test that failed before the
   fix; readers and decoders get tests with truncated and corrupted input.

When one of these goals becomes checked by a tool, it moves to "Required
today" in the same pull request that adds the check.

## 2. Assets: open licenses only, every asset attributed

**What counts as an asset:** any image, icon, cursor, logo, illustration,
screenshot, font, color look-up table, preset, template, sound, music, video,
3D model, sample project or other non-code media. This covers files in the
repository, bytes embedded in code (`include_bytes!`, Base64, data URIs) and
data written into code to reproduce a picture (for example point lists traced
from someone else's icon).

1. **Every asset is open or original.** It is either
   - under an open license compatible with GPL-3.0-or-later: MIT, Apache-2.0,
     the BSD licenses, ISC, zlib, the SIL Open Font License 1.1 (fonts),
     CC0 or the public domain, CC BY 4.0 or CC BY-SA 4.0 (no "NonCommercial"
     or "NoDerivatives" licenses); or
   - the original work of the contributor, who owns it and licenses it to the
     project under GPL-3.0-or-later.

   If the license is unknown or unclear, the asset does not go in.
2. **Nothing from proprietary editing programs.** No artwork, icons,
   cursors, logos, splash screens, interface graphics, screenshots, fonts,
   look-up tables, preset or template files, sample media or sounds taken
   from a proprietary video, audio or photo editing program, in any form,
   not even traced, redrawn or recolored. BananaV's icons may follow generic
   conventions (a play triangle, a razor blade, a speaker), but each is drawn
   from scratch without looking at another program's artwork. An effect's
   factory presets are settings, not artwork: they may follow the presets of
   the program whose behaviour the effect follows — their short names and
   their parameters' values, written into the code by hand from using that
   program — but never its preset files themselves, nor what a preset
   carries besides settings (pictures, drawn gradients, look-up tables).
3. **A sidecar for every asset file.** Next to each asset file `X`, add a
   file `X.attribution`:

   ```text
   asset:        <file name>
   title:        <what it is>
   author:       <creator / copyright holder>
   source:       <URL, or "original work">
   license:      <SPDX identifier or license name>
   license-file: <path to the license text, when the license asks for it to be shipped>
   added:        <YYYY-MM-DD> by <contributor>
   notes:        <how it was made or changed; for screenshots, what they show>
   ```

   A folder of generated test fixtures may share one sidecar named after the
   folder (`testdata.attribution`, next to `testdata/`) when every file in
   it has the same author, source and license; its notes say how each file
   was made. Data written into code (glyph outlines, a table) has its
   sidecar next to the source file (`arimo.rs.attribution`).
4. **One line in [ATTRIBUTION.md](ATTRIBUTION.md)** for every asset (or
   folder of fixtures with a shared sidecar), in the same pull request.
5. **Screenshots** in the repository show only BananaV (or other open
   projects), with media generated for the purpose or openly licensed. Their
   sidecar says what they show.
6. **Assets drawn or generated in code** (the interface's icons in
   `crates/ui-egui/src/icons.rs` and `crates/ui-egui/src/icons/`, the media
   generators, built-in looks) are original work under GPL-3.0-or-later.
   They need no sidecar, but they are listed in [ATTRIBUTION.md](ATTRIBUTION.md).
7. **When in doubt, leave it out**, and draw or generate it yourself.
8. **BananaV's name and logo** are the project's own marks. The logo files
   are in the repository with sidecars like any asset; how the name and the
   logo may be used is in [TRADEMARKS.md](TRADEMARKS.md).

## 3. Clean-room code

1. **Never disassemble, decompile, reverse engineer or copy** the binaries,
   code or bundled files of proprietary programs, and never read their
   installation folders beyond file names. This includes their license
   agreements' limits: if a program's terms forbid something, BananaV's
   contributors don't do it.
2. **Behaviour comes from public sources**: published specifications and
   standards (ITU-T, ISO/IEC, SMPTE, IETF, the Academy, OpenFX, VST 3…), public
   documentation and help pages, and using a program as any user would. Say
   in the pull request which public pages a feature follows.
3. **Name your sources inside the code.** Code comments, test names and the
   documentation for contributors (`CLAUDE.md`, `docs/ROADMAP.md`,
   `docs/STANDARDS.md` and the like) may, and should, name the program, the
   specification or the public documentation page a behaviour was taken
   from, so that whoever comes later can check it against its source. That
   is a record of where the behaviour comes from, never a copy of the
   program's code or assets, and it is not to be removed. It does not apply
   to what users see — the interface, its messages and help, settings,
   templates and the files BananaV writes — nor to the README and the
   project's other public pages (the website, release notes, store
   listings): those name no other editing program and never present BananaV
   as a copy of one. Interchange formats and standards keep their own
   names, even when a format is named after the program that introduced it.
4. **Reference material stays local.** Screenshots, recordings, notes, sample
   files or exported settings of other programs, used to study how they
   behave, stay on the computer of whoever studies them (for example in the
   git-ignored `local-reference/` folder). They are never committed,
   attached to issues or pull requests, embedded, traced or shipped.
5. **Third-party code only under a GPL-3.0-compatible license**, with its
   copyright notice and license text kept (in the crate's `licenses/`
   folder, or next to the code in `vendor/`), and a note of where it came
   from and what BananaV changed (as `vendor/*/BANANAV.md` do). If its
   license asks for the text to ship with the program, add it to the
   packages (`.github/workflows/package.yml`).
6. **Dependencies** are added only under the licenses `deny.toml` allows;
   `cargo deny check licenses` must pass. A new license needs a reason in
   the pull request.
7. **The ffmpeg and ffprobe tools** are test oracles and fixture generators
   on developers' computers only. On Linux, BananaV links FFmpeg's libraries
   (an LGPL build in the packages); nothing else of FFmpeg is copied into the
   repository.
8. **Plug-ins and files are never misled.** BananaV identifies itself as
   BananaV to plug-ins and in the files it writes, never as another
   program.

## 4. Licensing of contributions: GPL-3.0-or-later with a DCO

1. BananaV is licensed under the **GNU General Public License, version 3 or
   (at your option) any later version** ([LICENSE](LICENSE), SPDX
   `GPL-3.0-or-later`). Every contribution is licensed under the same terms.
2. Instead of a contributor license agreement, BananaV uses the
   **Developer Certificate of Origin** (DCO, version 1.1; its text is in
   [CONTRIBUTING.md](CONTRIBUTING.md#developer-certificate-of-origin)). Every
   commit carries a `Signed-off-by:` line with the author's real name, or a
   known identity, and an email address that can be reached
   (`git commit -s`). Signing off certifies that you have the right to
   submit the change under the project's license.
3. **An AI agent never signs off.** Only a person can certify the DCO. An
   agent's commits are reviewed by the person responsible for them, who adds
   their own `Signed-off-by:` line. The agent may be credited with a
   `Co-Authored-By:` or `Assisted-by:` trailer.
4. Copyright stays with each contributor; nobody assigns it to the project.

## 5. Credits

1. Everyone who contributes is credited by their GitHub username.
2. Anything more — a real name, a display name — is up to each person, in
   [contributors/people.toml](contributors/people.toml): **you add or change
   only your own entry**, in your own pull request.
3. **Never add, guess, edit or copy anyone else's entry or name**, not from
   git configurations, commit authors, emails or GitHub profiles.
4. **Agents working for a contributor:** when preparing a pull request, check
   whether your person's GitHub username has an entry. If not, ask them once
   whether they want to be credited by more than their username. If they
   do, add only their entry, with exactly the names they gave; if not,
   change nothing.

## 6. Interface texts and languages

English is the project's main language: code, comments, commit messages,
documentation and the interface's texts in the source are written in English.
Every text the user sees goes through the translation catalogs of
`crates/ui-egui/src/i18n` (see [CLAUDE.md](CLAUDE.md) for how).

## See also

- [CONTRIBUTING.md](CONTRIBUTING.md): building, checks, commits, bug reports
- [ATTRIBUTION.md](ATTRIBUTION.md): every asset, its author and license
- [TRADEMARKS.md](TRADEMARKS.md): BananaV's name and logo
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md), [SECURITY.md](SECURITY.md),
  [SUPPORT.md](SUPPORT.md)
- [CLAUDE.md](CLAUDE.md): working instructions;
  [docs/architecture.md](docs/architecture.md) and
  [docs/testing.md](docs/testing.md): how BananaV is built and checked
- [docs/ROADMAP.md](docs/ROADMAP.md) and [docs/STANDARDS.md](docs/STANDARDS.md)
