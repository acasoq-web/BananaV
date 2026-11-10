# Changelog

What changed in each public build of BananaV. The
[Releases page](https://github.com/acasoq-web/BananaV/releases) has the
downloads, and each release's notes say how to install it.

## 0.3.0 — 2026-11-10

For Windows and Linux (x64 and ARM64) and, experimental, macOS (one app
for Apple's chips and Intel).

### Highlights

- **macOS, experimental.** One app for Apple's chips (M1 and later) and
  Intel Macs, macOS 11 and later, with video on the Mac's media engine,
  the interface on Metal and plug-ins from the Mac's folders. It is built
  and started on GitHub's Macs but not yet tried by us on our own: please
  tell us what works and what does not.
- **Smooth playback, 4K included.** A 4K 59.94 project at Best (Full)
  plays every frame, the interface taking a third of the time it took;
  heavy 4K parts no longer stall when they start, and picture and sound
  start together after a jump.
- **VST 2 and VST 3 plug-ins**, their editors inside the FX window,
  32-bit ones in a bridge of their own, each tried in a process of its own
  so that one that crashes never takes BananaV with it.
- **Starts that used to end early** (a graphics driver failing on
  DirectX 12) now switch to Vulkan or OpenGL by themselves.
- **Transform on the preview**: drag the picture to move, resize and turn
  it, with a Transform window beside the Explorer holding every control
  of the event.
- **Windows dock where you drop them**, as in other editors, or float in
  windows of their own on another monitor.
- **Updates from within BananaV**, and **18 more interface languages**,
  partly translated.

### macOS (experimental)

- Experimental: the app is built and its start checked on GitHub's Macs,
  but we have not tried it on Macs of our own yet. Things may be missing or
  wrong; tell us on Discord or in GitHub's issues. Intel Macs have been
  checked less than Apple's chips (GitHub's are virtual machines): some
  effects drawn on the graphics card may look slightly different there,
  and screen recording may not work yet.
- BananaV runs on macOS 11 and later, one app for Apple's chips (M1 and
  later) and Intel Macs: a disk image to drag into Applications.
- Video is decoded and encoded on the Mac's media engine (H.264 and HEVC,
  "Apple VideoToolbox" in the Render window), the interface drawn with
  Metal, sound through Core Audio.
- The Command key does what Ctrl does on the other systems (Cmd+S saves,
  Cmd+click picks more), a Control-click is a right click, and the menus
  and tooltips say Cmd and Option. Quit (Cmd+Q, the Dock's) asks about
  unsaved changes first; projects open from Finder.
- VST 3, VST 2 and OpenFX plug-ins from the Mac's plug-in folders, their
  editors inside the FX window; a plug-in that crashes is switched off
  without taking BananaV with it. Intel-only plug-ins run with BananaV
  opened through Rosetta.
- The eyedropper uses the system's color sampler; the render's progress
  shows on the Dock's icon.

### Playback

- Smooth playback again, 4K included: the preview kept up at 40 frames a
  second where it played 60 before (a 4K trailer at Best (Full)) — the
  interface redid work at every frame (the LUT looks' tables written out,
  each preview frame converted pixel by pixel) and video thumbnails
  decoded during playback. Now a 4K 59.94 project at Best (Full) plays
  every frame, the interface taking a third of the time it took. Frames
  come back at the size the window shows them (drawn at full size, their
  effects unchanged; reduced on the graphics card), their memory used
  again, thumbnails wait while playing or rendering, and a second drawing
  thread is only tried where it helps: no more stalls of a second when a
  heavy part of a 4K long-GOP recording starts playing.
- Picture and sound start together: when the preview still decodes the
  first frame (a 4K recording with keyframes seconds apart takes a moment
  after a jump), the sound waits for it instead of playing over a frozen
  picture. Jumps forward within the same group of pictures go on decoding
  instead of starting again at the keyframe before (MP4 and MOV).

### VST plug-ins

- VST 2 plug-ins work too, through BananaV's own compatibility (written
  without Steinberg's VST 2 kit): they are tried in a process of their own
  as VST 3 ones are, offered in the Plug-In Chooser's VST folder, run in the
  mixer with automation, the timeline's place and tempo, their programs in
  the FX window's preset list, `.fxp` and `.fxb` preset files, and their own
  windows.
- A VST plug-in's own editor shows inside the FX window (Windows):
  under the chain and the preset bar, the window as big as
  the editor; a 32-bit plug-in's too, from its bridge. The preset bar has
  the plug-in's programs and Open VST Preset, Save VST Preset As, Open VST
  Bank and Save VST Bank As; the chain's button says its kind and width
  ("(VST3, 64 Bit)").
- 32-bit plug-ins, VST 2 and VST 3, run in BananaV's 32-bit bridge, a
  process of their own (Preferences › VST Effects: "Enable support for
  32-bit VST plug-ins", on by default): a crash or a hang there ends the
  bridge only, the sound goes through and BananaV says so. Their windows
  open from the bridge. 64-bit plug-ins run in BananaV itself, as before.
- Projects of the other editor bring their other VST 2 effects with their
  programs, found among the plug-ins installed here by their own number.
- Each VST 3 plug-in is tried in a BananaV process of its own when it is
  found — loaded, set up and given a block of silence — so one that
  crashes or hangs costs that process, never BananaV; several are tried
  at once, and what they said is kept until their files change, the known
  ones listed at once. Plug-ins that failed are listed in Preferences ›
  VST Effects with why, and never offered.
- Preferences › VST Effects: the effects' boxes work (an effect left out
  is no longer offered, wherever it is installed), the list says how the
  search goes and how many failed, and Refresh looks at every plug-in
  again. Effects with the same name are told apart by their file's name.
- A plug-in that crashes while BananaV runs is turned off until BananaV
  restarts, BananaV says so, and the sound goes through it untouched.
- The FX window's preset list has the plug-in's own programs, then the
  presets saved in BananaV; Save VST Preset… and Open VST Preset… keep a
  plug-in's settings in the standard preset file (`.vstpreset`), in the
  standard folder other programs look in.
- Plug-ins see where the timeline plays, its tempo and its meter
  (tempo-synced delays and LFOs follow the project), and FX automation
  reaches them sample by sample instead of once a block.
- A plug-in is kept when playback jumps elsewhere (its sound cleared): no
  wait while it starts again. One that has been silent in and out for
  longer than its tail sleeps until sound comes again, sparing the
  processor.
- Projects of the other editor bring their VST 3 effects with their
  settings: each is found among the plug-ins installed here (by the
  number its class makes and its name's and file's checks); those not
  installed are counted when the project opens, their sound going
  through untouched.

### Starting

- A start that closes before BananaV settles (a graphics driver failing
  on DirectX 12) makes the next one draw through Vulkan, then OpenGL; the
  one that works is kept. On Windows BananaV starts again by itself on the
  next one. `Crash.log` in the logs folder says what happened.

### Transform

- Transform mode (the button by Preview Quality): the event at the cursor
  gets a box on the picture — drag inside to move it, a corner or a side
  to resize it (Alt: about its middle), just outside a corner to turn it.
  It writes the event's Pan/Crop (the keyframe at the cursor when it has
  keyframes).
- The Transform window opens beside the Explorer, Project Media and the
  other browsers, with every control of the event: Motion (position,
  scale — uniform or not —, rotation, crop, flips, anchor point, the
  Pan/Crop box, keyframe curve and smoothness, the source settings),
  Opacity with the fades, Time (playback rate, undersample, loop, hold
  last frame, reverse) and its audio's volume, channels and pitch; a
  stopwatch animates the framing, arrows and a diamond go to, add and
  remove keyframes, and the keyframes sit on a strip of the event's time
  where they drag. Reset buttons on every row.

### Workspace

- Interface brightness (Options › Preferences › Display): the interface
  type's greys lighter or darker, up to 40 %, shown as the slider moves.
- Windows dock as in Premiere and Resolve: dragging a tab lights up the
  room it would take (the middle of a window: a tab beside it; a side: a
  window of its own there). A tab's menu has Float in Its Own Window: a
  window of the system's own that goes to another monitor, with Dock to
  put it back; BananaV opens with them as they were.

### Updates

- BananaV looks for a newer version as it starts and asks: Update, Cancel
  or Don't Show Again (Options › Preferences › General turns the check
  back on). Help › Check for Software Updates… looks at any time. On
  Windows, Update downloads the installer, checks it against GitHub's
  checksum and runs it as BananaV closes; elsewhere it opens the download
  page.

### Languages and letters

- Ten more partly translated languages: Turkish, Traditional Chinese,
  Spanish (Latin America), Thai, Ukrainian, Italian, Czech, Hungarian,
  Portuguese (Portugal) and Swedish, as the eight before them; Thai and
  Traditional Chinese letters come from the computer's fonts.
- Eight more interface languages, partly translated: Simplified Chinese,
  Russian, Spanish (Spain), German, Japanese, French, Polish and Korean.
  The menus, the main windows, the timeline and the Render window are
  translated; the rest shows in English. Preferences and the welcome
  screen mark them "partial translation".
- Chinese, Japanese, Korean and other scripts no longer show as boxes:
  the interface (captions, names, text boxes) draws them with the
  computer's fonts, and Titles & Text draws a letter its font lacks from
  a font that has it (Japanese text with Japanese shapes, Korean with
  Korean ones).

### Media generators

- Checkerboard, Color Gradient, Noise Texture, Solid Color and Test
  Pattern were rebuilt: in 8-bit projects their pictures match, pixel for
  pixel, reference renders of the same settings (125 frames), and they
  draw on the graphics card from the same tables (Noise Texture and Color
  Gradient a level apart at most here and there).
- Color Gradient has three shapes (linear, elliptical, rectangular) and up
  to nine color points on its angle's axis, with eighteen presets; Test
  Pattern its nine patterns as whole bytes; Noise Texture its seven styles
  and twenty presets.
- Credit Roll has sixteen ways in and out, four ways to group a timed
  sequence's lines, a connector between a dual line's sides and each
  side's snapping, and eight presets; sizes count points for a frame 480
  lines tall and the room above and below lines is a part of the frame's
  height (older projects are converted). Its lines fall where reference
  renders put them.
- Titles & Text can crop its background to the text (a box with rounded
  corners).
- Titles & Text's panel has the text box's bar above it (typeface, size,
  bold, italic and justification) and an Animation list; its presets are
  the plain title, Title01 to Title25, twenty-five animated ones (Bounce,
  Earthquake, Fly In, Slide Up, Twist In…), Placeholder and Subtitles.
  The animated presets play under the pointer in the Media Generators
  window.
- Titles & Text's fifty animations play over the generated media's
  length: the text, its lines, words or letters fly, drop, bounce, pop,
  turn, scroll along a path, fade, blur and glow in and out, one after
  another.
- Titles & Text counts its settings in parts of the frame, the same at
  any frame size: sizes in points of a 200th of the frame's height (a new
  title is white 48-point Verdana, "Sample Text"), tracking in twentieths
  of its height, the outline a stroke along the letters' edges (half of it
  outside them) and the shadow's offset and blur in tenths of the frame's
  width, down and to the right at first. Older projects' titles keep
  their look: their values are converted when they open.
- Titles & Text's lines are as far apart as their fonts ask (their line
  gap counted), letters land half a pixel nearer where reference renders
  show them, and letters of a see-through color cut the background out,
  showing what is below. Text running past the frame's right edge no
  longer leaves a streak on the next row of pixels.
- Color Gradient's panel has a Control Points box: the gradient over the
  frame with its center and numbered points to click and drag, a bar that
  adds, deletes, picks and reorders points, and the picked point's own
  settings below it.
- Credit Roll's panel keeps the credits above two tabs, Properties and
  Styles; Styles shows one line style at a time (Header, Single Item or
  Dual Item), picked by name.
- Media an older built-in generator made (solid colors, color bars,
  checkerboards, gradients) open as today's Solid Color, Test Pattern,
  Checkerboard and Color Gradient: their panels, presets and place in a
  `.veg`. The demo project and Custom Color… make them too.
- Text set in Arial, and in other families with narrow faces, no longer
  comes out in the narrow face; lines of text are laid out on their fonts'
  cell heights.
- Plug-in generators draw in the values the project composites on: in
  8-bit projects a 50 % gray Solid Color shows 128.
- Reduce Interlace Flicker blurs only in interlaced projects.

### Effects

- The Plug-In Chooser has a search field, and letters typed over its list
  pick the plug-in they begin, as in a file manager.
- In the Pan/Crop window, the arrow keys move the box once it is clicked
  (a pixel of the media, ten with Shift).
- Transitions follow the measured model: with a transition the fade's
  curve counts for nothing (the progress is the time through the fade, or
  the transition progress envelope's value), and at an event's end the
  event goes out as the transition's first picture.
- A crossfade without a transition sums the two events, the coming one by
  its fade-in curve and the going one by that fade-in's reciprocal curve,
  on the processor and on the graphics card.
- One transition progress envelope covers both ends of an event.
- OpenFX plug-ins can put several controls on one line (OpenFX's
  no-new-line layout hint) and show their pages as tabs (BananaV's
  `BananaVParamPropTab` extension).

### Timeline

- The loop region is clearer: its bar in a grey of the interface's own
  (lighter under the pointer), the region a lighter strip between yellow
  handles and, while the time selection shows, yellow lines with a dark
  edge down to the timeline's bottom, the tracks lighter within it and the
  room under them blue. Clicking elsewhere on the timeline lets go of the
  time selection — the blue area and the lines go — while the loop region
  stays in its bar for Loop Playback. A double-click on the bar takes the
  loop region away (with none, it selects the whole project), at the
  system's double-click speed; so does a right-click on it.
- Options › Envelope Appearance… (also in an envelope's shortcut menu):
  the envelopes' line width, how near a click takes a line or a point,
  the points' size and shape (diamonds or squares), with a live sample to
  try them on.
- Two double-clicks in quick succession on an envelope add two points (the
  second was lost); double-clicks follow the system's speed everywhere.
- Events dragged below the tracks each go to a new track: the picked ones
  with the one under the pointer, one new track for each track they were
  on, in one undo.
- Media let go below the tracks goes on new tracks, even when there are
  tracks it could go on.
- A transition let go on the second half of a video event goes out with
  it, over its fade-out (or becomes the crossfade of the event right after
  it). Its label, and those over fade-ins with a transition, has the
  Transition Properties button, as crossfades' labels do.
- The red box of an event out of sync moves below the transitions'
  labels instead of hiding under them.
- Thumbnails no longer show another video's picture after they are made
  again (files changed on disk, proxies).
- About and the window's title give the version ("Untitled — BananaV
  0.3.0").

### Envelopes and internal preferences

- Shift and a drag on a track envelope's line paint it: each place the
  pointer goes puts a point and takes away the ones it passes over (going
  back paints over them too); the stroke starts where it was pressed with
  the value the envelope had there. Let go, it is thinned to its ends, its
  peaks and valleys and the plateaus that matter, joined by smooth curves
  (Preferences › External Control & Automation: "Smooth and thin
  automation data after recording or drawing"). With Ctrl or Alt held too,
  Shift puts a point to drag instead. Over an envelope the pointer stays
  the arrow until it is pressed.
- Envelopes' values follow the lane's height a pixel in from its top and
  bottom.
- A video track's Level slider: Ctrl (or the right button) moves it in
  fine steps of a tenth of a percent, the pointer kept on the knob; the
  wheel moves it a percent a notch (a tenth with Ctrl); Shift no longer
  changes it. Its undo is named Track Overlay Level Change.
- Options opened with Shift held has Internal… at its end: every
  preference of BananaV in one list (Name, Value, Default), changed by
  typing; the search box keeps the internal ones (Enable Envelope
  Painting, Thinning gap threshold, Dim Attenuation, Minimum Video Render
  Threads, Number of Control Plug-ins allowed). Values that do not read or
  fall outside their range are left as they were.
- New internal preferences: Dim Output's cut, the range of Video's
  rendering threads (now up to 48 by default), and how many control
  surfaces can be added.

### Pointers

- BananaV draws its own mouse pointers, about ninety, sharp at any screen
  scale and shown by the system as its own (in dialogs too): trimming an
  event's start or end, stretching its time, slip-trimming, slipping and
  sliding it, sliding a crossfade, trimming the adjacent event, splitting
  and trimming, fades, an event's level line and snap offset, envelopes'
  lines and points (and painting them), scrubbing, the Envelope, Selection
  and Zoom tools, the timeline's dividers, scroll bars, markers and
  regions, Pan/Crop's and Track Motion's handles (turning with the frame),
  masks' pens, color droppers, the wheel's scrolling and more.
- The pointer over an event tells what pressing there does with the keys
  held, and the press does it: Ctrl on an edge stretches the event's time,
  Alt slips (on an edge, slip-trims), Ctrl+Alt slides (on an edge, trims
  the adjacent event; on a crossfade, slides it), Ctrl+Alt+Shift splits
  and trims — in every edit mode, the keys now working in the Slip, Slide,
  Stretch and Split Trim modes too. Without keys, each mode does its own
  on every part of an event: Slip's edges slip-trim, Slide's trim the
  adjacent event, Stretch's middle moves the event, Split Trim's splits and
  trims while dragged. Tooltips name what dragging does (Time Stretch, Slip
  Trim, Trim Adjacent, Split Trim, Slide Crossfade).
- The time selection's edges can be dragged over the tracks; a scrub
  (Ctrl+drag, by the edit cursor too) leaves a mark where it began; the
  wheel's scrolling marks where it began and points where the view runs.
- In the Pan/Crop window the Zoom tool's right button zooms out by half,
  and with the normal tool a drag far from the box moves the view.
- What is dragged over a window that takes nothing shows it can't be let
  go there.

### `.veg` projects

- Generated media come in and go out: all seven generators with their
  settings and keyframes, their length, frame size, pixel aspect, fields,
  alpha and rotation (Color Gradient's and Credit Roll's own records, and
  Titles & Text's rich text, too). A generated media with a length of its
  own times its animation by it.
- Transitions at events' ends, the fades' reciprocal curves and the
  transition progress envelopes of both ends come in and go out.

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
