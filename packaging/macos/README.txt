BananaV — video editor (macOS)
==============================

- Website: https://bananav-editor.github.io/
- Help and community: https://discord.gg/vtC7Yasz98

Drag BananaV into Applications, then open it from there. One app runs on
every Mac it supports: natively on Apple's chips (M1 and later) and on
Intel processors.

The first time
- If this copy is not notarized by Apple, macOS stops it the first time
  ("BananaV cannot be opened" or "Apple could not verify…"). Open it
  anyway: in System Settings › Privacy & Security, under Security, click
  "Open Anyway" beside BananaV's name (or, on macOS 14 and earlier,
  Control-click BananaV in Applications and choose Open). macOS remembers
  the choice.
- macOS asks before BananaV records from the microphone or an audio
  interface, the first time it does; and, for the Screen Capture window,
  to let BananaV record the screen (System Settings › Privacy & Security ›
  Screen & System Audio Recording).

Requirements
- macOS 11 Big Sur or later.
- Metal (every Mac that runs macOS 11).
- Video decoding and encoding on the Mac's media engine (VideoToolbox):
  H.264 and HEVC; ProRes is BananaV's own.

Keys
- BananaV's Ctrl shortcuts use the Command key (⌘): Command-S saves,
  Command-click picks more events, Command-drag moves in fine steps. The
  menus and tooltips name them so ("Cmd+S"). Alt is the Option key (⌥).
  A Control-click is a right click.
- Quit (Command-Q, the Dock's) asks about unsaved changes first.

Plug-ins
- VST 3 effects from /Library/Audio/Plug-Ins/VST3 and
  ~/Library/Audio/Plug-Ins/VST3, VST 2 ones from the VST folders beside
  them, OpenFX ones from /Library/OFX/Plugins (Options › Preferences adds
  more folders). A plug-in's own window shows inside the FX window.
- A plug-in made for Intel Macs only does not load in BananaV on Apple's
  chips: to use it, quit BananaV, select it in Applications, choose File ›
  Get Info and turn on "Open using Rosetta" (BananaV then runs as an Intel
  program, with every Intel plug-in).

Settings
- Your settings (recent projects, window layouts, language, toolbars,
  keyboard maps, presets, video templates) are kept in
  ~/Library/Application Support/BananaV/. Delete that folder to start over
  with the defaults.
- The interface is in English; Options › Preferences › Language switches
  it to Brazilian Portuguese.

License
- BananaV is free software: you can redistribute it and/or modify it
  under the terms of the GNU General Public License as published by the
  Free Software Foundation, either version 3 of the License, or (at your
  option) any later version (LICENSE.txt, in "Licenses and notices"). It
  comes with ABSOLUTELY NO WARRANTY.
- Its source code is at https://github.com/acasoq-web/BananaV.
- The open-source libraries it is built with, and their licenses, are in
  THIRD-PARTY-NOTICES.txt, in "Licenses and notices".

FFmpeg and the codec libraries
- BananaV reads and writes media with FFmpeg's libraries, inside
  BananaV.app under the GNU Lesser General Public License 2.1 or later
  (https://ffmpeg.org; FFmpeg-LGPL-2.1.txt and FFmpeg-license.md, in
  "Licenses and notices"), with dav1d (BSD), LAME (LGPL) and SVT-AV1 (BSD,
  with its patent license), each one's license beside FFmpeg's. They were
  built from their sources by packaging/macos/build_ffmpeg.sh. FFmpeg's
  and LAME's sources, and that script, are published beside this package,
  in the same release, as BananaV-<version>-macOS-LGPL-sources.tar.gz.

Fonts
- The Timecode effect writes with the glyphs of Arimo Bold, Copyright
  2020 The Arimo Project Authors, under the SIL Open Font License 1.1
  (Arimo-OFL.txt).

Transitions
- The GL Transition plays transitions of the GL Transitions collection
  (https://gl-transitions.com), ported from their GLSL: Copyright
  2017-present gl-transitions contributors, under the MIT license, each
  transition's author in GL-Transitions-license.md.

Color
- Color management (ACES) works out the Academy Color Encoding System's
  reference rendering and output transforms, ported from the Academy's
  aces-dev 1.3: Copyright 2015 Academy of Motion Picture Arts and Sciences
  (A.M.P.A.S.), under the ACES license, its conditions and Disclaimer of
  Warranty in ACES-license.md.
