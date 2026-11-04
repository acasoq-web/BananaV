# Changelog

What changed in each public build of BananaV. The
[Releases page](https://github.com/acasoq-web/BananaV/releases) has the
downloads, and each release's notes say how to install it.

## 0.2.0 — 2026-11-04

For Windows and Linux (x64 and ARM64), and the first with an installer for
Windows.

### Highlights

- **51 audio effects.** Equalizers (Modern Equalizer, Track EQ, Graphic, Parametric and Paragraphic EQ), dynamics (Compressor, Gate, Limiter, DeEsser, Multi-Band and Graphic Dynamics, Wave Hammer Surround), reverbs and delays, modulation (Chorus, Flanger, Phaser, Vibrato, Tremolo Pan, Amplitude Modulation), Distortion and Tube Stage, Vocal Strip, Pitch Shift, Time Stretch, Audio Restoration and the Express set, each with its panel and factory presets.
- **Compatible with `.veg` projects.** File › Open, File › Import and File › Export: tracks, events and takes, fades, transitions, audio effects, volumes, Pan/Crop with its masks, keyframes, Track Motion and busses. Import stays marked Experimental while real projects test it.
- **Color correction with LUTs.** Color Grading rebuilt: color wheels, curves, HSL curves, white balance, levels, tone and saturation, with Camera and Look LUTs from `.cube` files and Export LUT.
- **An installer for Windows.** Open it and follow the steps: BananaV goes to the Start menu, no administrator password needed. The zip stays for those who prefer not to install.
- **Pan/Crop rebuilt**, and compared frame by frame with reference renders.
- **Smooth playback.** The graphics card draws from the first frame, and a 1080p60 event with two effects plays at 60 frames a second, evenly, on about one processor core.

### Audio

- Each effect has its own panel, with sliders that reset on a double-click, and those drawn as curves (Graphic EQ, Graphic Dynamics, Distortion, Amplitude Modulation, Vibrato) have a graph editor.
- Pitch Shift, Time Stretch, Gapper/Snipper and Vibrato play on events, their sound made through the whole chain; in a track's or a bus's chain they let the sound through, and the FX window says so.
- Modern Equalizer: eight bands, with minimum and linear phase, oversampling, mid/side and per-channel routing.

### `.veg` projects

- Transitions come in with their settings and their progress envelopes; effect keyframes of every kind (numbers, points, colors, choices, with Bézier handles); effects switched off or placed before the Pan/Crop.
- Volume: each event's gain, Normalize, channels and phase; the tracks' volume, pan, envelopes and sends; the busses' faders, mute, solo, effects and envelopes.
- Video: velocity, Resample mode, Reduce Interlace, Undersample rate, Track Motion with its shadow, glow and 3D box, Parent Motion and compositing children.
- Export writes all of this except audio effect chains. Not imported yet: events of generated media, a transition at an event's end, and pan up and down.

### Color

- Older projects' Color Grading settings go back to their defaults.
- Lift and Gain eyedroppers, a neutral tone picker and a color selector; curve handles with Lock axis and Lock to grid.

### Pan/Crop

- An event's Pan/Crop is a box in its media's pixels; older projects come in as they were.
- Keyframes move along smooth paths with Smoothness; masks follow their keyframes, each with its own path settings.
- The rotation center can be dragged, the presets include the 1:1, 4:3, 16:9 and 1.85:1 shapes, and the menu resizes adjustment events to the project.

### Timeline and performance

- The Sharp curve is now 4p³ − 6p² + 3p in fades, envelopes and keyframes; snapping reaches 7 pixels; Esc stops playback; W and E go to the Trimmer's start and end.
- Peak files beside media (`.sfk`) give their waveforms without reading the sound.
- Effect shaders compile in the background in 6.5 s instead of 63 s, the window paints only when a frame is due, and the Explorer's tree no longer touches the disks every frame.

## 0.1.0 Ultra Alpha 2 — 2026-11-02

The second test build, for Windows and Linux (x64 and ARM64), and the first
with BananaV's source code public.

### Highlights

- **The source code is public.** BananaV is free software under the GPL, version 3 or later, and from this build on its source code is in this repository: the tag `v0.1.0-ultra-alpha.2` holds exactly the code these packages were built from. Read [AGENTS.md](https://github.com/acasoq-web/BananaV/blob/main/AGENTS.md) and [CONTRIBUTING.md](https://github.com/acasoq-web/BananaV/blob/main/CONTRIBUTING.md) to take part.
- **Third-party plug-ins, steady and fast.** Large packs of professional OFX plug-ins, with hundreds of effects in one file, now work. On Windows a plug-in that crashes is switched off instead of taking BananaV down, and one that needs a missing or mismatched library is skipped quietly instead of Windows showing an "Entry Point Not Found" box at every start.
- **Plug-ins on the graphics card.** Plug-ins that render with CUDA draw straight on NVIDIA cards: a 4K frame with a blur went from 180 to 80 ms. The on-screen controls plug-ins draw with OpenGL show over the preview.
- **Smooth playback through transitions.** No more stalls at a transition's start and end, and when effects are heavy the preview draws two frames at once.
- **Final Cut Pro 7 XML both ways.** BananaV opens other editors' XML files, including those that keep their media in bins, with transitions, stereo sound and each of a recording's audio streams, and writes them the way those editors read them. File › Open takes `.xml` and `.fcpxml` files.
- **DVD, Video CD and audio CD.** BananaV's own MPEG-2 and MPEG-1 encoders, DVD-Video authoring and burning, Video CDs and audio CD burning.
- **Tracking and stabilization.** Motion tracking, video and 360° stabilization, and slow motion and motion blur by optical flow.

### Plug-ins and performance

- On Windows, a plug-in that crashes is switched off until BananaV restarts, and the status bar says so.
- Fixed a crash some plug-ins caused when they started helper programs; plug-ins now load only when used and hear BananaV closing.
- Fixed BananaV crawling with many plug-ins installed: every lookup copied all their descriptions, several times a frame.
- A plug-in whose libraries are missing, or are another program's of the same name, no longer brings up Windows' "Entry Point Not Found" box over and over at start: it is listed as failed and the reason goes to the log.
- CUDA rendering on NVIDIA cards, and plug-ins' OpenGL controls over the preview.
- A crossfade's transition opens in the FX window with its presets and settings, and can be switched off without losing them.
- Playback without stalls at transitions: plug-ins are readied before each one starts, and two frames are drawn at once when effects are heavy.
- An effect plays as fast with its FX window open as with it closed: playback no longer reads each frame's input back from the graphics card for the window's pickers (still frames keep it), and plug-ins' OpenGL controls are drawn again only when they change, so the rest of the interface no longer slows down with them.
- A slow preview (4K with heavy effects, say) no longer freezes for the rest of playback after a frame that takes longer than about 0.8 s: it keeps showing frames as fast as it can draw them.
- Reverse playback decodes ahead and plays smoothly.

### Editing and timeline

- **A redesigned Pan/Crop window.** Its properties are laid out as a property grid: Position, Rotation (the angle and the point it turns about), Keyframe interpolation, Source (Maintain aspect ratio and Stretch to fill frame) and Workspace, beside a workspace you can resize.
- Pan/Crop keyframes: select several (click, Ctrl+click, Shift+click, or drag the cursor across them), drag them together (the first one too), and cut, copy, paste, delete or change their curve all at once from their menu or the keys. The keyframe timeline scrolls and zooms, and what lies past the event's end is shown white.
- The Pan/Crop window decodes the media at its own cursor, at the size it shows it, so it stays sharp and follows you while you scrub, with Sync Cursor on or off.
- Mask drawing: a click adds a corner and a drag a curve; clicking the first point closes the path; clicking a point turns it sharp, and clicking it again brings its handles back; dragging a line bends it, its handles showing as they follow; the Split Tangent tool adds a point exactly where a line is clicked. Pick several points with Ctrl, Shift or a rectangle, and move or delete them together. A mask's box appears when you double-click it and goes when you click elsewhere. The mask properties are laid out as a grid too.
- The selection rectangle highlights events as you drag it.
- An event's shortcut menu lists its commands in their usual order, greyed where they would do nothing; a crossfade shows its length in its middle while one of its events is selected, at any zoom, under its transition's name when it has one.
- **Event Properties** (an event's shortcut menu): the Video Event tab (the active take's name, the switches, the playback and undersample rates), or the Audio Event tab, then the Media and General tabs.
- Media Properties: Alpha channel (Undefined, None, Straight, Premultiplied, or Premultiplied against a background color), Color range (Undefined, Limited, Full) and Rotation (90° either way, 180°) now change how the media is read, and the Format names the file's type and codec. The new General tab shows the file's type, size and dates, each stream's format, color depth, bit rate and length, and the component that reads it.
- **Nested timelines** (create, open and go back to the main one), and nested projects used as media, playing their prerendered video.
- Sync links between events.
- Smart Split joins the video parts with the Warp Flow transition.
- Reversing an event makes the reversed subclip in Project Media.
- The Take Chooser plays and stops takes; a copy edited in the audio editor becomes a take.
- Double-clicking an event and dragging extends the time selection.
- The timeline scrolls by itself when a drag reaches its edge.
- Enter goes back to where playback started, and playback runs on past the project's end.
- In a crossfade the overlapping sound is shaded darker, and a video crossfade shows its transition's name in a blue band.
- Audio waveforms thin along their fades as you drag them, a crossfade shows both sounds merged, dark lines mark where each crossfade begins and ends, a selected event's outline stays visible over its neighbour, and the fade and level handles are blue tabs, the pointer changing over a fade's handle.
- Meters across the track headers, Restore Track Height, and dragging a header's edge sizes every selected track alike.
- Envelopes: double-click a line to add a point, drag it to raise or lower that stretch; a point's menu offers its preset values, Set To... (type the value right at the point), Delete, the fade curves, Flip, Thin and Reset All. Drop video effects on an adjustment track, or on any video track's header, to add them to the track.
- V and P add the volume and pan envelopes to the selected audio tracks, then hide and show them; a volume envelope has 0 dB in the middle of the track and +6 dB at the top, its points drawn as diamonds.
- With several tracks selected, Delete Track deletes them all in one undo.
- An envelope point under the pointer turns into a white diamond, its tooltip giving the envelope, the time and the value ("Composite Level at 00:01:45:14 is 42 %"); a double-click that adds a point puts the cursor on it. A crossfade's dark lines trim its events from either side: the start of the event coming in, the end of the one going out under it.

### Video and compositing

- **Pan/Crop masks**, with a redesigned Pan/Crop window, and masks in track and output effects.
- Motion tracking, with the motion handed to effects and regions imported from an effect; Bézier Masking tracks each of its masks.
- Video stabilization, with its Expert view, dual fish-eye stitching and 360° stabilization.
- Slow motion, motion blur, the Warp Flow transition and the Optical Flow resample mode.
- **3D compositing**, and Track Motion in 3D with layouts, views, a trackball, presets and editing in object space.
- The video bus track's automation, fade colors and the Video Output FX keyframes; Bypass Motion Blur on video tracks.
- Smart adaptive deinterlacing, interlaced renders, Undersample rate, each media's color range, and the properties of several media at once.
- Photoshop layers with ZIP compression, CMYK and layer masks, and blend modes on tracks.
- Adjustment tracks take video events too: they show over the tracks below and the track's effects work on them along with the rest, its level setting the effects' strength.

### Effects and transitions

- **Brightness and Contrast** (with Automatic Contrast), Levels, Black and White, Invert, Sepia, Gaussian Blur, Sharpen, Vignette, Mirror and Glow remade and measured pixel by pixel on a calibration card.
- Black Restore, Broadcast Colors, Channel Blend, Chroma Blur, Mask Generator, Median, Min and Max, Quick Blur, LAB Adjust, Convolution Kernel, Linear and Radial Blur, Offset and Wrap and Spherize remade with new panels, presets and math; Threshold (red, green and blue each against it), Wave, Swirl, Pinch/Punch and Deform the same. Projects that used them start them again from their defaults.
- Radial Pixelate, Tiny Planet, Checkerboard and Test Pattern remade the same way, with new patterns and presets.
- Chroma Keyer, Bump Map, Fill Light, Displacement Map (a brighter map brings pixels from below) and Height Map remade the same way; Bézier Masking with a new panel and math, projects keeping their masks.
- These remade effects, the generators Checkerboard and Test Pattern, the compositors Displacement Map, Height Map and Bump Map, and Gaussian Blur, Sharpen, Vignette, Mirror and Glow now run on the graphics card too, with the same result as on the processor.
- The Gradient Wipe transition.
- The Linear Wipe, Clock Wipe, Iris (fourteen shapes), Star Wipe, Dissolve (ten types), Flash and Cross Effect transitions remade with new panels, presets and math; Flash and Cross Effect blur with the same exact Gaussian. Projects that used them start them again from their defaults.
- The Barn Door, Push, Slide, Swap, Squeeze, Split, Zoom and Spiral transitions remade with new panels (a Border group: size, feather and color), presets and math: the pictures drawn into their rectangles to a 16,384th of a pixel, read between pixels or averaged when smaller, and a colored border along the moving edges; on the graphics card in a single pass. Projects that used them start them again from their defaults.
- The Venetian Blinds transition remade with a new panel (Angle, Feather, Number of blinds), presets and math: the blinds' openness along the frame's direction in 8-bit steps, softened by a Gaussian blur. Projects that used it start it again from its defaults.
- The Portals transition remade with a new panel, presets and math: seeded random numbers place the portals (a seed gives the same pattern), each composited with its transparency and a border. Projects that used it start it again from its defaults.
- The Gradient Wipe transition follows new math (the gradient in 8-bit steps at the frame's pixels, its stretch, flips, mirrors and blur, its ramp) and its panel's order, and now runs on the graphics card. Projects that used it start it again from its defaults.
- The Page Peel, Page Roll and Page Loop transitions remade with new panels, presets and math: the page bent around a cylinder layer after layer (Page Roll's four turns, Page Loop's rising paper and loop), lit by the Light color, and its edges smoothed. Projects that used them start them again from their defaults.
- The 3D Blinds, 3D Cascade, 3D Fly In/Out and 3D Shuffle transitions remade with new presets and 3D drawing: the pictures as cards in space seen by a camera, each pixel covered exactly and its color averaged over the pixel's footprint, lit by two lights, the cards moving along keyframed paths. Projects that used them start them again from their defaults.
- The Film Grain effect remade with a new panel, presets and math: tables of normal random numbers stretched over the frame by Granularity, the same grain for every color or one each by Chromaticity, each color times its grain by Amount; it runs on the graphics card. Projects that used it start it again from its defaults.
- The Light Rays effect remade with a new panel (Light source, Tint color and its three groups), its eleven presets and its 8-bit math: rays drawn from the light outward through what is already drawn, weaker where they are darker than the picture, the bound's ellipse, the noise, the tint and the blend; Radius X, Radius Y and Feather grey out while Bound radius is off. It runs on the graphics card with the same integer math. Projects that used it start it again from its defaults.
- The Gradient Map effect remade with a new panel: a gradient bar with a marker for each color stop — click a marker to pick it and edit its color, drag it to move it, click the bar to add a stop, drag a marker below the bar to remove it — plus Amount and Mode (Blend, Add). The picture's tones follow the gradient through a smooth curve between stops. It runs on the graphics card. Projects that used it start it again from its defaults.
- The Film Effects effect remade with a new panel (Grain, Tint, Color, Convert to grayscale and the Enable particles group with Type and Amount), its four presets and its grain, tint and grayscale math; its particles (dust, flicker, hairs, jitter, scratches) are BananaV's own. It runs on the graphics card. Projects that used it start it again from its defaults.
- The TV Simulator effect remade with a new panel, its two presets (TV Look, Bad Sync) and its layers: the picture rolling with a poor vertical sync, lines torn with a poor line sync, the aperture grill's stripes, the scanlines, the scan phasing band, the static and the phosphors' glow. It runs on the graphics card. Projects that used it start it again from its defaults.
- The Newsprint effect remade with a new panel (Dot size; Type: Color, Monochrome, Offset color), its six presets and its halftone: the picture averaged in cells laid out in brick rows, a dot in each as big as the cell is dark, printed on white paper with smooth rims; below a dot size of 0.15 the picture shows through. It runs on the graphics card. Projects that used it start it again from its defaults.
- The Noise Texture generator remade with a new panel (Noise style, Number of layers, Swap colors, the two colors and the Frequency, Offset and Noise parameters groups), its seven styles and twenty presets, and its layers of noise mapped onto the two colors. It runs on the graphics card. Projects that used it start it again from its defaults.
- The Checkerboard generator's settings show their groups (Tile dimensions, Edge blending, Grid offset), as the Noise Texture generator's do
- The Lens Flare effect remade with a new panel (Light color, Light position, Lens type, Tint, Intensity, Blend, Size, Perspective), its five presets and its five lenses' elements laid out along the line from the light through the frame's middle, sized by Size and Perspective and tinted toward the light's color. It runs on the graphics card
- The Solid Color generator remade with a new panel (Color, transparent at first) and its ten presets, gray and orange at 128 of 255; the Media Generator's Solid Color now makes it. It runs on the graphics card
- The Looks effect laid out anew (Look: none first, then the looks; Strength) with its engine: each look a 16 × 16 × 16 color table read between its points, the picture moved toward it by Strength. It runs on the graphics card
- The Color Match effect remade with a new panel (Strength, Match brightness, the Source, Target and Result images), its seven presets and its measure: the images' colors in L*a*b*, each pixel's a and b (and its lightness with Match brightness) moved from the target image's means and spreads to the source image's; it says when an image is a solid color, varies too little, differs too much or the target image is still missing. It runs on the graphics card
- The Defocus effect remade with a new panel (Radius, Iris shape: circle, pentagon, hexagon, heptagon or octagon, Orientation, Disc curvature, Bloom threshold and strength, Reduce flicker), its three presets and its engine: the lens's iris drawn with its light from rim to middle, the brightest lights blooming, and the picture convolved with the iris through the frequency domain, whatever its size. It runs on the graphics card
- The Starburst effect remade with a new panel (Threshold, Brilliance, Strength, Radius, Points, Center burst, Aspect, Orientation, Soften edges, Reduce flicker, Effect only), its two presets and its engine: a star of thin rays, with its center burst of eight long and 27 short rays, convolved through the frequency domain with the picture's lights. It runs on the graphics card
- The Soft Contrast effect laid out anew (its units, the Vignette group with Exterior effect, Strength, Shape, Softness, Width, Height, the blurs, Corner radius and Position) with its three presets; its glow and its vignette's blur now use the exact Gaussian. It runs on the graphics card
- New Rays effect, laid out anew (Key alpha channel, Threshold, Amount, Position, Aspect, Boost, Peak, Noise, Hue, Hue sweep, Saturation, Quality, Reduce flicker, Effect only, and the Mask group: a rectangle, bar, wipe or ellipse on the effect, its lights, or shown alone) with its two presets: rays of light streaming from the picture's bright parts away from a light. It runs on the graphics card
- The Glint effect laid out anew (Threshold, Boost, Horizontal and Vertical radius, Hue, Hue sweep, Saturation, Orientation, Streaks, Reduce flicker, Effect only, and the Mask group) with its two presets; its colored streaks now come from a kernel convolved through the frequency domain. It runs on the graphics card
- Levels' gamma follows an exact curve, and Gaussian Blur, Unsharp Mask, Linear Blur, Glow, Fill Light and Black Bar Fill blur with an exact recursive Gaussian, with radii (each a part of the frame's longer side); Unsharp Mask sharpens alpha too, keeps a pixel whose sharpened value stays within Threshold of the blurred picture, and its Amount goes from −100,000 to 100,000; Gaussian Blur's Extreme Blur preset is 0.19
- The graphics card's blur now keeps wide blurs as accurate as the processor's
- Sharpen's weights, Vignette's rotation, pixel shape and colors (the alpha kept, every channel held within 0–1), Pixelate's cells, Crop's window (each pixel kept or not, turned clockwise) and Mirror's slanted lines follow exact math
- The Motion Blur effect remade with a new panel (Quality: Coarse, Medium, Fine; Blur Mode: Fixed Blur or Motion-adaptive Blur, each showing its own Blur Amount; Motion Threshold (min) and (max); Maximum Blur Amount; the Visualization group) and its math: the moving areas blurred along the frame's mean motion by a line kernel turned to it. It now runs on the graphics card too. Projects that used it start it again from its defaults
- The Slow Motion effect laid out anew (the Parameter group with Quality, Blending and Playback Rate — 1 at first, up to 100 —, the Visualization group, Adjust Event Length) and the Warp Flow transition with its Method and Blending; both make their frames between (each picture moved along the motion, mixed, or the nearer one moved with Nearest Frame) and now run on the graphics card. Projects that used them start them again from their defaults
- The Flicker Control effect remade: its panel (Mode, Flicker Frequency, Reduction Mode, Analysis Window, Sensitivity, Type, Use motion compensation, Calculate, Reset, Show mask only), its messages and its five presets; Temporal blend weighs the frames around each one, Histogram smoothing matches each color channel to the window's histograms, which Calculate now counts, and Addition adds a flicker, a sine or a random one. It runs on the graphics card. Projects that used it start it again from its defaults
- The Denoise effect remade: its panel (the Noise Reduction group with Luminance and Chrominance, 3 at first, and the Sharpening group with Amount), without presets, and its math: the picture's luma and colors cleaned apart by non-local means and then sharpened by a small kernel. It runs on the graphics card. Projects that used it start it again from its defaults
- GL Transition remade: its code box (a crossfade at first) and 62 presets, each one the GL Transitions collection's code with its settings named and ranged as the code says (the code hidden); the GLSL you type runs as well, with arrays, macros and structs, and now on the graphics card too. Projects that used it start it again from its defaults
- Lens Correction remade: its panel (Lens Model's Search for and Found Lenses, Distortion Center, k1 to k3, Scale and Show Distortion), a list of 636 lenses, their distortion from the lensfun project's open database, and its rules (a search lists the lenses whose names have its words; picking one sets k1 to k3 and moves Distortion Center), and its math: the PanoTools lens model, or the radial one, each pixel read between four. Show Distortion leaves the picture as it is and draws over the preview the grid the settings bend. It runs on the graphics card. Projects that used it start it again from its defaults
- Mesh Warp remade: its panel (an 8 × 8 grid, Lock, the Mesh Behavior group with Elasticity and Fixed Border Points, Reset Mesh), its rules (the grid's size changes only while the mesh is untouched and not animated) and its math: each cell split in two triangles that carry the picture to where its points went, the picture read between pixels. Dragging a point keeps it between its neighbours; Elasticity pulls the others along, holding Shift only its neighbours; a double click puts a point back. It runs on the graphics card. Projects that used it start it again from its defaults
- Video Stabilization remade: its panel in three views (Basic: Stabilize and Reset; Professional and Expert: Method, Motion Compensation with Warp, Stabilization options, Smoothing Parameters, Borders, Motion Blur, Visualization) and its rules — which groups show and which grey out before and after an analysis —; the Expert view's region of interest, grid of cells, points per cell and tracker type (Lucas-Kanade, Sum of Absolute Differences, Hybrid) now steer the analysis, and the region is drawn and dragged on the preview; the camera's moves smoothed by a window as long as Smoothing says, or frozen at the reference frame (chosen as the one needing the least zoom), Auto Zoom, the picture read with its four edge fillings, and the motion blur of horizontal pans; on the graphics card too.
- New Stabilize effect, a simpler stabilizer: Pan smoothing, Stabilization amount, Rolling shutter correction and Apply, with its three presets (Light, Medium and Heavy Stabilization); a changed setting waits for Apply; on the graphics card too.
- 360° Video Stabilization remade (Stereoscopy, Analyze and Clear Motion Data, Exclude Views, Smoothing in frames), and a new Scene Rotation effect that turns a 360° picture about its three axes, each eye of a stereoscopic picture apart; both on the graphics card.
- Dual Fish Eye Stitching remade: its panel (Field of view, each fisheye's orientation, Image Vignetting, Refinement's Center, Radius and Rotation), its five cameras' presets, the overlap of wide lenses mixed and the rims' vignetting taken out; on the graphics card.
- New Stereoscopic 3D Adjust effect: Horizontal offset, the Corrections (Vertical offset, Zoom, Keystone, Rotation, Auto correct, the eyes' flips), Automatic crop and Crop, and the Floating windows, each correction split between the eyes; on the graphics card. Projects are not stereoscopic yet, so it shows the left eye's picture.
- New Smart Zoom effect: a window around a point dragged on the preview, zoomed up to 4 times by a high quality Lanczos resampling, with an optional enhancement of the detail that leaves noise alone; on the graphics card.
- Effects' points have handles on the preview again: wherever an effect draws none of its own, each of its positions (Spherize's, Swirl's, Pinch/Punch's, Radial Blur's and the transitions' centers, the Rays' position and others) is a handle you drag; Lens Flare's light, Offset and Wrap's offset, Bump Map's light and Lens Correction's distortion center have handles of their own.
- New AI Smoothen, AI Sharpen and AI Dehaze effects in the AI/ML category, with their panels (Details and Amount; Dehaze's Artifact Reduction: Border, Gain, Offset): the picture worked out on a small copy and brought back to full size by an edge-aware guided filter; on the graphics card.
- Titles & Text uses the font's own kerning and styles the selected text on its own; Credit Roll imports and exports files.
- A redesigned Color Grading panel with its Color Curves, and HSL Curves with tangents and a points menu.
- BananaV's own transitions and Color Grading run on the graphics card.
- Redesigned sliders in the FX window, a double-click to put a setting back at its default, and lighter temporary effects.

### Audio

- Input monitoring, input busses sending to assignable effects chains, punch-in and loop takes.
- Pitch changes on audio events.
- **Plug-in delay compensation** on tracks, on events and along the busses.
- VST 3 presets in the FX window, automation of their parameters, and their editors in windows of their own on Linux.
- Busses on the sound card's outputs, and an audio device selector on the master bus.
- The Modern Equalizer's analyzer, peak meters and oversampling.
- **A redesigned Mixing Console**, with channels dragged into place, ganged edits and a sends divider.
- Mackie Control with extenders and its mapping, presets from a V-Pot, and busses on the strips.

### Import, export and discs

- **Final Cut Pro 7 XML** read and written in full; FCPXML brings speeds, titles, compound clips and multicam clips.
- MacCaption (`.mcc`) captions, and an `.scc` caption file written beside a render.
- Smart rendering of ProRes, without recompressing what did not change.
- Renders through plug-in effects draw two frames at once on the graphics card: a 1080p60 render through a heavy third-party effect went from 22 to 32 frames a second.
- Rendering gets the computer to itself: playback stops when a render starts, and the interface redraws only a few times a second while it runs (it used to redraw at every frame rendered).
- BananaV's own MPEG-2 encoder, with B-frames, and MPEG-1 for Video CDs.
- DVD-Video authoring, DVD burning and audio CD burning (Track-at-Once and Disc-at-Once).
- **Screen capture** of displays, windows, cameras and the computer's sound.
- Project archives include nested projects, and Save As can copy only the parts of the media that are used.
- Media changed on disk are read again by themselves, and MXF and DV files' start timecode syncs events.

### Interface

- BananaV opens as you left it: window layout, bars and view options.
- Effects' on-screen controls carry on past the frame's edges into the preview's margin, where they can still be seen and grabbed.
- Plug-ins can be dragged straight from the Video FX, Transitions and Media Generator lists onto the timeline, with their default preset.
- With snapping on, resizing the Pan/Crop or Track Motion box puts its edges on the grid.
- Audio output failures are written to `Audio.log`, and hovering "Audio: unavailable" tells why.
- The welcome screen opens in front of the program, and its Getting Started page leads to BananaV's website, GitHub, Discord and donations: Ko-fi from any country, PIX from Brazil.
- BananaV's official logo takes the place of the banana icon in the toolbar, the welcome screen and the Explorer's project files.
- The menus sit in BananaV's own title bar, beside its icon. The timeline's track list reaches down to its bottom with the Rate there, the transport bar starts beside it, and the time boxes at its right show the cursor, or the time selection's start, end and length (double-click one to type a time, or + or − and a length).
- Track headers can show every button (Edit Visible Button Set: Bypass Motion Blur, Track Motion, Track FX, automation, Mute, Solo, the compositing ones, Min/Max, Color Grading); the buttons that don't fit go from the left as the track list narrows, and the More menu (⋯) holds the commands of those not shown.
- The interface draws through DirectX 12 on Windows and moves smoothly between monitors on different graphics cards.
- The Video Preview works in narrow windows, the transport buttons that don't fit going to its More menu.
- Full-quality snapshots of the preview.
- Docking areas with F11, docked windows that maximize and restore, and Delete and an address trail in the Explorer.
- The Trimmer's monitor, its frames along the time bar and hover scrub.
- The Trimmer laid out anew: the history with its buttons, the picture or a sound's ruler, waveform and scroll bar, a loop bar with the media's numbered markers, the transport and time boxes in the middle, and right-click menus for the picture, the waveform, the loop region, the bar and the ruler's time formats. Space, Shift+Space, Enter, Ctrl+Home, Ctrl+End, Shift+L, Ctrl+P and Ctrl+Shift+Up/Down work in it; a subclip opens on its own; Copy takes the selection to the clipboard.
- The Trimmer's loop and time bars redrawn to scale, with numbered marker flags (on the timeline's marker bar too), and its playback smooth: frames drawn ahead and the window redrawn every frame while it plays.
- Project Media's right-click menu now holds Detect Scenes (added to the timeline or as subclips), Explore Containing Folder, Swap Video Files, Remove from Current Bin, Remove from Project and Delete File(s), Rename, Replace…, Refresh, Rotate 90° and Lay Out Tracks by timecode or date.

### Known issues

- Burning DVDs and audio CDs has not been tried with a real burner yet.
- Many of the new features are still being checked against how video editors usually behave: tell us when one surprises you.
- Pan/Crop does not travel in Final Cut Pro 7 XML yet.

## 0.1.0 Ultra Alpha 1 — 2026-10-23

The first public test build: BananaV for Windows 10 and 11 (x64 and ARM64)
and Linux (x64 and ARM64, as AppImages), the packages only.
