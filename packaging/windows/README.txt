BananaV — video editor (Windows)
================================

- Website: https://bananav-editor.github.io/
- Help and community: https://discord.gg/vtC7Yasz98

The installer (BananaV-...-setup.exe) puts BananaV in your Start menu, with
no administrator password, and Settings > Apps uninstalls it. The zip needs
no installation: unpack it anywhere and run BananaV.exe. The builds are not
signed yet, so Windows may warn about an unknown publisher the first time
("More info", then "Run anyway").

Requirements
- Windows 10 or 11: the x64 package on Intel and AMD processors, the ARM64
  package on ARM ones (Snapdragon and the like).
- A graphics card with DirectX 12, Vulkan or OpenGL.
- Media Foundation (part of Windows) to open and render video. On
  Windows "N" editions, install the Media Feature Pack.

Settings
- Your settings (recent projects, window layouts, language, toolbars,
  keyboard maps, presets, video templates) are kept in
  %APPDATA%\BananaV\. Delete that folder to start over with the defaults.
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
  THIRD-PARTY-NOTICES.txt, beside this file.

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
