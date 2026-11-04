BananaV — video editor (Linux)
==============================

- Website: https://bananav-editor.github.io/
- Help and community: https://discord.gg/vtC7Yasz98

Make the AppImage executable (chmod +x BananaV-*.AppImage) and run it.
Nothing to install.

Requirements
- A 64-bit Linux from 2022 on (glibc 2.35 or newer: Ubuntu 22.04,
  Debian 12, Fedora 36 and later): the x64 package on Intel and AMD
  processors, the ARM64 package on ARM ones.
- A graphics card with Vulkan or OpenGL (Mesa or NVIDIA's driver).
- An X11 or Wayland desktop. Under Wayland BananaV's windows go through
  XWayland, so that dialogs can be placed and the eyedropper and the
  numeric keypad work; BANANAV_WAYLAND=1 gives Wayland's own windows.
- Sound through ALSA (PipeWire and PulseAudio offer it).
- Video decoding and encoding on the graphics card: VA-API drivers on AMD
  and Intel cards (mesa-va-drivers, intel-media-va-driver), NVIDIA's
  driver on NVIDIA cards.

Settings
- Your settings (recent projects, window layouts, language, toolbars,
  keyboard maps, presets, video templates) are kept in
  ~/.config/BananaV/. Delete that folder to start over with the defaults.
- The interface is in English; Options › Preferences › Language switches
  it to Brazilian Portuguese.

License
- BananaV is free software: you can redistribute it and/or modify it
  under the terms of the GNU General Public License as published by the
  Free Software Foundation, either version 3 of the License, or (at your
  option) any later version (LICENSE.txt, beside this file). It comes
  with ABSOLUTELY NO WARRANTY.
- Its source code is at https://github.com/acasoq-web/BananaV.
- The open-source libraries it is built with, and their licenses, are in
  THIRD-PARTY-NOTICES.txt; the system libraries the AppImage carries, and
  theirs, in SYSTEM-LIBRARIES-NOTICES.txt, beside this file.

FFmpeg
- BananaV reads and writes media with FFmpeg's libraries, included here
  under the GNU Lesser General Public License 2.1 or later
  (https://ffmpeg.org; FFmpeg-LGPL-2.1.txt and FFmpeg-license.md, beside
  this file). They were built from FFmpeg's sources by
  packaging/linux/build_ffmpeg.sh. FFmpeg's sources, that script and the
  sources of LAME (libmp3lame, also under the LGPL), which the AppImage
  carries too, are published beside this package, in the same release, as
  BananaV-<version>-LGPL-sources.tar.gz.

Fonts
- The Timecode effect writes with the glyphs of Arimo Bold, Copyright
  2020 The Arimo Project Authors, under the SIL Open Font License 1.1
  (Arimo-OFL.txt, beside this file).

Transitions
- The GL Transition plays transitions of the GL Transitions collection
  (https://gl-transitions.com), ported from their GLSL: Copyright
  2017-present gl-transitions contributors, under the MIT license, each
  transition's author in GL-Transitions-license.md, beside this file.

Color
- Color management (ACES) works out the Academy Color Encoding System's
  reference rendering and output transforms, ported from the Academy's
  aces-dev 1.3: Copyright 2015 Academy of Motion Picture Arts and Sciences
  (A.M.P.A.S.), under the ACES license, its conditions and Disclaimer of
  Warranty in ACES-license.md, beside this file.
