# GL Transitions

BananaV's GL Transition (`crates/effects/src/transitions/gl.rs`) ships
transitions of the GL Transitions collection
(<https://github.com/gl-transitions/gl-transitions>, <https://gl-transitions.com>)
as its presets' GLSL (`crates/effects/src/transitions/gl/presets.rs`): the
collection's code, from its published package, its repository or its pull
requests as listed below, its uniforms renamed after the panel's controls
and, for a few, a setting made fixed or adjustable. Each was written by the
author listed below and is under the MIT license, as its file header says:

MIT License

Copyright (c) 2017-present gl-transitions contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## The transitions and their authors

| In BananaV | The collection's file | Author |
|---|---|---|
| Bounce | Bounce.glsl (gl-transitions 1.43.0) | Adrian Purser |
| Bow Tie Horizontal | BowTieHorizontal.glsl (gl-transitions 1.43.0) | huynx |
| Bow Tie Vertical | BowTieVertical.glsl (gl-transitions 1.43.0) | huynx |
| Burn | burn.glsl (gl-transitions 1.43.0) | gre |
| Butterfly Wave Scrawler | ButterflyWaveScrawler.glsl (gl-transitions 1.43.0) | mandubian |
| Circle Crop | CircleCrop.glsl (gl-transitions 1.43.0) | fkuteken |
| Circle Open | circleopen.glsl (gl-transitions 1.43.0) | gre |
| Color Distance | ColourDistance.glsl (gl-transitions 1.43.0) | P-Seebauer |
| Color Fade | FadeColor.glsl | gre |
| Color Phase | colorphase.glsl (gl-transitions 1.43.0) | gre |
| Color Planes | ColorPlanes.glsl (pull request #175) | Adam Liu |
| Crazy Parametric Fun | CrazyParametricFun.glsl (gl-transitions 1.43.0) | mandubian |
| Cross Hatch | crosshatch.glsl (gl-transitions 1.43.0) | pthrasher |
| Cross Warp | crosswarp.glsl (gl-transitions 1.43.0) | Eke Péter <peterekepeter@gmail.com> |
| Cross Zoom | CrossZoom.glsl (gl-transitions 1.43.0) | rectalogic |
| Cube | cube.glsl (gl-transitions 1.43.0) | gre |
| Directional | Directional.glsl (gl-transitions 1.43.0) | Gaëtan Renaudeau |
| Directional Scale | DirectionalScaled.glsl | Thibaut Foussard |
| Directional Warp | DirectionalWarp.glsl | pschroen |
| Doom Screen | DoomScreenTransition.glsl (gl-transitions 1.43.0) | Zeh Fernando |
| Doorway | doorway.glsl (gl-transitions 1.43.0) | gre |
| Dots Curtain | PolkaDotsCurtain.glsl (gl-transitions 1.43.0) | bobylito |
| Dreamy | Dreamy.glsl (gl-transitions 1.43.0) | mikolalysenko |
| Dreamy Zoom | DreamyZoom.glsl (gl-transitions 1.43.0) | Zeh Fernando |
| Flyeye | flyeye.glsl (gl-transitions 1.43.0) | gre |
| From In | coord-from-in.glsl (January 2024) | haiyoucuv |
| Glitch Displace | GlitchDisplace.glsl | Matt DesLauriers |
| Glitch Memories | GlitchMemories.glsl (gl-transitions 1.43.0) | Gunnar Roth |
| Gray Fade | fadegrayscale.glsl (gl-transitions 1.43.0) | gre |
| Grid Flip | GridFlip.glsl (gl-transitions 1.43.0) | TimDonselaar |
| Hexagonalize | hexagonalize.glsl (gl-transitions 1.43.0) | Fernando Kuteken |
| Honeycomb | honeyComb.glsl (pull request #174) | linjunping |
| Kaleidoscope | kaleidoscope.glsl (gl-transitions 1.43.0) | nwoeanhinnogaehr |
| Linear Blur | LinearBlur.glsl (gl-transitions 1.43.0) | gre |
| Luminance Melt | luminance_melt.glsl (gl-transitions 1.43.0) | 0gust1 |
| Morph | morph.glsl (gl-transitions 1.43.0) | paniq |
| Mosaic | Mosaic.glsl (gl-transitions 1.43.0) | Xaychru |
| Multiply Blend | multiply_blend.glsl (gl-transitions 1.43.0) | Fernando Kuteken |
| Over Exposure | Overexposure.glsl (January 2024) | Ben Zhang |
| Perlin | PerlinDissolve.glsl | Rich Harris |
| Pin Wheel | pinwheel.glsl (gl-transitions 1.43.0) | Mr Speaker |
| Pixelize | pixelize.glsl (January 2024) | gre |
| Power Kaleido | powerKaleido.glsl (January 2024) | Boundless |
| Random Squares | randomsquares.glsl (gl-transitions 1.43.0) | gre |
| Ripple | ripple.glsl (gl-transitions 1.43.0) | gre |
| Rotate Scale Fade | rotate_scale_fade.glsl (gl-transitions 1.43.0) | Fernando Kuteken |
| Squares Wire | squareswire.glsl (gl-transitions 1.43.0) | gre |
| Static Wipe | static_wipe.glsl (January 2024) | Ben Lucas |
| Squeeze | squeeze.glsl (gl-transitions 1.43.0) | gre |
| Stereo Viewer | StereoViewer.glsl (gl-transitions 1.43.0) | Ted Schundler |
| Stretch | stretch.glsl (pull request #168) | DanMan |
| Swap | swap.glsl (gl-transitions 1.43.0) | gre |
| Swirl | Swirl.glsl (gl-transitions 1.43.0) | Sergey Kosarevsky |
| Undulating Burn Out | undulatingBurnOut.glsl (gl-transitions 1.43.0) | pthrasher |
| Water Drop | WaterDrop.glsl (gl-transitions 1.43.0) | Paweł Płóciennik |
| Wave | wave.glsl (pull request #164) | a |
| Wind | wind.glsl (gl-transitions 1.43.0) | gre |
| Window Blinds | windowblinds.glsl (gl-transitions 1.43.0) | Fabien Benetou |
| Window Slice | windowslice.glsl (gl-transitions 1.43.0) | gre |
| Zoom | SimpleZoom.glsl (gl-transitions 1.43.0) | 0gust1 |
| Zoom In Circle | ZoomInCircles.glsl (gl-transitions 1.43.0) | dycm8009 |
| Zoom Out | scale-in.glsl (pull request #161) | 1 |
