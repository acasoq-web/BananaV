# BananaV roadmap

What is done, what is being worked on and what comes next. VEGAS screenshots
sent by the project's maintainer — "the user" below — are the spec for each
piece of interface.

## Done

- **Foundation** — ticks (254,016,000,000/s), frame rates and timecode,
  bitstream, deflate/zlib, PNG and WAV/RF64 written from scratch, color science
  (transfer curves, Y'CbCr matrices, primaries, linear light).
- **Media** — Media Foundation decoding and encoding on Windows (validated
  against ffmpeg), generated media, thumbnails and waveforms in the
  background.
- **Model and editing** — tracks, events, groups, undo history with gestures,
  split/trim/slip/slide/stretch, ripple editing, fades and crossfades, quick
  fades, markers and regions, track envelopes (volume, pan, mute), event
  switches (mute, lock, invert phase, normalize, channels), interchange (EDL
  import/export, FCP7 XML and FCPXML import and export, OpenTimelineIO
  import and export).
- **Processing** — CPU compositing in linear light, video FX chains (event,
  media, track, output), Pan/Crop with keyframes, transitions, audio mixer with
  envelopes, EBU R 128 loudness (integrated, LRA, momentary, short-term).
  Pictures (PNG, JPEG…) and the colors picked for generators and Fade to
  Color take video's values, as VEGAS does: a still shows its file's values.
- **OpenFX plug-ins** — an OFX 1.5 host: plug-ins found in the standard
  folders and the user's (each binary described in a process of its own,
  cached), effects on events, media, tracks and the output, transitions,
  generated media; their settings as the plug-in describes them, with
  keyframes (VEGAS's keyframe controller), presets and their messages;
  overlays drawn with the draw suite over the Video Preview; temporal clip
  access; on the processor and in frames composited on the card. A
  plug-in's settings look like VEGAS's (the bar with About and help, a
  slider with its number, the Animate clock, a position's box; one, two or
  three columns) with VEGAS's keyframe controller under them, its height
  set by the bar between them: a row per parameter and per number of a
  position, Lanes and Curves, keyframes picked by click, Ctrl/Shift and a
  box (the cursor following it), moved, copied, cut, pasted, spaced, given
  a curve or a typed value, Sync Cursor; its scroll bar zooms by its ends,
  as the timeline's does. Parametric parameters (curves) are edited as
  VEGAS's Color Curves edits its own: Channel (the curves most share
  together — a curve of its own keeps it — or one), the other curves
  shown, points added with a double-click or the box's Create Point (the
  curve keeping its shape), picked with Ctrl and in runs with Shift,
  dragged together and deleted, Bézier handles locked or apart and put
  back level, the box's menu and buttons that make the others like one,
  and the bar of what the curves make of a ramp; curves animate as a
  whole, through the window's Animate, their keyframes in the effect's
  row. A parameter may have an interface of its own drawn with the draw
  suite (`kBananaVParamPropInteract`, BananaV's extension until OpenFX
  has one: the Color Corrector's wheels), under its row.
- **BananaV's own effects as OFX plug-ins** (`bv-effects`), as VEGAS's are:
  built into BananaV and buildable as `BananaV.ofx` for other OFX hosts
  (`apps/ofx-bundle`). Color Curves: the red, green and blue channels
  each through a curve, VEGAS's eleven presets (Warm Colors to Blues and
  Yellows), Auto Modes and Calculate reading the picture at the cursor
  (Color Balance, Color Levels, Contrast, Histogram Equalization).
  Color Corrector: VEGAS's three-way corrector — Low, Mid and High color
  wheels (angle and magnitude, a vectorscope's hues; a double click puts
  one back in the middle) with their eyedroppers (the color or its
  complement, picked on the preview), Saturation, Gamma, Gain and Offset,
  VEGAS's seven presets (Red Midtones to Remove Yellow Cast); the wheels'
  math measured on VEGAS's pictures of its presets, to the prints' noise
  (about an 8-bit step). Cookie
  Cutter: VEGAS's — fourteen shapes kept or cut away, the border's color,
  feather, repeats, size and the center (its handle on the preview),
  VEGAS's fourteen presets (Grate and TicTacToe among them). Layer
  Dimensionality: VEGAS's shadow (Drop, behind the picture away from the
  light, or Inner, inside its edge), glow (Outer, Inner, Edge or Dual),
  embossed surface (the picture or what is around it) lit by a light
  placed in its own box (a double click puts it back where it starts) or
  by its cross on the preview, and the picture's opacity, what its types
  turn off greyed out as VEGAS's; the shadows' distances and blurs and the
  glows measured on the pictures of VEGAS's presets, all eighteen of
  which it has. Pixelate: VEGAS's — Horizontal and Vertical pixelization
  (none at 0, then cells of two pixels up to the whole frame at 1, as
  many across and down as VEGAS's pictures hold), each cell the average
  of its pixels, VEGAS's five presets (Small to Medium Vertical); it took
  the place of BananaV's older Pixelate, which projects that have it
  keep. On the graphics card BananaV draws its own effects itself
  (the plug-ins' math as kernels, the picture never leaving the card),
  the same pictures as on the processor for every preset. A color parameter shows as VEGAS
  shows one (swatch, numbers, the picker under them with HSV, HSL, RGB or
  Lab and an eyedropper), groups as sections a triangle opens. Crop, the
  first: VEGAS's Crop (Anchor Point, X and
  Y Crop, Location, Angle, Opacity), its handles on the preview (edges,
  moving, turning); Location is the window's middle, 0–1 as VEGAS gives
  it, the anchor staying where it is when the crop changes; VEGAS's four
  presets (Crop Left, Right, Top and Bottom Half). LUT Filter: VEGAS's —
  Name, File (Browse…), Interpolation (Linear or Tetrahedral) and
  Strength — on `.cube` tables (`bv-lut`: 1D, 3D and Resolve's shaper
  before a lattice), a picked file naming the table after its title;
  VEGAS's has no presets, BananaV's eight looks are (Warm, Cool, Teal and
  Orange, Faded Film, High Contrast, Black and White, Bleach Bypass,
  Vintage). White Balance: VEGAS's — the neutral color (its eyedropper,
  shown in Lab), Temperature and Tint following it as VEGAS's numbers do,
  Amount of correction (how far the colors move against the light, set by
  itself to what takes the light's color to gray, as VEGAS's is),
  Brightness (the luma), Equalize Histogram (each channel stretched over
  the frame, red and green held under blue), the math measured on VEGAS's
  pictures; BananaV's seven lights to take away as presets (VEGAS's has
  none). HSL Adjust: VEGAS's — Add to hue (a whole turn is 1),
  Saturation and Luminance scaled in the HSL model as VEGAS's pictures
  show, with its five presets (Desaturate, Invert Color, Max Saturation,
  Half Saturation, Saturation and Luminance Boost). Picture In Picture:
  VEGAS's — Location, Angle, Scale in X and Y (a part of the frame's,
  their sliders logarithmic), Mode (Fixed Shape, Fixed Parallelogram,
  Free Form), Shear, Opacity and the four corner points kept together as
  VEGAS keeps them, with handles on the preview (the corners, which stop
  before the shape folds; the ring's handle, turning and scaling; the
  middle) and VEGAS's five presets (the picture a third big in each
  corner, and Middle Left, Scaled, Angled, Translucent), which keep the
  corners where they start as VEGAS's do. Saturation Adjust: VEGAS's —
  Amount, Center, Spread, Low and High: the colors' saturation changed by
  how saturated they are (each color moved toward or away from its luma,
  Amount around Center fading into Low and High, as VEGAS's pictures
  show), with its four presets (Boost Midtones, Reduce Minor Color Noise,
  Reduce Oversaturated Colors, Selectively Desaturate). Border: VEGAS's —
  Type (Blurred: the edges fade away; Beveled: four facets lit from
  Angle, tinted by Color; Solid: a frame of Color, its alpha too) and
  Size, the angle and the colors shown only for the types that use them,
  as VEGAS's pictures show, with its five presets (Soft Edge, Bevel,
  Solid White Border, Translucent Blue Border, Pyramid). Black Bar Fill:
  VEGAS's — the Foreground (Location, Scale, Shadow Blur) over the
  Background (Location, Scale, Blur), the picture enlarged and blurred
  behind itself so a portrait video fills a landscape frame, a crosshair
  for each on the preview, as VEGAS's pictures show, with its eight
  presets. An event's effects now work on its framed picture, after its
  Pan/Crop, as VEGAS's do by default (a media's on its own picture);
  VEGAS lets the Pan/Crop be dragged along the chain, BananaV not yet.
  Unsharp Mask: VEGAS's — Amount (typed past its slider, as VEGAS's
  presets go: up to 4, down to −3), Radius and Threshold, the picture
  sharpened against itself blurred as VEGAS's pictures show, with its six
  presets (Light, Medium, Strong, Extreme, Negative, Outline Edges); it
  takes the place of BananaV's older Sharpen. Add Noise: VEGAS's — Noise
  level, Monochromatic, Gaussian noise and Animate, each pixel's noise
  added and folded back past black and white as VEGAS's pictures show,
  with its five presets (Medium, Grainy, Extreme, Color, Screen). Dialogs
  dragged no longer draw the whole application at every step. Timecode:
  VEGAS's — the frame's time as the ruler shows it, in any of the ruler's
  twelve formats (Time, Seconds, Time & Frames, Absolute Frames, Feet and
  Frames, the SMPTE ones), white on a black box placed by its alignments
  and sized by Width and Height, as VEGAS's pictures show (Arial Bold's
  shapes through Arimo Bold, an open font), with its eight presets.
  Color Balance: VEGAS's — Red, Green and Blue pushing the colors toward
  each in the light parts, the middle tones or the dark parts (Type),
  Preserve luminosity keeping the channels' mean, as VEGAS's pictures
  and the calibration card show, with its six presets (Red Midtones to
  Blue Highlight). Color
  Corrector (Secondary): VEGAS's — the Color Corrector's corrections
  (Chrominance's wheel with its eyedroppers, Rotate hue, Saturation, Gamma,
  Gain, Offset, Alpha) on what its limits pick (luminance, saturation and
  hue, each with its Smooth; Show mask, Invert mask; Select effect range
  setting them from a click or a rectangle on the preview), as VEGAS's
  pictures show, with its six presets (Invert Color to Saturate Greens).
  The Color Corrector's Saturation, Gamma, Gain and Offset follow what
  the calibration card through VEGAS shows (Gamma: a power in BT.709's
  encoding of the luma; Offset: 8-bit steps), and Color Curves' Warm
  Colors matches it under ACES at both compositing gammas. Brightness
  and Contrast: VEGAS's, as the calibration card through each of its
  presets shows — Brightness first, then Contrast's spread from Contrast
  center, its 8-bit halves rounding down as VEGAS's —, with its nine
  presets (Brighter to Darker, Less Contrast, and Flat). Levels: VEGAS's
  too, as the card through its six presets shows (Brighten, Darken and
  Studio RGB to Computer RGB bit for bit) — the input range stretched and
  clipped, Gamma's video-camera curve, the output range. Black and White,
  Invert and Sepia: VEGAS's — the gray the plain average of the three
  colors, Invert's blend straight, Sepia taking the color away and then
  tinting with its presets' 8-bit colors (every value within a level of
  VEGAS's). Gaussian Blur, Sharpen, Vignette, Mirror and Glow: VEGAS's
  too, as the card through each of their presets shows — Gaussian Blur a
  true Gaussian whose sigma follows each range (both counted on the
  frame's width), the channels picked; Sharpen VEGAS's 3 × 3 kernel;
  Vignette's ellipse, its feather's curve and Invert; Mirror's reflection
  about a line through Center at Angle, its background past the picture;
  Glow the blurred brightness past Suppression, times Intensity, toward
  Color —, each window as VEGAS's (Gaussian Blur's ranges with four
  decimals and its Channels closed; Glow's Intensity up to 20), with
  their presets (Medium Blur to Yellow Glow; Medium, Light and Heavy;
  Edges to White; Reflect Top to Reflect to Lower Right; White Soft Glow
  to Green Highlights on Brights). In 32-bit projects BananaV's effects keep what
  passes white or black, as VEGAS's float effects do; 8-bit projects
  clip each effect's result, as VEGAS's 8-bit pictures. A double click
  puts a setting back at its default: on its slider, as VEGAS's, on a
  color's swatch, or on any setting's name (a position's, a list's, a
  check box's). The Video Preview's Copy Snapshot to Clipboard and Save
  Snapshot to File take the frame as a render draws it, as VEGAS's at
  Best (Full): the output's whole size at Best quality with every effect
  — BananaV's own, which are plug-ins, included —, the file a PNG with
  the frame's transparency, as VEGAS's.
- **Engine** — playback with the audio clock as master, background preview,
  rendering to files, loudness scan.
- **Window layouts like VEGAS 23's** — the timeline is one of the
  layout's windows, fixed in its place (no tab, never closed or floated),
  the others docked around it; View › Window Layouts' Default Layout,
  Audio Mixing, Color Correction (the preview over the scopes down the
  right side, the timeline over Color Grading), Source Window Layout and
  Timeline Top Layout as VEGAS's screenshots show them (Alt+D and D, A, C,
  S, T); saved layouts are files (Save Layout As: name, number, folder;
  Organize Layouts: the folder's and the menu's, Assign/Replace, Move
  Up/Down, Activate, Clear, Delete Layout), recalled with Alt+D and their
  number, saved with Ctrl+Alt+D and it, the one in use marked in the menu,
  Save Layout and Reload Selected Layout for it. BananaV opens as it was
  left, as VEGAS does: the windows' layout, the toolbar and status bar
  shown or hidden, and the View menu's timeline switches (what events
  show, the envelopes, the bus tracks), kept as they change — even
  stopped without closing, it opens as it was a second before.
- **Video Scopes like VEGAS 23's** — the toolbar's eight views
  (Vectorscope, Waveform, Histogram, RGB Parade, and three, four or two of
  them side by side), Video Scope Settings (skin tone line and its angle,
  7.5 IRE Setup, Studio RGB; the HDR scope greyed out) and Update Scopes
  While Playing; each scope under a bar with its name and its mode
  (Waveform's Luminance or Composite; Histogram's Luminance, Red, Green,
  Blue, Alpha, Luminance/R/G/B or Lightness/a/b with the mean and the
  standard deviation; RGB Parade Separate or Combined with its R, G and B
  buttons), remembered between sessions. Their scales show with or without
  video, measured on VEGAS's screenshots: the vectorscope's rings, I and Q
  axes, skin tone line, outer ring of hues and the 75 % bars' targets
  (±10°, ±20 %), the waveform's IRE lines from −20 to 120 and the bars'
  levels, the 0–255 lines of the histogram and the parade; the pointer
  reads chroma, luma or a channel's value. The traces are VEGAS's, matched
  pixel for pixel on its screenshots of the calibration card: the rows and
  columns each scope reads, the waveform's points brightening with the
  pixels there, the parade's channel colors, the vectorscope's crosses in
  their chroma's color, the histogram's logarithmic bars with the mean
  and standard deviation of every pixel, Studio RGB on.
- **Interface** — VEGAS-like dock layout and saved layouts, all seven top
  menus mirroring VEGAS, Pan/Crop and FX windows, plug-in chooser, presets,
  OS drag-and-drop import with drop preview, two needles during playback,
  Customize Keyboard (all nine VEGAS tabs with VEGAS's command ids and
  default keys, two-key shortcuts such as Alt+D, A, saved maps), Mixing Console view commands (channel width, channels
  and faders shown, hide channel), Trimmer keyboard commands (mark in/out,
  add to the timeline from or up to the cursor, video/audio only), track selection and keyboard focus between the track list and
  the timeline, inline track rename (F2 / double-click), tabbed Preferences.
- **Toolbars** — Options › Customize Toolbar (main toolbar, 58 buttons) and
  Customize Timeline Toolbar (54 buttons) like VEGAS: available/current
  lists, add, remove, move, reset, saved per user; BananaV icons; Shift
  overrides snapping. Long names in lists end in "…" instead of overflowing.
- **Windows** — dialogs and tool windows are OS windows owned by the main
  one: they can be dragged out of it, even to another monitor.
- **Project Properties, Video tab** (File › Properties) — templates (built-in,
  saved, deleted, matched to a media file), frame size and editable frame
  rate, output rotation (preview, snapshots and renders turned), pixel format
  and compositing gamma (8-bit and 2.222 composite in video gamma, 32-bit
  can composite in linear light; new projects are 8-bit (full range), as
  VEGAS's, so effects match VEGAS's from the start), full-resolution rendering quality (nearest,
  bilinear, bilinear with averaging, bicubic), prerendered files folder with
  free space, "Start all new projects with these settings".
- **Color management (ACES 1.3, OpenColorIO)** — File › Properties ›
  Video's ACES settings, as VEGAS's, with 32-bit floating point (full
  range): the ACES version, the color space video composites in
  (ACES2065-1 or ACEScg), the view transform frames are shown and rendered
  through (VEGAS's lists; drawn with 1.2: sRGB, Rec.709, Rec.2020, P3D65;
  with 1.3: sRGB, Display P3, Rec.1886 Rec.709 and Rec.2020's SDR Video,
  P3-D65's SDR Cinema, and the un-tone-mapped and raw views) and the Look (with 1.3 the
  Reference Gamut Compression, with 1.2 the Blue Light Artifact Fix,
  in linear light); compositing in linear light, or with the 2.222 gamma
  on the pictures' values taken as ACES's light, as VEGAS does; each
  media's color space (Project Media's Properties, VEGAS's Media tab:
  ACES's and the cameras' spaces, logs included) and a render template's
  (Customize Template's Project tab, with its own rendering quality). `bv-ocio` is BananaV's own
  OpenColorIO: it reads configurations (versions 1 and 2) and works out
  their transforms (matrices, curves, logs and camera logs, CDLs, ranges,
  LUT files, the builtins: ACES 1.3's encodings, Looks and SDR outputs —
  ported from the Academy's CTL — cameras' logs and gamuts, displays);
  BananaV's ACES 1.3 configuration names the spaces as OpenColorIO's ACES
  studio configuration does (its names and ACES 1.x's as aliases). Media
  and picked colors enter the working space, taken as ACES2065-1 as VEGAS's
  default (checked on the calibration card through VEGAS's sRGB view); the finished frame leaves
  through the view, worked out exactly on the processor and, kernel by
  kernel, on the graphics card (Preferences › General: "Use GPU processing
  for ACES color management"; off, the processor draws those frames).
- **Project Properties, Audio tab** — master bus mode (stereo, 5.1
  surround), editable sample rate (8–192 kHz listed, 2–384 kHz typed; MP4
  renders mix at 48 kHz when AAC cannot take the project rate), bit depth
  (8/16/24: the default of rendered WAV files), resample and stretch quality
  (linear, cubic, windowed sinc), LFE low-pass filter of 5.1 projects (cutoff
  and 12/24/48 dB per octave), recorded files folder with free space.
- **Project Properties, Ruler and Summary tabs** — the ruler format now
  belongs to the project (shared with Options › Ruler Format) and includes
  Measures & Beats (measures.beats.ticks, 64 ticks per beat, ruler marks on
  beats and measures); ruler start time; tempo (beats per minute, beats per
  measure, note that gets one beat). Summary (title, artist, engineer,
  copyright, comments) is saved with the project and written into rendered
  WAV files (INFO chunk).
- **Track headers like VEGAS's** — colored strip with the More (⋯) menu,
  number and kind; scribble-strip name; M and S (solo on video tracks too);
  level slider (video), volume fader, pan and a live meter (audio); sliders
  reset on double-click and step finely with Ctrl. More menus in VEGAS's
  order: Track FX, Minimize, Maximize, Color Grading, Invert Track Phase.
  Scrub control under the track list: shuttle both ways from the cursor,
  normal-rate triangle, double-click to type the rate.
- **Cursor and event level like VEGAS's** — the edit cursor snaps to event
  edges, markers, regions and the project's end when placed or dragged
  (ruler and timeline; Shift overrides); an event's level line has the blue
  handle in its middle while hovered and shows "Opacity is 52 %" / "Gain is
  −4.1 dB" while hovered or dragged (Ctrl: fine steps).
- **Track list like VEGAS's** — drag a header to move the track anywhere
  (a line shows where it lands); drag a header's bottom edge to change the
  track's height.
- **AAC decoding like ffmpeg's** — sample times rounded to 100 ns no longer
  add or drop a sample at each AAC frame (heard as clicks, seen as holes in
  the waveform); MP4/MOV priming samples are skipped as the file's edit
  list says (audio was 21 ms late). Checked sample by sample against ffmpeg,
  in order and with jumps.
- **Timeline thumbnails and waveforms** — thumbnails keep their decoders
  open, come in fixed heights and stand in for each other while loading (no
  blanks when zooming); zoomed in past 10 ms per pixel, waveforms are drawn
  from the samples themselves, down to single samples.
- **Event Loop and Hold Last Frame** (Edit › Switches) — events stretch
  past their media like VEGAS's: with Loop (on for new events) the media
  repeats, video and audio, with notches where it starts over; without it
  the last frame holds or nothing shows. Slip and trims wrap around; a
  dragged edge snaps to the media's start, end and each lap.
- **Several audio streams per file** — every audio stream of a video is
  found (in the file's order, also for MP4/MOV, whose tracks Media
  Foundation lists backwards) and goes to its own audio track on import,
  grouped with the video. Header values (level, volume, pan) can be typed
  with a double-click.
- **Project Properties, Audio CD and Advanced tabs** — all six VEGAS tabs are
  in place. Audio CD keeps the disc's catalog number (13 digits) and first
  track number in the project for when BananaV burns CDs.
- **Languages** — English as the main language, Brazilian Portuguese
  translation, Language tab in Preferences (BananaV's own: VEGAS has none).
  Ready for more: each language is one `Locale` (separators, CLDR plural
  rule, catalogs) and one line; texts that count use plural forms (`trn!`);
  `cargo xtask i18n` and a test list the texts a language lacks.
- **Render window like VEGAS 23's** (File › Render…) — tree with search,
  favorites, use cases (YouTube, social media, web, VFX & master, image
  compositing, DVD & Blu-ray, audio master, podcast), video, image sequence
  and audio formats; templates in columns or a details table, sorted by
  format or name; filters matching the project (resolution, frame rate,
  ratio, color depth, sample rate, channels, bit depth) and "Show all
  formats"; details with the settings that differ from the project in red;
  folder (recent folders, project location), name, free space and estimated
  size; options (letterbox or stretch, output rotation, markers in the file,
  loudness log); Customize… to save templates of one's own.
- **Render progress like VEGAS's** — "Writing MP4 File: 3%" window with the
  share of the estimated size, time left and elapsed, Estimated Properties
  (free disk space, rendering duration, time of completion), Performance
  (average and current speed, CPU, memory and disk use), Options (show the
  frames in the Preview window, open the folder or close the window when it
  ends, shut the computer down afterwards), Open Folder / Open / Cancel;
  Cancel asks first ("Cancel Warning") and a canceled render leaves no file.
  The window is modal, as VEGAS's: until it closes the rest of BananaV
  takes no input (the main window and the other windows refuse the mouse,
  the keyboard's shortcuts, the joystick and the control surfaces wait).
  The progress also shows in the main window's title, the status bar
  (Cancel, bar, file) and the taskbar button; renders over a minute end with
  a sound.
- **Folders that refuse files** — Render checks the folder first; when
  Windows Security's Controlled folder access blocks BananaV (Documents,
  Videos, Pictures, Music, Desktop) it says so, how to allow BananaV, and
  opens Windows Security's ransomware protection page. Failed renders and
  saves explain it the same way.
- **Importing media like VEGAS** (File › Import › Media, File › Open,
  Explorer, drag and drop) — the Import dialog's filters (All Media Files,
  Video, Audio, Image Files); File › Open takes projects and media alike;
  File › Import › Image Sequence takes numbered pictures as one video event
  (first and last picture, frame rate; missing pictures show black). A file
  is recognized by its content, not only its name, and one that cannot be
  read says why (the codec's name, e.g. "DNxHD", or "not a media file").
  - Video: MP4, MOV, M4V, 3GP (H.264, HEVC 8/10-bit, MPEG-4, H.263, Motion
    JPEG, Apple ProRes — every profile, 4444 with alpha, interlaced — through
    BananaV's own decoder), MKV and WebM (H.264, VP8, VP9, AV1), AVI (H.264,
    Xvid/DivX, Motion JPEG, DV, uncompressed), WMV/ASF, MPEG-1/2 program
    streams and DVD VOB, MPEG transport streams (TS, MTS, M2TS — AVCHD),
    animated GIF (each frame's own duration, disposal).
  - Audio: WAV (PCM, float, µ-law, A-law, IMA and Microsoft ADPCM), BWF,
    RF64, Wave64, AIFF/AIFC, AU, CAF, MP3 (encoder delay skipped), MP2,
    AAC/M4A, ALAC, FLAC, Ogg Vorbis and Opus, WMA, Dolby Digital AC-3
    (BananaV's own decoder, from ATSC A/52, also inside videos), AMR.
  - Images: PNG, JPEG (baseline and progressive, CMYK, EXIF orientation),
    BMP, GIF, TIFF, TGA, DPX, OpenEXR, Photoshop PSD/PSB (merged image) with
    BananaV's own readers; WebP, HEIC/HEIF, AVIF, JPEG XR, ICO and camera RAW
    (DNG, CR2, NEF, ARW…) through Windows' codecs.
  Checked against ffmpeg (JPEG against libjpeg): pictures and frames match,
  and audio starts on the right sample (a chirp finds any delay).
- **Smooth preview and the graphics card** — each event has a decoder of its
  own (a crossfade between two parts of one recording made a single
  decoder seek twice a frame: 1.5 fps, now 60+), and so has each audio event
  (the same crossfade's sound mixed at 3× real time, now 68×); decoders run
  on threads of their own and decode the next frames ahead, the next frame
  is drawn while one shows, and events about to start get their decoders
  2.5 s early (no stall where a transition starts).
  Video is composited on the graphics card (wgpu: Direct3D 12, Vulkan,
  Metal or OpenGL — NVIDIA, AMD and Intel alike), the same frames as the
  processor's compositor (checked kernel by kernel, every card of the
  computer), falling back to it when there is no card or it fails;
  Options › Preferences › Video picks the card ("GPU acceleration of video
  processing"). Video is decoded on the graphics card (Windows' decoders
  driving its video engine: H.264, HEVC, VP9, AV1; MPEG-2 stays on the
  processor, whose decoder loses a file's last frames on the card after a
  seek), the same pictures as on the processor, with a fraction of the
  processor's work;
  Options › Preferences › File I/O ("Enable Hardware Decoding", the card to
  use). HEVC Main10 stays 10-bit. The preview shows the frame rate it keeps
  up with while playing.
- **Scrubbing and the two needles** — dragging the cursor shows frames all
  the way: a seek that has not reached its frame within 25 ms shows the
  latest frame it decoded on the way, a newer position cancels an older
  seek, seeks decode about twice as fast (frames far from the target are
  not copied out) and the latest frames shown are kept (160 MB). While
  playing, the playback needle can be dragged by its head on the ruler or
  its line in the timeline: playback waits and goes on from where it is let
  go, like VEGAS's; the edit cursor can be dragged meanwhile. Options ›
  Preferences › General: "Allow edit cursor to be dragged" (Shift+drag
  selects time instead), "Allow Ctrl+drag cursor style scrub over events",
  and Space and F12 as Play/Pause (VEGAS's default) or Play/Stop.
- **Options › Preferences, General and Video tabs** — VEGAS's tabs in its
  three rows (the open tab's row next to the page). General, working: open
  the last project at startup, confirm removing media in use, close media
  and the audio output when BananaV is not the active program, match the
  project to the first media, check the project file association, stereo
  files as dual mono, undos for FX parameter changes, keep bypassed FX
  running, name markers and regions as they are added, linear scrub,
  Ctrl+drag scrub over events, Space/F12 as Play/Stop or Play/Pause, marker
  lines, edit cursor dragging, double-click to the Trimmer, the Trimmer's
  history names, warning before deleting tracks with events, autosave and
  recovery, FX windows resizing to their parameters, peak files (none, or
  hidden), saving active prerenders on close, the joystick (color wheels,
  track sliders, pan), Trimmer markers saved with the media file,
  prompting for recorded files, double-clicking an event edge for Expanded
  Edit Mode; the recently used project list, the temporary files folder.
  Video: rendering threads, the graphics card, the Dynamic RAM Preview's
  memory (Tools › Build/Clear Dynamic RAM Preview), proxies (made at import
  for media over 4K that cannot play in real time, or always), simulated
  device aspect ratio, source frame numbers and which thumbnails events
  show, the Video Preview's safe areas, grid, display at project size and
  background, and the colors (fade to color, take indicator). The color
  swatches open VEGAS's color picker (alpha, RGB or HSL bars, spectrum,
  values, eyedropper anywhere on the screen).
- **Options › Preferences › Display** (every tab is done) — the track
  colors Color A–H (headers' strips; events at half the color, waveforms
  in it; a track header's Track Color menu picks one: video tracks start
  with A, audio ones with B), the envelopes' colors, the snap line's color
  by what an edit snapped to (cursor, envelope point, loop, drag origin,
  marker, region, event, event fade, grid: events now also snap to their
  fades' ends, the shown envelopes' points and where a dragged event or
  edge started), the icons' color strength, the active toolbar buttons'
  color, the interface type (Dark, Medium, Light, White, at once), the
  docking area hidden until wanted (its strip: double-click, or point at
  it when it is at the bottom; the toolbar's Auto Hide Docking Area), the
  docked windows' tabs at the bottom or the top, floating windows docking
  only as the setting says (Ctrl turns it around; dragged by their tab
  they follow the pointer from the first move, as VEGAS's), the scale
  (High DPI off: 100 %) and the processor, graphics card and memory
  BananaV uses in the status bar. Deprecated Features lists nothing
  (BananaV has none):
  File › Import › DVD Camcorder Disc and View › Window › Device Explorer
  are regular commands.
- **The main window like VEGAS 23's** — new projects start without tracks;
  the docking area in VEGAS's proportions (Explorer, Project Media, Media
  Generator, Transitions, Video FX and Color Grading; Video Preview,
  Trimmer and Properties; Master Bus), neutral greys, flat tabs below the
  windows (the open one bold and underlined, with its close button; the
  longest titles shortened with "…" when the tabs do not fit). Media
  Generator, Transitions and Video FX: categories (All Plug-ins with
  search, Favorites, the groups; "»" for those that do not fit), the
  plug-ins with favorite stars, and the presets as pictures where only the
  one under the pointer plays its example, with its progress bar (effects
  on BananaV's own sample picture); presets dragged onto events or tracks,
  double-clicked, or Apply to Selected Events / Apply to Track; Solid
  Color's colors and Custom Color…. Explorer: back, forward, up, the path
  and places; the folder tree (favorites, the user's folders, drives) and
  a list or details (name, size, type, date modified; sorted by any);
  Add to Favorites, Add to Project Media, Add to Timeline, Trim. Project
  Media: the import menu, Remove All Unused Media, Remove Selected, Media
  FX, views, search, hover scrub; bins (All Media, By Type, bins of one's
  own: created, renamed, deleted, media dropped on them; saved with the
  project); thumbnails, list or details. Video Preview: Project
  Properties, external monitor, Video Output FX, quality, zoom, overlays
  (grid, safe areas), copy and save snapshot; the transport and its More
  menu; the project, preview, frame and display sizes. Master Bus: mute,
  fader, meters with clip lights and peak holds. The status bar's Record
  Time (the recording folder's free space). The panes inside the windows
  are resized by dragging the splitters between them.
- **Video Preview, Media Generator and events, further** — Preview
  Quality as VEGAS's: Draft, Preview, Good and Best (how pictures are
  scaled), each at Auto, Full, Half or Quarter size, kept between
  sessions; the zoom typed or from its menu (+, −, Original resolution,
  50–800 %, Bypass Zoom); the overlays' grid, dashed safe areas and one
  color channel (red, green, blue, each as grey, alpha as grey);
  Split-Screen View with every video effect bypassed, or the last
  snapshot copied, in a part of the frame dragged out (halves, all; a
  double-click shows it all), retained between sessions when asked.
  Media Generator: a preset's + adds it at the cursor and opens the
  Video Media Generators window (frame size, length, the preset saved
  or deleted, the plug-in's settings), which the Generated Media button
  of generated events opens too. The events' buttons sit in dark
  squares. The Master Bus as VEGAS's: a narrow strip at the right that
  keeps its width when the main window changes size (dragging its edge
  sets another), the white two-square fader knob, meters dim red, amber
  and green down their height and lit up to the level, a scale every
  3 dB with its dots, each channel's peak (red once it clipped).
- **Faster renders to MP4** — the graphics card converts each frame to
  the encoder's NV12 or P010 (the same values as the processor: a tenth
  of the bytes come back), and a thread of its own writes the file while
  the next frames are drawn: 1080p60 to H.264 went from 15 frames a
  second to about 250 with NVENC (four times real time); the processor
  converts on all its threads when there is no card. `bv-engine --test
  render_desempenho` measures it.
- **Options › Preferences, Sync, External Control & Automation and
  Deprecated Features tabs** — Sync: MIDI timecode (24, 25, 29.97 drop and
  non-drop, 30 fps; quarter frames and full-frame messages) and MIDI clock
  (song position, Start or Continue) sent while playing; playback cued,
  started, kept in step, free-wheeling and stopped by MIDI timecode coming
  in (Options › Timecode, F7, Shift+F7, Ctrl+F7; Advanced… for the
  free-wheel, delay and offset); the time display shows the timecode in and
  out (its right-click menu). External Control & Automation: track
  automation (a header's Automation Settings Off, Read, Write (Touch), Write
  (Latch), and Activate Automation Controls: its sliders record envelopes
  while playing, thinned afterwards; with automation off the envelopes do
  not play and the controls hold values, their defaults or the last
  ones); the Mackie Control protocol (Options › External Control: eight
  strips banked over the tracks, faders, V-Pots, REC/SOLO/MUTE/SELECT,
  transport, markers, loop, metronome, cursor keys, zoom and jog, the
  automation buttons; motorized faders, lights, the display and the time
  sent back); the fade types of new envelopes, events' fades and Pan/Crop
  keyframes (six types: Linear, Fast, Slow, Smooth, Sharp, Hold) and Pan/Crop
  keyframes' smoothness (a curved path). File › Import › DVD Camcorder
  Disc (a disc's titles copied and added to Project Media) and View ›
  Window › Device Explorer (cameras' and cards' media).
- **Options › Preferences, MIDI, VST Effects, Editing and CD Settings
  tabs** — MIDI: the system's MIDI ports made available, and MIDI Thru (an
  input played out on an output while BananaV is active). VST Effects: the
  search folders (Add/Edit/Remove path, Refresh) and the VST 2 and VST 3
  effects found in them, read without running them (exports, VST 3 module
  information, version resource). Editing: new events' Loop switch and
  pitch, cut/copy/delete with or without the group, audio-only edits off
  the frame grid, the loop region collapsed by a click, JKL speeds, the
  quick fade length, still images' length, Transport › Cursor Preview,
  several files dropped overlapping, a transition on a cut turning it into
  an overlap (before, around or after the cut), an envelope dragged within
  the time selection with fades (outside, centered or inside), media cropped
  to fill the frame, Smart Split's overlap length (and its group), the
  cursor and selected events in Auto Ripple's color, a right-drag selecting
  like the Selection Edit Tool, clicks on events that leave the cursor.
  Audio events stretched (Ctrl+drag of an edge, Playback Rate) keep their
  pitch (WSOLA in the mixer) unless their Properties say otherwise.
- **Options › Preferences, Preview Device, Audio and Audio Device tabs** —
  Preview Device: Video Preview on External Monitor (Alt+Shift+4) full
  screen on the chosen display (Identify Displays shows each one's number),
  scaled to fit or at its own size, deinterlaced, studio levels expanded,
  the output rotation taken back, a sync offset, edited frames held back,
  the Video Preview window left still while playing; the external hardware
  devices are to come. Audio: the waveforms drawn while recording, the
  Normalize level, Broadcast Wave recordings (their place on the timeline),
  what Record does with no track armed, the preferred audio editor, the
  metronome (Options › Metronome: BananaV's clicks or sound files, accent
  on each measure), the loudness log opened in the text editor. Audio
  Device: the system's default device or a chosen one, playback buffering,
  track buffering (each track mixed on a thread of its own), the default
  recording input (stereo, left or right), the recording latency measured
  from the devices' clocks or set by hand.
- **Tools › Selectively Prerender Video** — the parts of the time selection
  (or of the project) that do not play in real time drawn into H.264 or
  ProRes files of at most 300 frames; the preview plays them, a blue bar
  above the timeline marks them, an edit there sets them aside (a
  fingerprint of what each part shows); Clean Up Prerendered Video.
- **Trimmer markers and regions** — M, R, S (Save Markers/Regions), Shift+S
  (save automatically), Ctrl+Left/Right and 0–9; dragged, named, deleted;
  kept in the project's media, saved into WAV files (`cue `/`ltxt`, the
  audio left where it is) or beside other files (`.bvmarkers`), read back
  at import; shown in the events (View › Event Media Markers, with or
  without labels) and snapped to.
- **Edge trimming and Expanded Edit Mode** — `[`/`]` put a red bracket on
  an event edge and move along the track's edges; the nudge keys and F3/F9
  trim it; Expanded Edit Mode (Edit › Select, Num 5, or a double-click on
  an edge) shows the track as A and B rolls with the media there is beyond
  the edit point, and the Video Preview splits into the frames on either
  side.
- **Recording** — Arm for Record (an audio track's More menu or
  Ctrl+Alt+R; the header shows its button while armed, as VEGAS's has none
  by default), Record Input
  (each input device, stereo pairs and single channels), Record (Ctrl+R) and
  Set Recording Path and Record: each armed track's input written to its
  own WAV file in the recorded files folder while the project plays, its
  meter showing the input, then the Recorded Files window (Rename, Delete,
  Delete All, Done) and the takes placed where recording started.
- **Formats and encoders** — H.264, HEVC (8/10-bit) and AV1 in MP4 with AAC,
  encoded by the processor or by the graphics card (NVIDIA NVENC, AMD
  VCE/AMF, Intel Quick Sync), found on the computer when BananaV runs;
  BananaV's own MP4/MOV writer (movie header first for progressive download,
  metadata, chapters from markers); Apple ProRes 422 Proxy/LT/HQ and
  4444/XQ with alpha (BananaV's own encoder); Windows Media Video/Audio;
  image sequences in BMP, DDS, DPX, EXR, HEIC, JPEG, PNG, TIFF and JPEG XR;
  audio in Wave (cue markers), Wave64, AIFF, FLAC (BananaV's own encoder),
  MP3, AAC (.m4a) and WMA. Every one checked against ffmpeg.
- **Timeline mouse and keys like VEGAS's** — the shortcut menus of a
  track's empty part and of an event in VEGAS's order (Duplicate Track,
  Insert/Remove Envelope, Fade Colors, Select Events to End, Select in
  Project Media list, Switches, Take, Group, Stream…); Shift+click selects
  the events from the last one clicked, and elsewhere makes a time
  selection from the cursor; dragging one of several selected events moves
  them all; Ctrl+Shift+drag over an event makes a time selection; Alt+drag
  slips (on an edge: slip trim; Alt+Shift: the end); Ctrl+Alt+drag slides
  (on an edge: trims the adjacent event too; over a crossfade: slides it);
  Shift+drag of an edge trims the event alone (L and J cuts) and turns
  snapping off, or on when it is off; Ctrl+Alt+Shift+click splits (a drag
  splits and trims); a double-click on an event selects its time; a drag
  with the right button asks Move Here, Copy Here or Cancel; the edit
  cursor can be dragged by its line over events too. The mouse wheel
  zooms toward the needle wherever the pointer is, as VEGAS's (in view it
  keeps its place, out of view it comes to the middle; ↑/↓ and the zoom
  buttons too); with Ctrl it scrolls up and down, with Shift across; Ctrl+Shift
  (+Alt) moves the cursor a pixel (a frame), the trimmed edge in
  edge-trimming mode, or the scrub rate while playing; a click of the wheel
  auto-scrolls. Keys: the numeric keypad works (Num 0–9, +, −, *, /),
  Shift with a key that moves the cursor extends the time selection, J
  shuttles backward, and VEGAS's remaining navigation commands work (Tab
  between track list and timeline, Num 2/8 move events between tracks,
  track heights, Swap Cursor, cut/crossfade toggles, typing the selection's
  end or length).
- **Video Preview navigation like VEGAS's** — the mouse wheel zooms toward
  the pointer (10 % a notch, 1 % with Ctrl), dragging with the wheel held
  pans, a double-click (with the wheel, or the left button) shows the whole
  frame at 100 % again; Ctrl+Num +/− (5 %), Ctrl+Num 1 and Bypass Zoom. Its
  shortcut menu: background (default, black, white), Simulate Device
  Aspect Ratio, Enable Preview Scaling, Adjust Size and Quality for Optimal
  Playback (smaller frames at Draft while playback falls behind), Show
  Toolbar, Status Bar and Transport Bar, the Video and Preview Device
  preferences and Preview Quality. Made narrow it stays whole, as VEGAS's:
  the toolbar cut off at its edge, the frame fitting the width, the
  transport's other buttons in More, the status line's items that fit.
- **Pan/Crop like VEGAS 23's** (from the user's prints) — Show Properties
  off at first (the gear shows them beside the workspace), VEGAS's tools
  in its order (Normal and Zoom Edit Tool, Enable Snapping, Lock Aspect
  Ratio, Size About Center, the movement's mode in one button), VEGAS's
  grid (the media's frame repeated, a dot at each sixteenth, which
  snapping follows), the box, its F and the rotation circle round it in
  dark and light dashes that show over any picture, the media small enough
  for the circle at first; a drag round the box turns it, the pointer a
  turning arrow there (drawn by BananaV: the system has none; Track
  Motion's and the masks' too); the shortcut
  menu VEGAS's six (Restore, Center, Flip Horizontal, Flip Vertical, Match
  Output Aspect, Match Source Aspect); the keyframe bar as the OFX
  controller's; Ctrl+C, Ctrl+X, Ctrl+V and Delete on the picked row's
  keyframe, Ctrl+arrows between keyframes, the program's other keys (Space,
  Ctrl+Z…) while the window has them. Masks are worked out on every
  processor thread and kept while they do not change (2.8 times faster at
  1080p, nothing again while playing).
- **Crossfades and the track list like VEGAS 23's** (from the user's
  prints) — an audio crossfade's overlap drawn darker; a video crossfade
  with a transition shows its name in a blue band instead of the X, a
  plug-in's Transition Properties icon at its right (or a double-click)
  opening the transition in the FX window as VEGAS's Video Event FX
  ("Transition: name" over its chain's button, its checkbox switching the
  transition off — a plain crossfade —, Remove taking it out, its
  presets, settings and columns); Event Fade Lengths off at first. The track headers give way as VEGAS's do as
  the list narrows (the strips alone, then Mute and Solo over the sliders
  alone, the values from 200 pixels, the names Vol: and Pan: sharing the
  row from 400), the time display growing smaller, then gone, the Rate's
  slider and the Rate too; the list widens to nearly the whole timeline.
- **Sapphire OFX in BananaV** (the user's request, with the user's own
  Sapphire 2026.5 OFX) — its 291 effects, transitions and generators
  describe themselves and render, all 291 in every step (each tried in a
  process of its own: `vegas_poc` with `BV_OFX_POC_DIR`; the 67
  transitions as transitions too, the 43 generators as generators), and
  its settings show in the FX window. One start in eight crashed in the
  C runtime's `_spawnle` as Sapphire launched its preset browser: the
  runtime reads past the environment's end when it holds no per-drive
  current folder, which BananaV now puts there before plug-ins run. A
  plug-in loads alone when first used instead of its whole binary
  (seconds less the first time a Sapphire effect is picked); every
  plug-in hears OpenFX's unload action when BananaV closes, so Sapphire
  closes the preset browser it keeps in the background (the scans'
  children also end what plug-ins start); a describe that crashed is
  tried again and never cached as "no plug-ins"; the guard's report says
  where a crash was (module and offset); the plug-ins' cache is written
  only when it changed (Sapphire's descriptions are tens of megabytes).
  Trials also draw a plug-in's overlay. Its on-screen controls show:
  overlays of OpenFX's first kind, which draw with OpenGL themselves (as
  Sapphire's do for hosts it does not know), are drawn in an offscreen
  OpenGL context of the graphics card's and laid over the preview. The
  registry no longer copies every plug-in's description at each lookup
  (23 ms each with Sapphire's, several a frame: BananaV crawled), and
  plug-ins get the main window as the parent of their own dialogs.
  Plug-ins that render with CUDA (OpenFX 1.5's CUDA render, as Sapphire's
  do) draw on the graphics card's pictures where they are when the card
  is NVIDIA's: the compositor's Direct3D 12 buffers are shared with CUDA
  and handed to the plug-in, where the pictures used to come out of the
  card, go to the plug-in, which sent them to the card and back itself,
  and go back up — at 4K most of a frame's time (on the user's RTX 4070,
  a 4K frame through S_Blur 180 ms before, 80 ms now; S_Glow 188 → 90,
  S_LensFlare 228 → 121; at 1080p S_Blur 30 → 14.5 ms). Plug-ins that
  render only with CUDA get the processor's pictures through the card;
  elsewhere (other cards, Linux) the pictures still go through the
  processor, its buffers for them now kept from frame to frame.
- **Playback without stalls at transitions, two frames at once** (the
  user's request) — the preview draws each frame for the moment it will
  show (by how long frames take lately): a preview slower than the
  project's rate drops frames evenly, where it used to draw the next
  frame, which came after its time while the one due waited (half its
  work thrown away); frames that keep up are still drawn one after
  another. Two threads draw: the second takes alternate frames when one
  cannot keep up on the graphics card and effects (not decoding) do the
  work, tried and kept only while that shows more frames (Sapphire at
  4K: 1.3–1.4 times the frames); the frames asked for (scrubbing,
  playback's first) stay the first thread's, its readers being where
  they go on from. Before an event or a transition starts, its readers
  are positioned — those left further on by an earlier playback too,
  which made a transition's first frame seek back (about 230 ms) — and
  its plug-ins readied in the background at the preview's size (Sapphire
  takes a second to make an instance and more for a first frame: its
  transitions froze playback for 1.5 s); plug-ins load, and instances are
  made, outside the registry's and the instances' locks. Playing through
  a one-second transition of a 1080p60 recording at 1280 × 720 and
  1920 × 1080: BananaV's Linear Wipe went from a 250 ms stall at its
  start, GL Transition from 300 ms (and 26 frames a second in its middle
  at 1080p) and Sapphire's S_DissolveBlur from a 1.6 s freeze to 60 frames
  a second throughout.
- **Final Cut Pro 7 XML both ways, VEGAS's files too** (the user's
  request, with a project of theirs exported by VEGAS 23) — VEGAS's file
  did not open: VEGAS describes its files in its bins' master clips,
  where BananaV did not look, and writes a recording's two sound streams
  as two stereo pairs both naming the first stream's channels.
  `bv_timeline::xmeml` reads the format as its writers keep it (Apple's
  description, OpenTimelineIO's reading, Premiere's, Resolve's and
  VEGAS's files): files described wherever they first appear; clip items
  next to a transition reaching over it, crossfades as overlaps and
  transitions from or to black as fades (clips written at the cut reach
  into their handles); stereo pairs of mono clip items on two tracks one
  event each, their channels choosing the stream and the channel layout,
  tracks playing the same channels of a file at once its next streams;
  links as groups; opacity and gain (keyframed ramps at the ends as
  fades); constant speeds; titles and colors; nested sequences in their
  place; markers and regions; the sequence's frame size and sample rate;
  VEGAS's audio tracks, listed from the bottom up, back in its order.
  File › Open opens `.xml` and `.fcpxml` files too. The export writes
  what those programs write: one sequence, each file described at its
  first clip item, crossfades and fades as transition items, sound as
  stereo pairs (each stream's channels; Premiere's markings of a pair),
  links, opacity, gain and pan, Time Remap, titles and colors, markers;
  it counts the events it says only in part (effects, Pan/Crop, frozen
  or reversed frames, other generators). The user's project opens as it
  was in VEGAS: its two videos crossfading, its two sound tracks each
  playing its stream of both recordings, their crossfades where they
  were. Still to come: Pan/Crop as Basic Motion (a VEGAS export with one
  to measure it on), the user's check of VEGAS opening BananaV's files
  (VEGAS writes its audio tracks from the bottom up, and may read them
  so).
- **Other makers' plug-ins kept from taking BananaV down** (the user's
  request) — on Windows a plug-in that crashes (an access violation, a
  C++ exception it lets out, a stack overflow), in any of its calls or on
  the threads it asks BananaV for, is caught: what it was doing fails, its
  plug-ins are switched off until BananaV restarts and the status bar
  says so (Plugins.log too). A test plug-in crashes on purpose to prove
  it.
- **The timeline runs on at its edges** (the user's request) — as VEGAS's:
  dragging an event, its edges or fades, a selection, an envelope point,
  a marker, the loop region or the ruler's scrub near or past the tracks'
  left or right edge scrolls the timeline that way, faster the further
  past, what is dragged following at the edge; events, the selection
  rectangle and tracks reordered scroll up and down too, and media
  dragged in near the sides.
- **Color Grading laid out as VEGAS 23's** (from the user's prints) — the
  header (the FX level's menu, what it grades; Bypass, Reset All, More);
  the navigation folded at first, VEGAS's rows (the picked one blue, a
  changed item marked, each wheel's view and the curves' pins at their
  right); a group's items side by side in columns, an item alone in half
  the width: the wheels as VEGAS draws them (a shaded ball in a ring of
  hues, red at the top, its crosshair, the ball's lines through the
  point; R, G and B beside it over the Y slider), the RL wheels after the
  Threshold column, the LUTs' boxes (the table's picture and name, the
  arrows to the one before or after) with Strength, the Utilities in two
  columns (Levels' double sliders, Tone, White Balance's and HSL's colored
  bands), the Color and HSL Curves side by side filling the panel (Select
  Curves on the title row, the zoom in the graph's corner). The colors'
  calibration against VEGAS comes later, as the user asked.
- **Lighter temporary effects** — the effects and transitions made from
  VEGAS's Help, until their final versions: pictures handed to plug-ins
  and back on every processor thread, float rows copied whole, long blurs
  worked out smaller and enlarged again, box blurs without turning the
  picture, Radial Blur by halvings, Defocus's iris and Light Rays smaller,
  Denoise's weights from a table, Median's values kept per thread, the
  card's frames copied out on several threads. At 1080p on the card:
  Defocus 326 → 56 ms, Radial Blur 176 → 68, Denoise 186 → 90, Light Rays
  144 → 51, Starburst 118 → 91, Soft Contrast 112 → 60, GL transitions
  about 50 → 38 (the plain frame 23); `bv-engine`'s `own_effects_speed`
  measures them.
- **Windows: the interface through DirectX 12** — with the monitors on two
  graphics cards, Windows copies the window between them itself, so it
  moves smoothly from one monitor to the other (awaiting the user's test).
- **Welcome screen like VEGAS 23's** (File › Welcome Screen; shown on
  the first start) — New Project with BananaV's own banner, the five
  shapes (Widescreen 16:9, Fullscreen 4:3, Portrait 9:16, Square 1:1,
  Scope 2.4:1) with their HD/FullHD/UHD sizes, the frame rates (24p to
  120p, Full Frame or Drop Frame 1/1.001), Create Project and Advanced
  Settings… (Project Properties); Open Project (Browse…, the last opened
  and the other recent projects); Getting Started (BananaV's videos, help,
  community and social media, to come); Language (BananaV's own: the
  interface's languages by their own names); Show Welcome Screen on
  Startup.
- **Events like VEGAS's, more** — out-of-sync events show by how much in
  red ("+27:02" / "−27:02": a video and an audio event of the same file,
  overlapping, their media apart — after U and a move, or a split and a
  move); Ctrl+drag of an edge changes the playback rate (25 %–400 %, the
  edge sticking at the normal length), shown as a wavy line along video
  events (tighter the faster) and the rate on audio ones ("154.3%"); the
  edges' tips (Trim Event Start / End). The audio track headers' meters
  in VEGAS's colors (dim red, amber and green; lit to the level; the peak
  above them). A filled cog for Settings.
- **Selecting like VEGAS's** — the events clicked (Ctrl+click: more;
  Shift+click: a range) are outlined in yellow, the members of their
  groups selected along with them in blue; dragging on an empty spot
  draws the selection rectangle (a right-click while dragging: the time
  it spans on every track, the tracks it spans, free again), a right-drag
  there selects time; the loop region is made in its bar (a double-click
  there: the whole project, again: none) or by double-clicking an event;
  a double-click on an empty spot selects the gap between two events, or
  past the last one puts the cursor at the project's end, and a third
  click selects the whole project. Overlapping events keep only their
  crossfade: an event's own fade in or out at an edge another event
  overlaps is left out (no handle there either), back when they part.
  Fades are drawn as VEGAS draws them, from the bottom corner up to the
  event's level line (its opacity, its gain), crossfades to each event's
  own, a dark line across the event where each fade ends and the level
  line only between them, the fades' handles on it; the grid's dotted
  lines (Options › Grid Spacing) on the timeline's background, as
  VEGAS's; a right-click on a crossfade opens VEGAS's 5 × 5 pictures of
  curve pairs, on a fade VEGAS's popup of curves (Fast,
  Linear, Slow, Smooth and Sharp as pictures, a dot by the fade's). Sharp
  is VEGAS's S turned around (fades, keyframes and plug-ins' keys);
  fade-outs are named by how they begin, as VEGAS's (a Fast one drops at
  once). Picking 32-bit floating point (full range) sets the 1.000
  (Linear) compositing gamma, VEGAS's default for it, and ACES's sRGB view
  when none was chosen, as VEGAS's 32-bit projects show; the Video
  Preview's bar names the view ("597x336 sRGB (ACES)").
  The audio tracks' output meters change range with the mouse wheel over
  them: 42 dB by default, 10.5, 78 and 120 dB, remembered.
- **Windows ARM64 and Linux** — BananaV builds and runs on Windows ARM64
  and on Linux x64 and ARM64, each checked on GitHub's machines. On Linux,
  media go through FFmpeg (5.1 or newer) with Media Foundation's rules:
  decoding on the processor or the graphics card (VA-API; NVIDIA's
  decoder), the encoders of the processor (x264, x265, SVT-AV1…) and of the
  cards (NVENC, VA-API), stills (WebP, AVIF, HEIF, icons), DTS and the
  other formats FFmpeg reads; sound and MIDI through ALSA; the disks,
  memory, clock and joystick from the kernel; the mouse, the numeric
  keypad, the eyedropper, the displays and the dialog windows through X11
  (XWayland under Wayland); project files opened by BananaV through the
  freedesktop.org standards; the user's folders where the system keeps
  them. Packages made in the CI by hand: zips for Windows x64 and ARM64,
  AppImages for Linux x64 and ARM64 with an FFmpeg built under the LGPL.
- **OpenTimelineIO** — File › Export › OpenTimelineIO writes `.otio`
  documents and the bundles `.otioz` (a zip) and `.otiod` (a folder)
  with the media the events use; File › Import › OpenTimelineIO, File ›
  Open and a drop on the window (a project too) read them (an `.otioz`
  unpacked beside it) as a new project:
  tracks, cuts, crossfades and fades (as transitions), speeds, frozen
  frames, image sequences, generated media, markers and regions, the
  ruler's start (an hour's timecode), nested clips' first track; other
  programs' video and sound of one file grouped, files not found kept
  offline. BananaV's own projects come back whole (effects, Pan/Crop,
  levels, envelopes, groups, the media's markers) through the metadata
  OpenTimelineIO keeps for each program. Bundles are packed and unpacked
  in the background with Cancel in the status bar.
- **The track headers' buttons like VEGAS 23's** (from the user's prints)
  — Edit Visible Button Set as VEGAS's dialog: "Specify the controls you
  want to appear in all video track headers.", a box for each button —
  Bypass Motion Blur, Track Motion, Track FX, Activate Automation
  Controls, Mute, Solo, Compositing Mode, Make Compositing Parent, Make
  Compositing Child, Min/Max, Color Grading —, only Mute and Solo ticked
  at first, Show All and Hide All beside them, OK and Cancel. With every
  button the video header is VEGAS's: Maximize, Minimize, Bypass Motion
  Blur, Color Grading, Track Motion, Track FX, Automation Settings (its
  arrow the modes' menu), Mute and Solo on the top row, the compositing
  buttons at the end of the level's row; as the list narrows they go from
  the left, as the prints at three widths show. More (⋯) lists the
  commands of the buttons a header does not show, in VEGAS's order, its
  switches checked as VEGAS's, then Edit Visible Button Set. The audio
  tracks follow the same rules (their order awaiting a print).
- **The adjustment track like VEGAS 23's** (from the user's prints) —
  its Composite envelope's line takes a point only with a double-click,
  and dragged it goes up or down (the points around it, their times
  kept); a point's menu is VEGAS's: Set to 100% and 0% Composite
  Strength, Set To… (the value typed in a box at the point), Delete, the
  fades, Flip All Points, Thin All Points and Reset All, as for every
  track envelope's points. Video effects dragged onto the adjustment
  track, or onto any video track's header, become the track's effects.
- **Fades and crossfades drawn like VEGAS 23's** (from the user's prints)
  — an audio event's waveform thins along its fades as they are dragged,
  and a crossfade shows the two sounds merged; a dark line marks where a
  crossfade begins and ends; a picked event's outline stays over the
  event overlapping it; the fade and level handles are VEGAS's blue tabs
  (the corners and the middle of the top), and over a fade's handle the
  pointer shows an arrow with a fade's curve.
- **Project Media's menu and the Trimmer like VEGAS 23's** (from the
  user's prints) — a media's right-click menu has VEGAS's 29 items in its
  order and groups, greyed as VEGAS greys them for a video file, a sound
  file and generated media: Open in Trimmer, Open Parent Media in
  Trimmer, Add as CD Track, Add at Project Frame Rate, Detect Scenes and
  Add to Timeline, Tempo Detection, Select Timeline Events, Match Project
  Video Settings, Edit Source Project, Open Nested Timeline, Explore
  Containing Folder, Create Video Proxy, Swap Video Files (also the
  toolbar's Swap Media), Remove from Current Bin, Remove from Project
  (Delete), Remove from Project and Delete File(s), Detect Scenes and
  Create Subclips, Search Media Bins, Rename, Replace…, Refresh, Media
  FX…, Color Grading, Rotate 90° both ways, Lay Out Tracks Using Media
  Timecode and Date/Time Stamp, File Format Properties… (DV AVI only:
  greyed), Properties…. The Trimmer is laid out as VEGAS's: the history
  ("name   [folder\]") with Sort, Clear, Remove and Trimmer on External
  Monitor; the picture, its loop bar (yellow handles, the media's
  numbered marker flags with their names) and time bar, or a sound's
  ruler, waveform and scroll bar with + and −; the transport in the
  middle (Loop, Play, Pause, Stop, Add to Timeline from Cursor, Fit to
  Fill, More with the other buttons and Edit Visible Button Set) and the
  time boxes. Its menus are VEGAS's: the picture's, the waveform's, the
  loop region's (Insert Region, Loop Playback, Zoom Selection, Set
  Selection to View, Select Loop Region), the bar's (everything, with
  Open in Audio Editor and Channels) and the ruler's time formats (Use
  Media or Project Frame Rate). A subclip opens alone; Copy puts the
  selection on the clipboard; its keys are VEGAS's (Space, Shift+Space,
  Enter, Ctrl+Home, Ctrl+End, Shift+L, Shift+Q, Ctrl+P, Ctrl+Shift+Up and
  Down, Alt+Left and Right, I, O, A, Shift+A, T, Shift+T, M, R, S,
  Shift+S, B, Ctrl+E, Tab).
- **The Trimmer's bar and markers like VEGAS 23's, its playback smooth**
  (from the user's print) — its loop and time bars measured on the print:
  a dark band under the picture, the loop bar 9 points tall (22 with the
  media's markers), the time bar 17 with its two light and two dark
  lines and dots at round times, the loop region's grey from its handles
  down over the time bar, yellow handles with their poles, VEGAS's cursor;
  markers as VEGAS's numbered orange pennants edged dark with their poles
  and names, regions green at both ends — on the timeline's marker bar
  too. Playing, the Trimmer's frames are drawn ahead as the timeline's,
  its project made once.
- **Videos on the adjustment track, envelope points and crossfade edges
  as VEGAS's** (from the user's prints) — media let go on an adjustment
  track, and events moved there, go on it: its events show over the
  tracks below and its effects work on all of that, its level their
  strength (both renderers alike). An envelope's point under the pointer
  turns into VEGAS's white diamond and its tooltip reads "Composite
  Level at 00:01:45:14 is 42 %" (the timeline's time as the ruler shows
  it); a double-click adding a point puts the cursor on it. A
  crossfade's dark lines trim its events from either side — the start of
  the event coming in, the end of the one going out under it. The
  Trimmer's preview plays smoothly: the interface draws every frame
  while it plays, as for the timeline.
- **V and P, envelopes drawn and tracks deleted as VEGAS's** (from the
  user's print) — V and P insert the volume and pan envelopes on the
  audio tracks picked (shown; again, hidden and shown), from the track
  list too, as VEGAS's "Insert/show/hide track volume envelope"; a volume
  envelope is drawn as VEGAS's, 0 dB in the middle of the track and +6 dB
  at the top, its line VEGAS's purple two points wide edged darker, its
  points diamonds. Several tracks picked, a right-click on one keeps them
  and Delete Track deletes them all, in one undo.
## Awaiting the maintainer's review

The VEGAS Pro 2026 documentation audit (2026-10-20): everything below
was made automatically from VEGAS's online Help, without the user's
prints or inspection, so each counts as partial until the user has
looked at it and approved it (the report "Raio-X VEGAS 2026 ×
BananaV" marks them "Parcial: aguardando sua revisão"). Approved
items move up to Done.

- **VEGAS Pro 2026 documentation audit, pack 1** — the 495 topics of the
  online Help compared with BananaV (the interactive report lists each
  feature, its state and the work order); then the quick wins: the
  ruler's Audio CD Time format (75 frames a second, no offset), its
  shortcut menu and Set Project Tempo, times typed in the ruler's format;
  the musical grid (measures, notes and triplets from the tempo) with
  snapping; the Time Display's MIDI Clock Out, Time Format and own colors
  per interface type; the Shuffle tool and Shuffle Events; events' snap
  offset flag; Synchronize by moving or by slipping; Add Missing Stream;
  the switches Trim Event to Include All Frames, Auto Normalize (worked
  out again after each edit, in its undo) and Reduce Interlace Flicker
  (on the processor and the card); the Master Bus's Downmix and Dim
  Output (playback only); the pan types (Add Channels, Balance at 0, −3
  and −6 dB, Constant Power, Film) for the pan and its envelope; the
  envelope brush (Shift+drag), Thin and Flip All/Selected Points and Lock
  Envelopes to Events.
- **Titles & Text (audit pack 2)** — BananaV's own text generator (an OFX
  plug-in, as VEGAS's): the computer's fonts (TrueType and OpenType/CFF
  through `skrifa`), size, bold, italic, justification, color, scale,
  location and anchor point, background, tracking, line spacing, outline
  and shadow, its box moved and scaled on the preview; Media Generator's
  Titles and Text category and presets, Insert › Text Media (the track
  menus and the timeline toolbar too), Transfer Text Properties; Insert
  Subtitles from File (plain or timed .txt and .csv, times in the
  ruler's format) and from Regions. Insert › Video Track now adds the
  track at the top and the track menus' Insert Video/Audio Track above
  their track, as VEGAS's.
- **Compositing (audit pack 3)** — the video tracks' Compositing Modes
  (Multiply (Mask), Source Alpha, Add, Subtract, Cut, Screen, Overlay,
  Hard Light, Dodge, Burn, Darken, Lighten, Difference, Difference
  Squared) and compositing parents and children (Make Compositing
  Parent/Child, Ctrl with the children, Parent Composite Mode), masks
  with Multiply (Mask), on the processor and the graphics card alike.
- **Adjustment events and tracks, Loudness Meters (audit pack 4)** —
  Insert › Adjustment Event (the Media Generator's Adjustment Event: its
  effects, Pan/Crop and level work on what is composited below it) and
  Insert › Adjustment Track (its track effects work on the tracks below,
  or on its compositing children, its level their strength; it takes no
  events, its header marked), Resize Selected/All Adjustment Events to
  the project's length, on the processor and the graphics card alike.
  The Loudness Meters window (View › Window, Ctrl+Alt+7): momentary,
  short-term and integrated loudness, loudness range and true peaks (four
  times oversampled, ITU-R BS.1770-4's Annex 2) of what plays, measured
  from each playback's start; EBU R 128 or ATSC A/85 targets with the
  over-target marks, the EBU +9 and EBU +18 scales in LU or LUFS, the
  true peak blocking filter, peak and valley holds, labels, Reset Clip and
  Reset Meter Measurement (the window's shortcut menu and Options ›
  Loudness Meters). In pt-BR, timeline tracks are "trilhas" throughout.
- **Velocity, freeze frame, reverse, transition progress (audit pack 5)**
  — video events' Velocity envelope (−100 % to 300 %: slow motion, speed
  up, backward until the media's first frame, held at 0; its points'
  presets), Freeze Frame at Cursor as VEGAS's (the velocity dropping to 0
  at the cursor, on every selected event there), the Transition Progress
  envelope (a transition held, reversed or repeated, BananaV's and OFX
  transitions alike), Reverse for audio and video events (an arrow on
  them, waveforms and thumbnails backward), View › Show Envelopes and
  Insert › Video Envelopes' Event Velocity and Transition Progress; trims,
  splits, Paste Event Attributes and OpenTimelineIO keep them (a reversed
  event a negative time scalar).
- **Track Motion (audit pack 6)** — Tools › Video › Track Motion and the
  video track's More menu: where the track's picture goes in the frame
  (position, width and height, angle; dragged on the workspace —
  moving, resizing from the center or an edge, turning — or typed), its
  keyframes on the timeline with their curve and smoothness (Options ›
  Preferences' "Track Motion default smoothness"), the 2D Shadow and 2D
  Glow rows (on or off, blur, intensity, color, offset and size, each
  keyframed), Enable Rotation, Snapping to Grid, Prevent Movement and
  Scaling per axis, Lock Aspect Ratio, Scale About Center, the
  workspace's Restore, Center, Flip and aspect commands, the Composite
  Mode; a compositing parent's Parent Motion moves its whole group. On
  the processor and the graphics card alike.
- **VEGAS's transitions (audit pack 7)** — BananaV's own OFX transitions
  with VEGAS's settings and presets: Linear Wipe, Clock Wipe, Iris (seven
  shapes, repeated in rows and columns), Barn Door, Venetian Blinds, Star
  Wipe, Push, Slide, Squeeze, Split, Swap, Zoom, Spiral, Flash,
  Dissolve, Cross Effect and Portals (borders, feathers, directions,
  centers), in the Transitions window's 3D Effects, Reveals, Wipes and
  Fades, their presets drawn by them (animated under the pointer); the
  OFX bundle carries them to other hosts.
- **More video effects (audit pack 8)** — VEGAS's Tiny Planet, Defocus
  (iris shapes, blooms), Linear Blur, Quick Blur, Radial Blur,
  Median, Min and Max, Convolution Kernel, Spherize, Swirl, Threshold,
  Wave, Pinch/Punch, Offset and Wrap, Black Restore, Broadcast Colors,
  Channel Blend, Chroma Blur, Mask Generator, Lens Correction (k1–k3,
  its grid; since 2.0 VEGAS's lens database, from lensfun's, and its
  PanoTools model), Radial Pixelate and Deform, with their
  settings and presets, as BananaV's own OFX effects.
- **Color, light and film effects (audit pack 9)** — VEGAS's LAB Adjust,
  Soft Contrast (with its vignette), Gradient Map (three control
  points), Fill Light, Lens Flare, Starburst, Glint, Light Rays, Film
  Effects (dust, hairs and scratches each with its amount), Film Grain,
  Newsprint, TV Simulator, Bump Map (as a filter), Denoise (non-local
  means) and Looks (AutoLooks, with BananaV's own looks), with their
  settings and presets, as BananaV's own OFX effects.
- **Media generators and Credit Roll (audit pack 10)** — VEGAS's
  Checkerboard (square, edge blending, offset), Color Gradient (linear,
  elliptical, rectangular, angular; up to four control points), Noise
  Texture (BananaV's own styles, layers, frequency, offset, progress, min,
  max, bias, amplitude, grain) and Test Pattern (SMPTE bars at 75 and
  100 %, full-field bars, grays, grid) as BananaV's own OFX generators,
  with presets; Credit Roll (VEGAS's text format, title, item and
  two-column styles, scrolling over its event or a timed sequence with
  ways in and out); generator plug-ins are told their event's length, and
  their thumbnails follow the time; the Media Generator window lists them
  (the older built-in generators stay for old projects).
- **3D and page transitions (audit pack 11)** — VEGAS's 3D Blinds
  (sections, extra spins, stagger), 3D Cascade (sections, twist), 3D Fly
  In/Out (the far point, rotations about three axes, in or out), 3D
  Shuffle, and its Loop and Peels — Page Loop, Page Peel and Page Roll
  (angle, the fold's radius, slide, opacity, perspective, the light's
  color) — with specular light and presets, as BananaV's own OFX
  transitions; the Transitions window's Loop and Peels category opens.
- **VEGAS's everyday video effects with its controls (audit pack 12)** —
  Chroma Keyer (color, thresholds, blur, the mask) as BananaV's own OFX
  effect with presets (Brightness and Contrast, Levels, Black and White,
  Sepia, Invert, Gaussian Blur, Sharpen, Vignette, Mirror and Glow, made
  from VEGAS's Help here, have since been measured on VEGAS's pictures
  and are done); the older built-in ones are no longer offered (projects
  that have them keep them).
- **Takes (audit pack 13)** — events keep other media as takes, VEGAS's
  way: Add as Takes (media dragged onto an event with the right mouse
  button), Next and Previous Take (T, Shift+T), Choose Active… and
  Delete… in the Take Chooser, Delete Active, Rename Active (F2), the
  takes listed in the Take submenu, the active take's name on the event
  (its number among them with Active Take Information), and Options ›
  Slip All Takes; trims and splits keep each take in its place.
- **Multicamera editing (audit pack 14)** — Tools › Multicamera: Create
  Multicamera Track (the selected events' tracks are the cameras, in
  their order; an event wherever a camera starts or stops, a take per
  camera, "(no camera)" where one did not record), Expand to Multiple
  Tracks… (the unused takes kept muted or deleted), Enable Multicamera
  Editing (Ctrl+Shift+D: the Video Preview shows every take in tiles,
  the active one outlined in the indicator color, the takes' numbers and
  names as Preferences say; a click or a number key cuts to a take at
  the cursor, playing or not) and Edit Multicamera Audio with Video.
- **Multicamera synchronization (audit pack 15)** — Tools › Multicamera ›
  Sync Mixed Audio to Align Events (the events picked after the first
  move to where their sound matches its sound: onset envelopes compared
  at every offset through an FFT, then the sound itself to a fraction of
  a sample; in the background, Cancel in the status bar; one sound heard
  through two microphones found, unrelated sounds refused, in the
  tests), Sync Media Timecode and Sync Date/Timestamp to Align Events
  (MP4/MOV timecode tracks and creation times, Broadcast Wave time
  references and origination dates, the files' times); Ctrl with a take
  makes a crossfade; a take chosen with the cursor in a time selection
  recuts all of it, joining what plays on in one take; Show Full Frame on
  External Monitor (otherwise the external monitor shows the tiles).
- **GL Transitions (audit pack 16)** — VEGAS 2026's GL Transition: 65
  transitions of the open GL Transitions collection (MIT), ported from
  their GLSL to BananaV's own OFX transition — Fade, Fade Color and
  Grayscale, the directional ones and the wipes, Circle, Heart, Angular,
  Radial, Pinwheel, the blinds and slices, the squares, Pixelize, Burn,
  Color Phase, Crosshatch, Cube, Doorway, Swap, Kaleidoscope, Morph,
  Perlin, Ripple, Wind, Hexagonalize, Cross Zoom, Bounce, the bow ties,
  Mosaic, Grid Flip, Swirl, TV Static, Water Drop, Film Burn, Book Flip
  and more —, each a preset with its own controls, in the Transitions
  window's GL Transitions category. Typing GLSL of one's own came with
  audit pack 137 (the list's Custom).
- **Compositors (audit pack 17)** — a video track's Compositing Mode (or
  a parent's Parent Composite Mode) can be Custom, as VEGAS's: a
  compositor plug-in draws what is below the track as the track's picture
  guides it — Displacement Map (its channels push the pixels; scales,
  channels, invert, edges, alpha, flip) and Height Map (light bent through
  a water surface the picture shapes; amplitude, elevation, height scale,
  smoothness…), BananaV's own OFX plug-ins with a second input, and Bump
  Map, a video effect that lights its picture by another's bumps there —;
  View › Window › Compositors lists them with animated presets (double-
  click gives one to the track), and the compositor's settings open in a
  window of their own; on the processor and on the card alike.
- **Flicker Control and Mesh Warp (audit pack 18)** — Flicker Control
  takes the flicker of time-lapses and lamps away by comparing each frame
  with up to three on each side (Temporal blend, with motion
  compensation, or Histogram smoothing; Sensitivity, the tones it works
  on, a mask of where it works, Reset) or adds a flicker of a given
  frequency; Calculate is greyed, as BananaV works the frames out as it
  plays. Mesh Warp bends the picture by a grid of up to 20 × 20 cells
  whose points are dragged on the preview (double-click puts one back,
  Reset Mesh all of them; Elasticity pulls the others along, Fixed
  Border Points keeps the edge; the grid's size locked once a point
  moved), its mesh animated by keyframes. A double click an overlay
  takes no longer resets the preview's zoom.
- **Color Match (audit pack 19)** — brings a picture's colors to those of
  a source image as a target image (a sample of it) shows they differ —
  with Match Brightness, the brightness too —, mixed in by Strength; the
  images come from the Trimmer's frame, a picture file or the Video
  Preview (the target's: the event's own picture before the effect), a
  mosaic of each under its row. Taking them from the clipboard or a part
  of the screen waits (greyed). Plug-ins' push buttons can ask BananaV
  for such pictures, and buttons share a line as OpenFX's layout hint
  says.
- **Beat Detection and Tempo Detection (audit pack 20)** — an event's
  menu (and Project Media's, and the Trimmer's metronome button) finds
  the beats of its media's sound — the hits of its low end — and shows
  them in the Trimmer while Sensitivity is set (the higher, the fewer);
  OK makes them the media's markers, which the events show and snap to.
  Project Media's Tempo Detection finds the tempo and, as asked, makes it
  the project's, sets the ruler to measures and beats, turns the
  metronome on and snaps to the grid.
- **Apply Non-Real-Time Event FX (audit pack 21)** — Tools › Audio renders
  the sound an audio event plays through its audio effects into a new WAV
  file (16- or 24-bit PCM, or 32-bit floating point), in the background;
  the file joins the project and becomes the event's new take, and the
  effects come off the event (as asked) since the take has them.
- **Auto Ducking (audit pack 22)** — an audio track's menu makes it an
  Auto Ducking controller (its events' loudness read in the background)
  or a listener; listeners go down by the controller's Floor Reduction
  while it sounds, and more on its peaks by its Peak Reduction, drawn on
  them as a dotted line; an event's Re-Analyze Auto Ducking reads it again
  and gives it reductions of its own.
- **Modern Equalizer (audit pack 23)** — an audio effect of up to eight
  bands (high and low pass with a slope of 6 to 36 dB an octave, low and
  high shelving, band pass, peaking), each on both channels, the left, the
  right, the middle or the sides; Global Scale, Mono Below, Stereo Width
  and Output Gain; its editor draws the curves on a graph — circles
  dragged for frequency and gain, the wheel for Q, double-clicks adding
  and deleting bands, Cut, Copy and Replace, a toolbar of the bands,
  knobs, A/B comparison and zoom — and has factory presets. Still to
  come: its FFT analyzer, its peak meters and oversampling.
- **VST 3 effects (audit pack 24)** — BananaV hosts VST 3 audio effects
  (written from the SDK's MIT headers): the plug-ins found in
  Preferences › VST Effects' folders are offered in the Plug-In
  Chooser's VST3 folder; an effect runs in the mixer's chains (mono,
  stereo or 5.1's front pair), keeps its parameters and state in the
  project, and shows in the FX window as BananaV's generic rows (the
  plug-in's own texts) and in the plug-in's own window on Windows
  (Preferences' "Generic VST editor" keeps to BananaV's rows). Since
  then: presets through the preset bar (audit pack 125), parameter
  automation (127), latency compensation (128 and 133) and the plug-in's
  window on Linux (136), and along busses and chains (140). Still to
  come: instruments.
- **Audio busses and assignable FX (audit pack 25)** — as VEGAS's Help
  and its signal-flow diagram describe them: up to 26 busses (Insert ›
  Audio Bus, the Mixing Console's Insert Bus, Project Properties' number
  of stereo busses) and 32 assignable effects chains (Insert › Audio
  Assignable FX…: the Plug-In Chooser, then the chain; Tools › Audio ›
  Remove Assignable FX). A track plays into the Master or a bus (its
  header's bus button, the console's output) and sends to busses and
  chains (the header's multipurpose slider, the console's sends): from
  before its volume and mute — busses' default, a cue mix, playing on
  while the track is muted unless Preferences › Audio's "Track prefader
  sends listen to mute" — or after them (chains' default), panned as the
  track (Link to Main Track Pan) or from the middle. A bus runs its
  effects before its pan and volume or after them (Pre/Post Fader Insert
  FX), sends to chains from before its pan, volume and mute or after
  them, and plays into the Master or another bus; a chain takes its sends
  at its input level through its effects, pan and volume into the Master
  or a bus; nothing plays in a circle, a deleted bus's tracks go to the
  bus before it, solo on a bus or chain mutes the others. The Master
  Bus's FX (Post Volume on its right-click, Pre FX/Post FX on its fader's).
  The Mixing Console shows a strip per track, chain and bus (effects,
  output, sends or input level, pan, fader with its meter, mute, solo,
  renaming with a double-click or F2, Delete), its toolbar inserts them,
  U and E show or hide busses and chains. Since then: the bus tracks
  and their envelopes, input busses and hardware outputs per bus (audit
  pack 134: the output device's pairs). Still to come: the legacy send
  gain (the Help only says it imitates VEGAS 7).
- **Mixing Console complete (audit pack 26)** — as VEGAS's Help and its
  picture of the window show it: the toolbar (the project's sound
  format, Views, Properties and Layout with Show Channels, Show Control
  Regions, region labels, fader ticks, channel width and Meter Layout,
  Downmix, Dim, the inserts), the Channel List (Shift+C: a checkbox per
  channel, a click picks it) and the View Controls (Shift+R: channel
  types, control regions, width), and each strip's regions: Insert FX
  (the effects as buttons, red when bypassed; Show, Bypass, Remove,
  Presets and replace on the right-click; + adds one), Sends, I/O (the
  recording input and the output), VU Meters (Traditional, Extended and
  Logarithmic VU, UK, EBU, DIN and Nordic PPM with IEC ballistics,
  labels, peaks, held peaks and valleys, Vintage Mode, Reset Clip; the
  mouse wheel goes through the kinds), Meters (peaks) and Faders
  (automation controls and mode, mute, solo, phase, arm, pan, the fader
  with its ticks and peak meter — the wheel, Ctrl+drag, the value typed
  with a double-click —, a bus's or chain's Pre/Post Fader Insert FX);
  the keys I, S, H, M, F, Shift+T and Shift+L. Still to come: the
  meters' digital alignment as a setting (0 dBu at −18 dBFS now, EBU
  R68), channels reordered by dragging their names, ganged edits of
  several channels, the Sends region's divider, input monitoring, input
  and preview busses.
- **The Color Grading panel (audit pack 27)** — as VEGAS 2026's Help and
  its pictures show it: BananaV's own Color Grading effect (a Camera LUT —
  a `.cube` file or Apple Log's published curve to linear or Rec. 709 —,
  Color Wheels Lift, Gamma, Gain and Offset, RL Color Wheels with two
  thresholds, Levels, Tone, White Balance, HSL, Color Curves, HSL Curves
  wrapping round red, a Look LUT — BananaV's looks or a file —, each LUT's
  strength, interpolation and LUT Selection) edited by the panel at the
  event's, the track's or the media's level: the navigation's sections
  (each wheel a wheel or sliders, the curves pinned beside the others),
  Bypass Color Grading, Reset All, More's Broadcast Colors and Export LUT
  (a 33-point `.cube`). Still to come: the pickers (black and white
  points, Select Neutral Tone, the HSL Curves' Color Selector), Show
  Limits on Scopes, the Video Output level, More's Bézier Mask, Samsung
  Log, the Color Curves' dark look and side by side, dragging the hue
  spectrum, and VEGAS's own math once prints measure it. (The graphics
  card kernel came with audit pack 139.)
- **Adjust source media (audit pack 65)** — as VEGAS 2026's Help describes
  it ("Setting project properties"): File › Properties › Video's "Adjust
  source media to better match project or render settings", on for new
  projects: DV widescreen media in HD projects, HD media in DV widescreen
  projects and 486-line media in 480-line projects are enlarged to fill
  the frame (cropped) instead of leaving thin bars; 480 lines in 486 stay
  padded. Still to come: DV cropped for 320x240 renders (renders to other
  sizes do not take the pixel aspect into account yet).
- **VEGAS 2026's new preferences (audit pack 64)** — as VEGAS 2026's Help
  describes them (Display Tab, File I/O Tab): Display's "Reverse Event
  Background color" (events in their track's color, their waveforms in the
  darker one) and "Display timeline at bottom of main window" (off, the
  timeline's window goes above the docking area; at startup too); File
  I/O's Log Plugin loading, Log Reads and Log Renders, Off, Basic (a line
  each) or Detailed (what each one found): the OpenFX and VST plug-ins
  found, the media imported (or why not) and the renders and real-time
  renders (their settings, how they ended), in `Plugins.log`, `Reads.log`
  and `Renders.log` under the local data's `BananaV\Logs` folder.
- **GLSL arrays and macros with parameters (audit pack 141)** — in the
  GL Transition's Custom code: arrays of constant sizes (constructors,
  indexing, whole assignment, `.length()`) and `#define`s with parameters.
- **Plug-in delay compensation along the routing (audit pack 140)** —
  busses and chains whose effects answer late: their tracks read that
  much earlier, the paths that answer earlier through delay lines.
- **Color Grading on the graphics card (audit pack 139)** — its LUTs as
  LUT Filter's kernel (or Apple Log's curve) and its own steps — the
  Color Wheels, the RL Color Wheels, Levels and Tone, White Balance, HSL,
  the Curves and the HSL Curves — in one kernel; a test compares them with
  the processor.
- **The transitions on the graphics card (audit pack 138)** — BananaV's 24
  own transitions (the wipes, the irises and stars, Push, Slide, Squeeze,
  Split, Swap, Zoom, Spiral, Flash, Dissolve, Cross Effect, Portals, the 3D
  ones and the peels) draw on the card the same math as on the processor,
  every preset compared by a test; the GL Transitions still go through the
  processor.
- **GL Transition's Custom (audit pack 137)** — GLSL typed or pasted
  into the GL Transition, as VEGAS 2026's code window: BananaV's own GLSL
  interpreter (no new dependencies) runs a fragment shader's GLSL ES 1.0
  and what GL Transitions' code uses of GLSL 3, with their harness
  (`progress`, `ratio`, `getFromColor`, `getToColor`); the code's
  uniforms take controls of their kind (eight numbers, two points, two
  colors) set to its `// = …` defaults, a line says which is which, and
  an error shows as the effect's message (the transition then fades).
  The collection's GLSL of six transitions draws what the ports draw.
  About a third of a second a 1080p frame on one processor thread.
  Still to come: structs and derivatives (arrays and macros with
  parameters came with audit pack 141).
- **VST 3 plug-ins' windows on Linux (audit pack 136)** — an X11 window
  and VST 3's Linux run loop; checked with clippy for Linux, not tried on
  a Linux desktop yet.
- **The Master's Audio Device Selector (audit pack 135)** — the Master's
  pair (in 5.1 its front, center/LFE and rear pairs) on the output
  device's pairs.
- **Busses on hardware outputs (audit pack 134)** — a bus on a pair of
  the output device's channels: heard while playing, left out of
  renders; other busses send to it; with an input bus, hardware effects.
- **Plug-in delay compensation on events (audit pack 133)** — an event
  whose effects answer late is read that much ahead.
- **Titles & Text: parts of the text in styles of their own (audit pack
  132)** — the text box's selection takes its own font, size, bold
  and italic.
- **Reverse playback decodes ahead (audit pack 131)** — a reader played
  backward decodes the stretch before the picture asked for.
- **Nested projects' prerendered video (audit pack 130)** — used where it
  still shows what the nested project does, as VEGAS's Help says.
- **Bézier Masking's own tracking (audit pack 129)** — each mask's
  Tracking › Start: Precision, Mode, Direction and Keyframe Interval, the
  mask's location, angle and size keyframed.
- **Plug-in delay compensation on tracks (audit pack 128)** — a track
  whose effects answer late is read that much earlier.
- **FX Automation of VST 3 effects' parameters (audit pack 127)**.
- **The video bus track's keyframes rows (audit pack 126)** — the Video
  Output FX's keyframes under the track, moved and deleted there.
- **VST 3 effects' presets in the preset bar (audit pack 125)**.
- **MXF and DV start timecode (audit pack 124)** — for Sync Media
  Timecode.
- **Video Stabilization's Expert view (audit pack 123)** — the analysis's
  own work (Analysis Width, Feature Points, Iterations, which Fast and
  Accurate preset), each motion component smoothed or not over time, and
  Show Points with Minimum Track Length (the features followed and their
  tracks over the next frames, for the session).
- **Color Grading's Color Curves (audit pack 122)** — as VEGAS 2026's
  Help describes them: Brightness alone at first, Select Curves, zoom and
  pan, Page Up/Down, the tangents' keys, Lock Tangents, and four graphs
  side by side once the section is wide.
- **Input busses' sends (audit pack 121)** — to assignable effects
  chains, after or before the bus's volume, heard live through the
  chain's effects.
- **Reverse's subclip (audit pack 120)** — a reversed event's subclip in
  Project Media, as VEGAS's Help says.
- **Mackie Control's Shift+Meters/Values (audit pack 119)** — the strips
  over the tracks, the busses, or both.
- **Mixing Console's channel order, ganged edits, Sends divider and Input
  Monitor Mode (audit pack 118)**.
- **The fonts' own kerning (audit pack 117)** — Titles & Text always,
  Credit Roll's styles by their Kerning checkbox.
- **Gradient Wipe (audit pack 116)** — the transition, from a gradient
  picture or BananaV's shapes.
- **The Modern Equalizer's analyzer, meters and oversampling (audit pack
  115)** — as VEGAS's Help ("Using the Modern Equalizer") describes them:
  while it plays, its output's spectrum behind the curves (the FFT
  analyzer) and its input's and output's peak meters, each turned off in
  Settings, and Settings' Oversampling (Off, 2×, 4×, 8×).
- **A media's Color Range (audit pack 114)** — Media Properties' Color
  range, Limited or Full (no level conversion of its Y′CbCr), in
  full-range projects.
- **Double-click and drag on an event (audit pack 113)** — as VEGAS's
  Help ("Creating selections and positioning the cursor") says: the time
  selection extended from the event to the next fade or event edge.
- **The Mackie Control's presets in PE (audit pack 112)** — V-Pot 6
  turns through the edited effect's presets and takes one when pressed.
- **The video bus track's Fade Colors (audit pack 111)** — its menu's
  Fade Colors (Top…, Bottom…), as a video track's: the colors its Fade to
  Color envelope fades to, from the color picker.
- **The video bus track's automation (audit pack 110)** — as VEGAS's
  Help ("Video bus track") describes it: its header's Automation Settings
  button and menu, its Motion Blur and Supersampling sliders (with
  automation off the whole output's amount; in Read following the
  envelopes; in Touch and Latch writing them, the envelope added when
  missing) and its Mute as an automation control.
- **Bypass Motion Blur (audit pack 109)** — as VEGAS's Help ("Video
  track controls", "Using a Mackie Control") describes it: a video track's
  header button (MB) and More menu item leave the track out of the video
  bus's motion blur and supersampling (drawn at the frame's time in every
  sub-frame); a Mackie Control's Input button in Video mode (Motion Blur)
  toggles it with each strip's V-Pot.
- **HSL Curves' tangents (audit pack 108)** — as VEGAS's Help ("HSL
  Curves") describes them: the tangents' handles (the picked point's, or
  every point's: Selected tangents and All tangents), dragged; the
  point's menu with Lock Tangents, Smooth curves, Lock axis (the hue kept)
  and Reset; the arrow keys move the point picked, with Shift its
  tangents (Ctrl: finer).
- **Motion Tracking's import from an effect (audit pack 107)** — as
  VEGAS's Help ("Motion Tracking") describes it: when the event has a
  Picture In Picture or a Bézier Masking, the import menu beside the
  regions' + and − makes a region following its picture or a mask (at
  each of the event's frames when the effect is animated). Import from
  Mocha waits for a file Mocha exported (its format is not published).
- **Smart Split's Warp Flow (audit pack 106)** — as VEGAS's Help ("Smart
  Splitting Events") describes it: the video parts Smart Split leaves
  are joined by a Warp Flow transition (a crossfade where the plug-in
  did not load), the sound by a crossfade.
- **The Optical Flow resample mode (audit pack 105)** — VEGAS's third
  Resample mode (File › Properties › Video, an event's switch): where a
  file shows at another rate than its own, the frame between its frames
  is made along the dense optical flow from one to the next, each pixel
  from both along its move, instead of the two mixed (on the processor,
  each pair's flow kept for the frames between it).
- **Copies of generated media (audit pack 104)** — as VEGAS's Help
  ("Adding generated media to your project") describes them: events
  pasted (Paste, Paste Insert, Paste Repeat), copied by the right-drag's
  Copy Here or dragged from Project Media while another event shows
  their generated media ask whether to create a new copy of the source
  media (each edited on its own) or a reference to the original (a change
  to either changes both).
- **Adjustment tracks' Composite envelope; Media Properties and Replace
  Plug-In (audit pack 103)** — as VEGAS's Help ("Adjustment Tracks",
  "Adding generated media to your project") describes them: a new
  adjustment track has its Composite envelope along its top, to raise
  and lower its effects over time; the Video Media Generators window's
  Media Properties opens the generated media's properties and Replace
  Plug-In puts another generator (its first preset) in the event's place.
- **Credit Roll's files and Position box (audit pack 102)** — as
  VEGAS's Help ("Credit Roll") describes them: Import Text (in place of
  the sample credits, else after them), Import Properties, Export Text
  and Export Properties as file parameters (a dialog to save for the
  exports), read or written once picked, an error shown over the
  settings until a file works; the Position group's box with the
  margins' yellow handles (the top too in a timed sequence), each
  effect showing only its own settings (the direction, or In, Out and
  Display; Top greyed while scrolling), Top's default 0.1 as VEGAS's.
- **Track headers' meters and heights (audit pack 101)** — as VEGAS's
  Help ("Audio track controls") describes them: the output meters'
  Use Vertical Meters off lays them across the header, "Out" and the
  peak beside; Minimize and Maximize bring back the height a track had,
  a track minimized alone shows Restore Track Height, ` minimizes every
  track and brings them back, Ctrl+` sets the default height on all. With
  several tracks selected (Shift, Ctrl), dragging one's bottom edge sizes
  them all alike, as VEGAS's.
- **Track Motion's presets, object space and keys (audit pack 100)** —
  as VEGAS's Help describes the Track Motion window: the Preset row
  (type a name, Save Preset, pick one to set the row at the cursor,
  Delete Preset; 2D and 3D presets apart), Edit in Object Space (the
  Prevent Movement buttons hold the box's own axes, in 2D and in 3D) and
  the TrackMotion keyboard context (1 to 6: the 3D layouts; A, C, X, Y,
  Z, Shift+X/Y/Z, F8, Shift+F8, O: the toolbar's toggles).
- **Takes (audit pack 99)** — the Take Chooser's Play and Stop preview
  the take picked (VEGAS's "Using takes as alternate versions of
  events"), and Open Copy in Audio Editor keeps the copy as the event's
  active take, the original one of its takes, as VEGAS does.
- **Re-reading changed media (audit pack 98)** — a media file changed
  on disk while BananaV was away (rendered again from its source project,
  which Edit Source Project opened) is described and read anew once
  BananaV is the active program again, its events following it, as
  VEGAS's rendered files with a project path reference do.
- **3D Track Motion's workspace (audit pack 97)** — as VEGAS's Help
  ("3D compositing") describes it: the Layout list (VEGAS's six: one pane,
  two side by side or one above the other, three with a large one on the
  left or on top, four), each pane showing the box from its
  view (Camera Front and Rear, Perspective, Top, Bottom, Left, Right); in
  Camera Front the trackball — the corners bring the box nearer or
  farther, inside moves it, the circle's top and bottom turn it about x,
  its sides about y, the rest of its rim about z, the guide drawn bold —;
  the flat views move it along their axes; Prevent Movement (Z). The
  views' names and the trackball's look wait for your prints.
- **Archiving nested projects (audit pack 96)** — the Archive dialog's
  Include nested projects (VEGAS's "Archiving projects"): each nested
  project's file is archived with the project, written pointing at the
  copies of its own media (nested projects deeper alike); off, nested
  projects stay where they are.
- **Burning audio CDs (audit pack 95)** — VEGAS's Tools › Burn Disc ›
  Track-at-Once Audio CD and Disc-at-Once Audio CD as its Help describes
  them: the Track-at-Once dialog (the file's length and the time left on
  the disc, the Action — Burn audio, Close disc, Erase RW disc —, Buffer
  underrun protection, Erase RW disc before burning, Close disc when done
  burning, Eject disc when done, Burn selection only, the drive and its
  speed) and the Disc-at-Once dialog (the drive, its speed, Buffer
  underrun protection, Burn CDs, Automatically erase rewritable discs,
  Eject when done) render the project's sound as CD audio and burn it
  with the system's recording service — a track, or the CD layout's
  tracks with their indices, ISRCs, copy and pre-emphasis flags and the
  disc's catalog number. The test runs wait (the service has none). Not
  burned on a real drive yet (none here): the Disc-at-Once image is made
  by the service in the tests.
- **B-pictures (audit pack 94)** — BananaV's MPEG-1 and MPEG-2
  encoder codes B-pictures (IBBP, as DVD's and Video CD's): each
  macroblock of a B-picture predicted from the picture before, the one
  after or both, whichever is nearest; the MPEG files and the DVD's video
  objects carry their decoding and showing times apart. GOPs stay closed.
- **MPEG-1 rendering (audit pack 93)** — the Render dialog's MPEG-1
  format ("Rendering MPEG Files": files for a Video CD program), with VCD
  NTSC (352 × 240, 29.97) and VCD PAL (352 × 288, 25) templates at 1,150
  kbit/s and MPEG audio at 224: BananaV's MPEG-1 video (its MPEG-2
  encoder without MPEG-2's extensions) and the system's MPEG audio
  encoder in an MPEG-1 system stream of Video CD's 2324-byte packs.
- **Burn DVD (audit pack 92)** — VEGAS's Tools › Burn Disc › DVD
  ("Burning a DVD from the timeline"): the Burn DVD dialog (Video format —
  DVD NTSC or PAL, standard or widescreen, the project's picture
  letterboxed in the disc's shape —, the Audio format it renders, Burn
  drive, Burn speed, Eject when done) renders the project into a
  DVD-Video folder, makes it a disc image (UDF 1.02 bridged with ISO
  9660, as DVD-Video discs are) and burns it with the system's recording
  service, the steps shown in a window with Cancel. Not burned on a real
  drive yet (none here): the image is checked by reading it back.
- **DVD-Video authoring (audit pack 91)** — the first part of VEGAS's
  Tools › Burn Disc › DVD ("Burning a DVD from the timeline"): the project
  rendered into a DVD-Video folder (`VIDEO_TS`, `AUDIO_TS`) with
  BananaV's MPEG-2 video and linear PCM sound in VOBUs with navigation
  packs, a chapter at each marker, and the information files a player
  reads. Still to come: the Burn DVD dialog, the disc image and burning.
- **MPEG-2 rendering (audit pack 90)** — as VEGAS 2026's Help describes it
  ("Rendering MPEG Files"): the MPEG-2 templates (DVD NTSC and PAL video
  streams, Blu-ray 1080-24p) render with BananaV's own MPEG-2 video
  encoder (I and P pictures, closed GOPs, motion search, rate control)
  and the system's MPEG audio encoder (linear PCM where there is none)
  into a program stream of DVD-sized packs written by BananaV, the
  picture's 4:3 or 16:9 shape in its headers, widths of whole
  macroblocks, I-frames at the markers when they are saved. Still to
  come: MPEG-1 (VCD), B-pictures, DVD authoring and burning.
- **3D compositing (audit pack 89)** — as VEGAS 2026's Help describes it
  ("3D compositing"): the 3D Source Alpha compositing mode (and Parent
  Composite Mode) places a track's picture (a group's) on a box in space
  with 3D Track Motion — Position X, Y, Z, Width and Height, Orientation
  and Rotation about X, Y and Z, keyframed —; 3D tracks next to one
  another are drawn together, intersecting where they cross, and a 2D
  track between them is a barrier. Still to come: the Track Motion
  window's layouts (the other points of view), its 3D drag handles and
  its Z buttons (Prevent Movement and Scaling); a compositing parent's
  3D children keep their own depth only inside the group.
- **Screen capture (audit pack 88)** — as VEGAS 2026's Help describes it
  ("Screen Recording with Vegas Capture"): File › Screen Capture and the
  Project Media toolbar open a window where displays, application
  windows, cameras, inputs and the computer's sound are added as live
  cards with their own settings, recorded together into a file each,
  then added to the project on tracks of their own; its Settings keep
  the recording folder (with its free space) and the default codecs and
  quality (with their size estimates). Still to come: each window's own
  application sound.
- **Smart rendering (audit pack 87)** — as VEGAS 2026's Help describes it
  ("Smart rendering"): with Options › Preferences › File I/O's "Enable
  no-recompress rendering", frames left as their media had them pass into
  a ProRes render as they are, without recompressing, when the media's
  profile, size, frame rate and fields match the template's (BananaV's
  intra-frame format; VEGAS's DV, MXF and MPEG-2 formats are not written
  by BananaV).
- **Nested timelines (audit pack 86)** — as VEGAS 2026's Help describes
  them ("Nested Timelines"): Create Nested Timeline (Alt+C, the toolbar,
  the event's menu) makes the selected events a project of their own,
  saved beside the parent ("<parent>_Untitled Timeline"), which takes
  their place as one grouped video and audio event and opens; Open Nested
  Timeline (Alt+N) and Open Parent Timeline (Alt+P) go back and forth in
  the same window, with General's two warnings (an adverse nesting, and
  saving a modified nested timeline first).
- **Nested projects (audit pack 85)** — as VEGAS 2026's Help describes
  them ("Nesting projects"): a BananaV project imported (Import Media, or
  dragged to the timeline) is media — its picture drawn over a transparent
  background, its master bus mixed down to stereo, its markers and regions
  shown in its events — edited like any event; Edit Source Project opens
  it in another window, a nested project saved meanwhile is read again,
  and a project nested in itself is refused. Its sound is mixed live
  (Rebuild Offline Nested Audio reads the nested projects again). Still to
  come: nested timelines (Create Nested Timeline, Open Nested and Parent
  Timeline), a nested project's prerendered video, and Archive's Include
  nested projects.
- **Optical flow: Slow Motion, Motion Blur, Warp Flow (audit pack 84)** — as
  VEGAS 2026's Help describes them ("Slow Motion", "Motion Blur", "Warp
  Flow"): Slow Motion (Quality, Blend or Nearest Frame, an animatable
  Playback Rate, Show Optical Flow Field, Adjust Event Length), Motion
  Blur (Quality, Fixed or Adaptive, Motion Threshold's minimum and maximum,
  Blur Amount, Show Visualization's Threshold Optical Flow Field, Blur Mask
  and Highlight Motion Blur Area) and the Warp Flow transition, on a dense
  optical flow of BananaV's own. Warp Flow on Smart Split is still to
  come (the Optical Flow resample mode came in pack 105).
- **360° stitching and stabilization (audit pack 83)** — as VEGAS 2026's
  Help describes them ("Stitching", "360° Video Stabilization"): Dual
  Fish Eye Stitching (Video FX's 360 category) makes one equirectangular
  panorama of a two-lens camera's fish-eye pictures — Field of view, Seam
  blend, Remove vignetting with its Intensity and Range, each lens's
  Refinement (yaw, pitch, roll, offsets, radius) and presets for 190° to
  210° lenses —; 360° Video Stabilization turns the sphere back against
  the camera's turns, analyzed in the background (Stereoscopy: none,
  top-bottom, side-by-side; Exclude views: the poles), with Smooth or
  Freeze motion. Presets named after cameras are still to come.
- **Video Stabilization (audit pack 82)** — as VEGAS 2026's Help describes
  it ("Stabilizing Video Clips"): an effect for events and media (Video
  FX's Utility): User view (Basic, Professional, Expert), Method (Fast,
  Accurate), Motion Compensation (translation, rotation, zoom), Analyze
  Motion (the motion followed in the background, the status bar showing
  how far, with Cancel) and Clear Motion Data, Smooth motion with
  Smoothing or Freeze motion on a reference frame, Auto Zoom, Additional
  Zoom, Rotation and Translation, Edge Filling (transparent, mirrored,
  repeated, the edge held) and Add Motion Blur with its Threshold.
  Expert mode's analysis parameters and Show Points are still to come.
- **Motion Tracking (audit pack 81)** — as VEGAS 2026's Help describes it
  ("Motion Tracking"): Tools › Video › Motion Tracking (Alt+M), an event's
  menu and its More button open the window on a video event: its regions
  (Mask's + and −), Track outside Region, Precision (Fast, Balanced,
  Accurate), Mode (Location, Rotation & Location, Scale & Location, Shape
  & Location, Perspective), Show Motion Vectors, the Motion tracked bar
  (green where frames were tracked) and its status, the track buttons
  (backward, a frame back, both ways, a frame on, forward, from the
  cursor to the event's edges or until the region is lost) and the motion
  transfer menu (the event or one beside it in time, its effects' location
  parameters, or a new Bézier Masking following the region). The region
  is drawn on the Video Preview, its corners and inside dragged. Mocha's
  data and the import from Bézier Masking or Picture in Picture are still
  to come.
- **Mask FX in track and output effects, the masks' feather and rotation
  (audit pack 80)** — as VEGAS 2026's Help describes them ("Bézier
  Masking", "Creating masks using the Pan/Crop tool"): Bézier Masking's
  Mask FX also works among a track's effects and the video output's (the
  effects before it show inside the masks, the picture the chain started
  from around them), on the processor and on the graphics card; the
  Pan/Crop window's masks show where each path's feather ends (dotted,
  inside, outside or both ways) and the picked path turns by a handle
  above its box (Shift: 15° steps). Motion tracking is still to come.
- **Sync links, Mackie Control as VEGAS lays it out, the Surround Master's
  channel faders (audit pack 79)** — as VEGAS 2026's Help describes them
  ("Using sync links", "Using a Mackie Control", "5.1 surround panning and
  mixing", "Recording track envelope and keyframe automation"): an
  event's menu creates a sync link with the selected events (they follow
  it, not the other way round; drawn shorter, outlined purple while the
  event they follow is selected), removes it, selects the linked events
  or unlinks one; the Mackie Control's buttons do what VEGAS's overlay
  says (Output/Compositing, Input, Pan/Fade to Color — 5.1's axes —,
  Sends, Inserts' PL, PE and PS, Settings with F1–F6 held, Flip, Video,
  Meters/Values, Time Fmt/Zero, Add New, Windows, View, Audio/Video,
  Project, Timeline with Shift and Ctrl, Event Trim and the jog wheel),
  up to four Extenders over their own tracks bank with it, and Configure
  Mackie Control sets the Device Type, Channel Mapping and the user's
  mappings (Option with F1–F16, the footswitches, any button: Assign,
  Clear, Clear All, Default All); in 5.1 projects the Master Bus window
  has the Surround Master's six channel faders and meters, and the
  Surround Panner's point is recorded while playing in Touch and Latch.
  Mackie's V-Pot 6 presets, Shift with Meters/Values (busses on the
  strips) and motion blur in Video mode are still to come.
- **Input monitoring, input busses, loop takes and punch-ins (audit pack
  78)** — as VEGAS 2026's Help describes them ("Recording audio", "Using
  input busses", "Input bus channel strips", "Recording using an input
  bus"): a track's Record Input menu sets Input Monitoring Off, On or Auto
  (what it records heard through its effects at its volume and pan when
  playback is stopped and while recording; Auto in a punch-in only over
  the selected events) and records from an input bus; Insert › Input Bus
  (Ctrl+Shift+I) adds one (up to 26), its strip in the Mixing Console
  (Insert FX before its fader, Input Source, Output Master or Off, its
  level, mute, pan, fader, name), heard through the Master, its effects
  recorded with the signal; with Loop Playback each lap round the loop
  region is a take of an event over it (regions in the file mark them),
  and selected events on the armed tracks get a take for each pass the
  recording made through them; Set Default Track Properties keeps the
  input monitor. Input busses' sends, hardware-based effects and outputs
  to hardware are still to come.
- **Pitch changes, Undersample, Smart adaptive, interlaced renders, several
  media's properties (audit pack 77)** — as VEGAS 2026's Help describes
  them ("Editing event properties", "Setting project properties",
  "Viewing or changing media file properties"): the selected audio
  events' pitch changes with = and − (Ctrl a cent, Shift an octave,
  Ctrl+Shift back to none) and the Properties' Pitch change, their length
  kept, Active Take Information showing it in the corner; a video event's
  Undersample rate shows that share of its media's frames, each held;
  Project Properties' Smart adaptive deinterlacing keeps still detail and
  removes the combs of motion, on the processor and the card; a render
  template's Field order renders interlaced frames (two fields drawn half
  a frame apart, the encoders told, ProRes coding each field); Media
  Properties edits several media at once with "(differing values)" and
  saves video profiles for files of a kind.
- **Final Cut Pro's speeds, titles, compound and multicam clips; MacCaption;
  captions beside renders (audit pack 76)** — as VEGAS 2026's Help
  describes them ("Adding Closed Captioning to Video Files"): Final Cut
  Pro 7 XML's clips come in at their Time Remap speed (as long as their
  media part at it), its Text generators as Titles & Text saying their
  text and its Color and Slug as solid colors; FCPXML's retimed clips
  (a constant speed, a frozen frame), titles, compound clips and
  multicam clips (the active angles' picture and sound) come in, its
  roles naming the tracks. File › Import › Closed Captioning reads
  MacCaption files (`.mcc`: the CEA-608 captions in their CEA-708
  packets), and a render writes the project's CC1 and CC2 captions
  beside its file as a Scenarist `.scc` of the same name, as VEGAS does
  for formats that do not carry captions; Tools › Scripting › Promote
  Media Closed Captioning places the captions of a `.scc` or `.mcc` file
  beside the selected events' media (`clip.mov.scc` or `clip.scc`) where
  the events show them. Still to come: speeds that
  change or reverse, Final Cut Pro 7's multiclips, MacCaption's
  CEA-708-only captions.
- **Photoshop layers and trimmed copies of the media (audit pack 75)** —
  as VEGAS 2026's Help describes them ("Adding layered PSD files to the
  timeline", "Saving and renaming a project (Save As)"): a layered PSD's
  layers are read when they are compressed with ZIP (with or without
  prediction; the merged image too), in CMYK and through their layer
  masks (a vector mask's pixel mask too); Add Across Tracks gives each
  layer's track the compositing mode nearest its blend mode (Multiply,
  Screen, Overlay, Hard Light, Color Dodge, Color Burn, Darken, Lighten,
  Difference, Linear Dodge, Subtract) when the track holds only it. Save
  As with Copy Media with Project asks how: Copy source media, or Create
  trimmed copies of source media with an Extra head and tail — the sound
  files' parts events use rendered to WAV files (an event going round
  with Loop keeps its file whole), the events pointing into them, the
  inactive takes removed; video files copied whole, as VEGAS copies the
  compressed ones.
- **Parts on thumbnails, storyboards' drops, the Explorer's picks (audit
  pack 74)** — as VEGAS 2026's Help describes them ("Project Media Hover
  Scrubbing", "Using Storyboards", "Tagging media files"): on a video's
  Project Media thumbnail a drag across picks a part (lighter between two
  lines; its ends dragged; I and O over the thumbnail set them where the
  pointer is), which a drag down, a double-click and Place at Cursor add
  to the timeline (the Trimmer's picture dragged down takes its selection
  too); a storyboard's cells take a part the same way (Alt still moves a
  cell, a drag down takes them to the timeline), the thumbnails and cells
  scrub under the pointer; media held over Storyboard Bins open it, then a
  storyboard; media let go on a storyboard leave the custom bin they came
  from (another storyboard's cells leave it), Ctrl copies instead. The
  Details view has a Tags column, and media let go on a tag of Tagged
  Media get it. The Explorer picks several files with Ctrl and Shift:
  dragged, added to the timeline or to Project Media and deleted
  together. Files, media and storyboard cells dropped together undo in
  one step (their imports too).
- **The Trimmer's picture and the Preview Bus's channel (audit pack 73)**
  — as VEGAS 2026's Help describes them ("Using the Trimmer", "Previewing
  media files"): the Trimmer's shortcut menu has Show Video Monitor (off,
  its picture shows in the Video Preview window while the Trimmer has the
  focus, with Show Video in Preview Window; file previews too), Show
  Video Frames (the media's frames along its time bar) and Enable Hover
  Scrub (the pointer over the picture shows that part of the clip); a drag
  across the picture selects (from left to right the in point first), one
  starting up or down takes the selection to the timeline, a double-click
  selects all again. The Mixing Console's Show Channels › Preview Bus
  shows the Preview channel (the level previews play at, their meters).
- **Docking areas, the Explorer's Delete and address bar, track groups'
  buttons (audit pack 72)** — as VEGAS 2026's Help describes them ("Using
  and Managing Docking Areas", "The Explorer Window", "Grouping tracks"):
  F11 hides and shows every window docked around the timeline, Alt+F11,
  Ctrl+Alt+F11 and Ctrl+Shift+F11 the top, bottom and right docking areas
  (brought back as they were), Ctrl+F11 folds the track headers to their
  color bars and Ctrl+Alt+Shift+F11 maximizes the timeline (both); each
  docked window's tab bar has Maximize and Restore. The Explorer's
  toolbar has Delete (the file or folder picked to the Recycle Bin — the
  desktop's trash on Linux —, asked first; also in its item menu) and Add
  to Favorites, and its address bar is VEGAS 2026's trail of folders, each
  a click away (a click beside them types a path; the first folders
  behind « when they do not fit). The track group header's Mute and Solo
  are drawn as the Help's figures show them.
- **Pan/Crop masks (audit pack 71)** — as VEGAS 2026's Help describes
  them ("Creating masks using the Pan/Crop tool"): the Pan/Crop window's
  keyframe controller has a Mask row, its checkbox turning the event's
  masks on; picked, the window's tools are the masks' — Normal Edit,
  Anchor Creation (a click adds an anchor, a drag pulls its tangents out;
  on the picked path's outline it puts one in; a click on the first anchor
  closes the path), Anchor Deletion, Split Tangent, Rectangle and Oval
  (Shift a square or a circle, Ctrl from the middle, Alt rounded corners
  or half an oval) — its properties the masks' Apply to FX and the picked
  path's Position (X, Y) and Path (Mode Positive, Negative or Disabled,
  Anti alias, Opacity, Feather type None, In, Out or Both, Feather %), and
  its workspace shows the paths dashed with their anchors, what the masks
  cut darkened, the picked one's box (its corners scale it, Ctrl and a
  drag copies it), its anchor's tangents and a small toolbar (Positive,
  Negative; Feather Inside, Both, Outside); its menu has Reset Mask,
  Deselect, Delete, Duplicate, Initialize, Hide and Split Tangents and
  Flip Horizontal and Vertical. The masks are shapes on the media
  (`bv_timeline::mask`), animated on the Mask row's keyframes (anchor by
  anchor) and kept through cuts, trims and joins; they cut what shows of
  the event, or with Apply to FX keep its effects inside them, the picture
  as it is elsewhere — on the processor and the graphics card alike.
  Still to come: the feather's outlines on the workspace, rotating a
  path by its box, the masks' motion tracking.
- **Automation controls everywhere (audit pack 70)** — as VEGAS 2026's
  Help describes them ("Audio track controls", "Audio bus tracks"): with
  Automation Settings selected the Mute button is an automation control
  (marked with a dot) — with automation off it mutes the whole track, in
  Read it shows the mute envelope but cannot be set, in Touch and Latch it
  edits the envelope at the cursor, or while playing records it, held
  until playback stops —, and so is the multipurpose slider: the send's
  envelope (with automation off a level it holds over the send's own). A
  bus track's header has the Automation Settings button and its menu
  Activate Automation Controls and Automation Settings (Off, Read, Touch,
  Latch): its volume, pan and Mute then work as a track's, recorded the
  same way; with its automation off its envelopes (and its Surround Pan
  Keyframes) do not play and the values its controls hold do. The Mixing
  Console's bus and chain strips have the same Automation Settings button
  and modes. Ctrl+Shift+Up/Down size every bus track while the bus area
  has the focus (it was clicked last).
- **Bézier keyframes and the OFX envelopes' menus (audit pack 69)** — as
  VEGAS 2026's Help describes them ("Bézier Automation in OFX Parameters",
  "Timeline-based OFX Keyframe Animation"): a plug-in parameter's keyframe
  takes a Bézier curve, its handles locked (they move together, equal and
  opposite) or split, from the keyframe controller's menu and from a
  timeline envelope point's; the value goes from key to key on a cubic
  curve over time and value through the handles (a curve beside one
  meeting it with its own slope; handles shortened to the next keyframe so
  the curve never turns back in time), in the plug-ins' values, the
  controller and the timeline alike; made Bézier, a keyframe's handles
  keep the curve as it was. The handles show and are dragged in the
  controller's Curves view and on the timeline's envelopes. The timeline
  envelopes' menus as VEGAS's picture shows them: Add Point, Set to… (a
  point's value typed), the eight curves, Flip, Thin (the points the shape
  keeps without, within a two-hundredth of each number's range, left out),
  Reset and Delete; the keyframes area's menu adds a point at the cursor
  to every envelope it shows and shows or hides each; its lines' keyframes
  (each effect's, Track Motion's) are dragged in time, their values kept.
  Bézier keyframes are BananaV's amber in Lanes (VEGAS's color unknown).
- **Timeline OFX keyframe envelopes (audit pack 68)** — as VEGAS 2026's Help
  describes them ("Animate OFX keyframes on the timeline", a beta there
  too): View › Show OFX Effect Envelopes (Beta) shows plug-in effects'
  animated parameters as envelopes — a video track's in its keyframes area
  (its header's Expand Track Keyframes: a line for each plug-in effect and
  one for Track Motion with their keyframes, the effect picked there
  (Expand Keyframes) showing its envelopes; View › Video Envelopes › Track
  Keyframes hides the areas), an event's over the event. Each number of a
  parameter (a position's X and Y apart) is an envelope over its display
  range, its points the parameter's keyframes: dragged in time and value
  (on frames, never past their neighbours), added by a double-click on the
  line, and from their menus given a curve (Linear, Fast, Slow, Smooth,
  Sharp, Hold) or deleted; a line's menu adds a point at the cursor, flips
  it or resets it (no keyframes: its value at the cursor). Bézier
  keyframes, Thin, typed values and the keyframes area's menu came with
  pack 69; Track Motion's own envelopes are still to come.
- **Expand Track Layers (audit pack 67)** — as VEGAS 2026's Help describes
  it ("Video track controls"): a video track header's menu has Expand
  Track Layers (again: collapsed); the track, twice as tall, shows three
  rolls — the A roll at the top, the B roll at the bottom, sequential
  events alternating between them, and the transition roll between where
  events overlap, each overlap a block named after its transition. Events
  are picked, moved and trimmed on their rolls as usual. Still to come:
  the track keyframes row (Expand/Collapse Track Keyframes).
- **Audio FX automation (audit pack 66)** — as VEGAS 2026's Help describes
  it ("Automating audio effect parameters"): Insert › Audio Envelopes › FX
  Automation… (and a track header's FX Automation Envelopes) open the FX
  Automation Chooser — the chain's effects as buttons, each one's Bypass
  and parameters ticked to have envelopes —; OK adds an envelope for each
  parameter ticked (holding its value now, with Preferences' "New audio FX
  automation envelopes" curve) and takes away those cleared. The
  envelopes are drawn and edited on the track as the others (named "3-Band
  EQ: Low (dB)", their values in the parameter's units, VEGAS's FX
  Parameter colors), the mixer follows them each block without clicks,
  and the FX window shows and sets their values at the cursor and has
  Bypass FX Automation; an effect taken out of its chain takes its
  envelopes. Busses' and assignable chains' effects follow their bus
  tracks' envelopes too. Still to come: VST 3 parameters, recording FX
  automation from the FX window while playing, 5.1's per-channel Enable,
  and the Plug-In Chooser's Automatable folder.
- **Generic Control surface (audit pack 63)** — as VEGAS 2026's Help
  describes it ("Using a Generic Control Surface", "Configuring a Generic
  Control Surface"): Options › Preferences › External Control &
  Automation's Available devices offer Generic Control (up to five, beside
  the Mackie Control); double-clicking one opens Configure Generic Control
  — its MIDI input, Number of channels, View function group (Transport,
  Channels, Assign, Audio Channels, Video Channels), the host commands with
  their channel, MIDI message and data, Learn (the next message the
  controller sends binds the command picked), Edit (the message typed),
  Reset, Reset All, Save As and Open (`.bvgeneric` files). With Options ›
  External Control on, the controller plays, stops, pauses, records, goes
  to the start or end and loops; Channel Bank Up and Down move the strips
  by the number of channels; Set Audio Mode and Set Video Mode pick which
  tracks the strips control (a control may do one thing in each mode);
  each strip's Fader, Pan, Send, Mute and Solo; the Master fader; Toggle
  Automation Mode moves the tracks' automation controls (recorded while
  playing in Touch or Latch, let go after a moment still) instead of their
  trims. Effect parameters are not controlled, as VEGAS's are not. Still to
  come: lights and motorized faders sent back, the TranzPort and
  FaderPort.
- **Multichannel mapping and real-time render (audit pack 62)** — as VEGAS
  2026's Help describes them ("Rendering Multichannel Audio Files",
  "Real-time rendering"): the Render window's options have Enable
  multichannel mapping (Wave and Wave64 files) and its Channels… button,
  the Channel Mapping dialog — the Master and every bus with a check box,
  "(Mono downmix)" and Move Up and Move Down, kept in the project (one
  undo) — and the file gets each mapped strip's sound in channels of its
  own (a bus's output after its effects, pan and volume; the Master's
  mix), as many as the mapping fills or the template's when it has more
  (the rest silent); Render to New Track uses it too. File › Real-Time
  Render… asks for a Wave file, unarms armed tracks and plays the project
  from the start of the timeline, writing what plays (without the
  metronome's clicks) at the project's sample rate, bit depth and
  channels until the project's end, the status bar showing how far;
  stopping playback stops it (the file keeps what played) and nothing
  records meanwhile. Still to come: the external inputs real-time
  rendering is for (input busses), MXF and AVI multichannel files.
- **360° projects (audit pack 61)** — as VEGAS 2026's Help describes
  them ("Preparing a project for 360° movies", "The Video Preview window
  in 360° mode", "Exporting 360° videos", File I/O's auto-detection):
  Project Properties' 360 Output makes the project a 360° one
  (equirectangular frames, the whole sphere around the camera); the Video
  Preview's 360° View button (360° projects only, never on by itself)
  shows the frame through a camera looking around in it — dragging turns
  it, the mouse wheel zooms (its field of view, 20° to 150°), a
  double-click looks ahead again —; MP4 and MOV renders say the video is
  360° (Google's Spherical Video V2 boxes and V1 XML) so players and
  sites show it as such; importing a 360° media (such metadata, or a
  photo's panorama XMP) turns the project's 360 Output on while File I/O's
  "Enable auto-detection of 360° media" is on (the default). Still to
  come: stereoscopic 360°, cube maps, 360° spatial audio, the 360° effects
  (Scene Rotation, 360° Stabilization, Dual Fish Eye Stitching).
- **Render to New Track (audit pack 60)** — as VEGAS 2026's Help
  describes it: Tools › Render to New Track (Ctrl+M) opens the Render
  window titled so; the soloed tracks (or the whole project, as the Master
  bus plays it) are rendered over the time selection (or the whole
  project) in the format picked, and once the render ends the file joins
  the project on new tracks — a video and an audio one, as it has them —
  where the render started, its events picked; a canceled or failed
  render adds nothing.
- **Transport bars' Edit Visible Button Set (audit pack 59)** — as VEGAS
  2026's Help describes it ("Using the Video Preview window"): the Video
  Preview's and the Trimmer's transport bars show the buttons ticked in
  their More › Edit Visible Button Set (Loop Playback — the preview's
  only —, Play From Start, Play, Pause, Stop, Go to Start, Previous
  Frame, Next Frame, Go to End; Show All, Hide All, OK), in that order,
  VEGAS's at first; the others are in the More menu, which runs them.
- **Video levels pixel formats (audit pack 58)** — as VEGAS 2026's Help
  describes them ("Setting project properties", "8-bit Pixel Format in
  Projects"): Legacy 8-bit (video levels) and 32-bit floating point
  (video levels) can be picked in Project Properties. They composite in
  video gamma (2.222) and keep Y′CbCr media as studio RGB: decoded
  without stretching 16–235 to 0–255, whatever range the file says (on
  the processor's readers and on the graphics card), and written back
  the same way by the encoders (MP4/MOV, WMV, ProRes), Y′ 16–235 staying
  16–235; RGB files, stills and generated media stay full range, as
  VEGAS's. Still to come: the media Properties' Color Range override in
  the full range format.
- **Field order and deinterlacing (audit pack 57)** — as VEGAS 2026's
  Help describes them ("Setting project properties", the media's
  Properties): a video file's field order is read from it (Media
  Foundation's interlace mode on Windows, FFmpeg's field order on Linux)
  and changed in its Properties; Project Properties' Field order and
  Deinterlace method (None, Blend fields — VEGAS's default —, Interpolate
  fields; Smart adaptive greyed out) work: where a media's field order is
  not the project's, its pictures become whole frames at Good and Best
  quality (VEGAS deinterlaces nothing at Draft and Preview), Blend fields
  as Reduce Interlace Flicker's filter, Interpolate fields keeping the
  field shown first and filling the other's lines from their neighbours,
  on the processor and on the graphics card. Still to come: rendering
  fields (interlaced output), Smart adaptive.
- **Resample mode: Frame Blend (audit pack 56)** — as VEGAS 2026's Help
  describes it ("Setting project properties", "Applying switches to
  events"): File › Properties › Video's Resample mode (Frame Blend,
  Disable resample — VEGAS's default for new projects —; Optical Flow
  greyed out until BananaV has it) and the events' switch (Use Project
  Resample Mode, Frame Blend, Disable Resample; Edit › Switches and the
  event's menu, keyboard ids Edit.Event.SmartResample, ForceResample and
  DisableResample). With Frame Blend, where a file shows at another rate
  than its own — its frame rate not the project's, or the event slowed
  or sped up —, each frame mixes the media's frame of that instant with
  the next by how near it is to each, on the processor and on the
  graphics card (the next frames read by a second reader of the event).
  Still to come: optical flow; blending after the media's effects, as
  VEGAS's signal flow has it (BananaV blends the pictures as read).
- **Project Media's event commands and high-frame-rate clips (audit pack
  55)** — as VEGAS 2026's Help describes them ("Using the Project Media
  window", "Working with high-frame-rate (HFR) clips"): a media item's
  menu has Select Timeline Events (the events playing it picked — added
  to the selection with Ctrl or Shift —, the cursor at the first, the
  timeline brought to it), Add at Project Frame Rate (its event at the
  cursor played so that each frame is one of the project's, long enough
  to show all of it) and Match Project Video Settings; the event's
  Properties have Conform to Project Frame Rate (its length kept). The
  playback rate stays within 0.25 and 4; past that the video's velocity
  envelope holds the rest (240 fps in a 24 fps project: 0.25 and 40 %),
  the sound out of step with it, as the Help warns.
- **Closed captions (audit pack 54)** — as VEGAS 2026's Help describes
  them ("Adding closed captioning to video files"): `608CC1` to `608CC4`
  command markers, their Comment holding the caption markup (`{RCL}`,
  `{EOC}`, `{EDM}`, `{RU2}`…`{RU4}`, `{RDC}`, row placements, tab
  offsets, mid-row colors, backgrounds…) and their Parameter the text it
  shows (`[BR]` between rows); text typed with no markup becomes a pop-on
  caption at the bottom rows (31 characters a row, longer lines
  wrapping); what each channel shows is worked out as a CEA-608 decoder
  does. File › Import › Closed Captioning reads Scenarist (`.scc`: the
  608 bytes turned into markup, pop-on captions placed where they show),
  SubRip, SAMI (a language a channel: CC1, then CC3), RealText, QuickTime
  text and transcripts (`.txt`) and DVD subtitles (`.sub`, MicroDVD and
  SubViewer too), placed by their times less the ruler's offset; Tools ›
  Scripting writes each channel as SubRip, SAMI with an `.asx` playlist,
  QuickTime text with a `.smil` presentation, or DVD subtitles; the Video
  Preview's Overlays draw CC1 to CC4 on CEA-608's 15 × 32 grid, white on
  black; Edit Details' Ctrl+K places the picked caption at the playback
  position and picks the next. Still to come: MacCaption (`.mcc`) files,
  an `.scc` beside renders and captions inside MPEG-2/MXF files, reading
  captions embedded in media (and Promote Media Closed Captioning), the
  Trimmer's overlay.
- **Track effects before or after compositing (audit pack 53)** — as
  VEGAS 2026's Help describes it ("Applying video track effects before or
  after track compositing" and the video signal flow diagram): the Video
  Track FX window's chain has a Composite button, last at first (VEGAS's
  default, all effects before compositing), dragged or moved by its menu
  as an event's Pan/Crop is. The effects after it work on the track's
  compositing group's picture once the track composited into it — a
  child's on the group so far, a parent's on the whole group before its
  Parent Motion —, on the processor and on the graphics card
  (`Track::composite_at`, `TrackScene::post_from`); outside compositing
  groups, where the diagram shows no such step, all of them work before
  compositing. Adjustment tracks have no Composite button.
- **Event effects before or after the Pan/Crop (audit pack 52)** — as
  VEGAS 2026's Help describes it ("Applying video event effects before or
  after event panning or cropping"): the Video Event FX window's Pan/Crop
  button stands anywhere in the chain, dragged there (a line shows where
  it goes) or moved by its menu's Move Left and Move Right, as the
  effects' buttons are; the effects before it work on the media's own
  picture, the others on the framed one (`Clip::pan_crop_at`, read by
  `MediaLayer::framed_from`, which the processor and the graphics card
  already split the chain at; Paste Event Attributes carries it with the
  effects). Still to come: the track chain's Composite button (track
  effects before or after compositing).
- **Final Cut Pro 7 XML and FCPXML import (audit pack 51)** — as VEGAS
  2026's Help describes it ("Importing and exporting projects"): File ›
  Import's "Final Cut Pro 7/DaVinci Resolve (*.xml)" and "Final Cut Pro X
  (*.fcpxml)" open a file's first sequence as a new, unsaved project
  (unsaved changes asked about first), its media found as OpenTimelineIO's
  are (from the file's folder when moved) and a report of what did not
  come in (files not found stay offline; generators, titles, effects,
  nested and multicamera clips counted). `bv_timeline::fcp_import` reads
  both into the timeline document OpenTimelineIO's reading makes
  (`otio::build` turns it into the project) on BananaV's own small XML
  reader (`xml.rs`): xmeml's clip items (a file defined once, named
  after; a start or end of −1 next to a transition worked out from the
  other and the clip's length, the overlap a crossfade; a stereo file's
  second channel on another track left out), FCPXML's resources (formats'
  frame durations as rates, assets' `src` or `media-rep`) and its spine
  (items one after another, connected clips in their parent's time on
  lanes — video lanes from the lowest up, audio the primary's, those
  above, then those below —, a transition a crossfade of the two clips
  reaching half of it into each other). Still to come: speed changes,
  titles and generators, roles, multicamera and compound clips.
- **Project references in rendered files (audit pack 50)** — as VEGAS
  2026's Help describes them ("Project references in rendered files"):
  Render's option "Save project as path reference in rendered file"
  (enabled once the project is saved, for the files BananaV writes
  itself: MP4, MOV, M4A, WAV) keeps the project's path in the file — a
  `bvpr` box in an MPEG-4 family file's user data (`bv_mp4::Metadata::
  project`, read back by `bv_mp4::read::project_reference`), a `bvpr`
  chunk in a WAV file (`WavWriter::set_project`,
  `bv_wav::read_project_reference`); other programs pass over both.
  Edit Source Project (an event's menu, Tools › Audio, Project Media's
  and the Explorer's file menus, offered when the file keeps one:
  `Engine::project_reference`, `archive::keeps_reference`) opens that
  project in another BananaV window. Still to come: the event following
  the file when it is rendered again, other programs' references.
- **Project archives, copying the media, safe render swaps (audit pack
  49)** — as VEGAS 2026's Help describes them ("Archiving projects",
  "Saving and renaming a project (Save As)", "Rendering your project"):
  File › Export › Project Archive opens VEGAS's Archive dialog, two pages
  (the archive's project file, Include media, Include video proxy/audio
  peak files, Exclude unused media; then what is copied and Finish;
  nested projects and RAW companion files greyed out until BananaV has
  them), and copies the media files — every picture of an image
  sequence, the proxies when asked — into the archive's folder on a
  thread of its own (the status bar's progress and Cancel; what a
  canceled copy made goes), the archive's project pointing at the copies
  (`archive.rs`: `plan`, `moved`, `CopyJob`). The system's save dialog
  takes no check box of BananaV's, so Save As's "Copy media with
  project" is File › Copy Media with Project: on, Save As (and a first
  Save) copies the media from outside the project's folder into it, then
  the project points at the copies and is saved again
  (`BananaApp::copy_media_with_project`). Render's options have Swap
  video files on render: a file already there is replaced only once the
  render has finished well (`swap_target`, `swapped`: rendered beside it,
  then renamed into its place; the loudness log follows). Still to come:
  Create trimmed copies of source media (extra head and tail).
- **Storyboards (audit pack 48)** — as VEGAS 2026's Help describes them
  ("Using Storyboards"): Project Media's Storyboard Bins hold the Main
  Timeline storyboard and the project's own (Create New Bin, Rename,
  Delete; `bv_timeline::storyboards`, `Project::storyboards`), shown as
  cells (`tabs/storyboard.rs`): each one's start in the sequence over its
  picture, its name under it, empty cells after the last. Media let go on
  an empty cell go there, placeholders (notes of what is to come, renamed
  with a double-click) filling the cells between; on a cell they take its
  place, on its left edge they go before it; files of the Explorer are
  imported first. Alt and a drag moves a cell, the others closing up and
  making room. Cells, or a whole storyboard bin, drag onto the timeline
  end to end (`BananaApp::add_story_cells`; a placeholder becomes a text
  media saying its name). Start Preview plays the picked cells, or all,
  one after another (`Engine::start_sequence_preview`). The Main Timeline
  storyboard is the timeline's sequence — the events of its first track
  that has any (`Project::timeline_cells`) —: moving a cell moves its
  event as the Help says (`Project::move_in_sequence`: the events after
  where it was close up, those from where it goes make room), media let
  go on it go to the sequence; after an edit on the timeline it turns red
  (VEGAS's color) until Refresh Storyboard, unless the button's menu has
  Automatically Synchronize Main Timeline Storyboard on
  (`UiSettings::storyboard_auto_sync`), and it takes no changes while it
  is red. Still to come: hover-scrub selections in cells, dragging media
  over the Storyboard Bins heading to open a storyboard, a placeholder
  that is a real text file.
- **Media tags, Quick Tags, Search Media Bins and smart bins (audit pack
  47)** — as VEGAS 2026's Help describes them ("Using the Project Media
  window", "Sorting your media with bins"): Project Media's Views › Media
  Tags shows the Media Tags panel under the media (`tabs/media_tags.rs`):
  the picked media's tags (filled when all of them have one, outlined
  when only some do; × takes it off), the Add Tags box (Enter tags the
  picked media, Backspace in the empty box takes the last tag off,
  Ctrl+Shift and a number keeps the text as that Quick Tag) and the Quick
  Tags, Ctrl+1 to Ctrl+9, three columns of three (a click or Ctrl and the
  number tags the picked media while Project Media has the keys —
  `BananaApp::keys_tab` —; Edit Quick Tags… in their menu); the tags are
  the project's (`bv_timeline::tags`, `Project::media_tags`), each a bin
  under Tagged Media. Search Media Bins (the toolbar's magnifier, All
  Media's and a custom bin's menu) is a dialog of conditions — name,
  path, type, tag, length, width, height, frame rate, sample rate,
  channels or tape name, compared as they suit, all of them or any —
  over the bin and the bins in it; Search Results joins the tree, kept
  as a bin (what it found then) or a smart bin (`Project::smart_bins`:
  the search, run again whenever it is shown; Edit Search…, Rename,
  Delete). The quick text filter the magnifier opened before is gone.
  Still to come: tags in the Details view, dropping media on a tag's
  bin, VEGAS's own search fields if its pictures show others.
- **Filter packages, the Third Party Filter and Save LUT (audit pack
  46)** — as VEGAS 2026's Help describes them ("Managing Video FX
  Packages", "Creating and using effects packages and FX chains", "Using
  LUT's", the plug-in windows): an FX window's Save Plug-In Chain As
  Filter Package and the Plug-In Chooser's Save As… name a chain in the
  Save Plug-In Package dialog and keep it, with all its settings, as a
  file of its own in `%APPDATA%\BananaV\FX Packages` (`fx_packages.rs`,
  `.bvfx`); Video FX's Filter Packages category lists the video ones —
  Refresh and Explore Effect Filters Folder in its cell while chosen,
  each package's preset the sample picture through its chain, dragged
  onto an event, double-clicked, its + or its menu's Apply to Selected
  Events or Apply to Track, Rename… and Delete —; the Plug-In Chooser's
  Filter Packages folder (Add puts a package's chain at the end, Replace
  now makes the picked plug-in or package the whole chain, as the Help
  says) and the Plug-In Manager's list them at every level (events,
  tracks, media, busses, the video output). The Plug-In Chooser's Save
  LUT… writes what a video chain's enabled effects do to colors as a
  33-point `.cube` (`bv_engine::effect_chain_cube`: the lattice through
  the chain, encoded values, through BT.709's transfer in 32-bit linear
  projects). The Third Party categories of Video FX, Transitions and
  Media Generator show a Filter at the top right, one maker's plug-ins
  (the name of the `.ofx.bundle` a plug-in comes in: OpenFX names no
  maker, `ofx::provider`). Still to come: VEGAS's own package files and
  folder, a package's picture as VEGAS draws it (a chain symbol; BananaV
  shows the sample through the chain).
- **Layered PSD files (audit pack 45)** — as VEGAS's Help describes
  them: a layered Photoshop document let go with the right mouse button
  and Add Across Tracks puts each of its picture layers on its own track,
  the top layer on the upper track (`BananaApp::psd_layer_media`); each
  layer is a media of its own (`MediaInfo::layer`, named after the
  document and the layer, made once), drawn alone on the document's
  canvas — its transparency and opacity in its alpha — by `bv-image`'s
  `psd_layer` (Adobe's published layer records: gray or RGB of 8 or 16
  bits, raw or RLE; group folders left out); an event's Stream menu lists
  the composite and the layers. Still to come: ZIP-compressed layers,
  layer blend modes and masks, CMYK layers.
- **Adding several media as VEGAS's Help lists it (audit pack 44)** —
  media let go with the right mouse button (from Project Media, or files
  from the Explorer) bring the menu: Add Across Time (end to end, the
  editing preference's overlap), Add Across Tracks (each on its own
  track from the one let go on down, tracks made when missing), Add as
  Takes (takes of the event under the pointer, or of the first one
  added), Video Only and Audio Only with the same three, Cancel
  (`BananaApp::add_dropped`); Project Media picks several media with Ctrl
  and Shift and drags them together, the one dragged first, then the
  others in the window's order (`MediaDrag::Many`). Still to come: the
  Explorer's multiple selection, one undo for a whole drop.
- **The Explorer's views (audit pack 43)** — Views › Thumbnails, as
  VEGAS 2026's Help shows it: each file's first picture (a sound's or an
  unread file's icon) with its name under it, the files read in the
  background a folder at a time; on the one under the pointer + (Add to
  Project Media), Start Preview and Trim. Views' Regions and Summary
  (VEGAS's older views, kept as panels at the bottom): the selected
  file's regions — dragged to the timeline, an event of that part
  (`MediaDrag::FilePart`) — and a short description (size, length,
  video, audio, modified). Still to come: Delete (the file to the
  Recycle Bin), VEGAS 2026's breadcrumb address bar and Favorites in the
  tree as its picture shows them.
- **Media previews (audit pack 42)** — as VEGAS's "Previewing media
  files" describes them: Project Media's and the Explorer's Start Preview,
  Stop Preview and Auto Preview (a click plays the media), the Explorer
  file menu's Start Preview and the Trimmer's Play, Pause and Stop play a
  media alone through the preview bus (`Engine::start_media_preview`: a
  project of its own with one event of it, so the playback's machinery
  serves it; the timeline's playback stops), its picture in the Trimmer
  (a file not in the project too); View › Mixer Preview Fader shows the
  Preview strip — its fader, the level previews play at, and its meters —
  left of the Master's in the Master Bus window. Still to come: the
  Trimmer's own video monitor and Show Video in Preview Window, hover
  scrub, the Mixing Console's preview bus channel.
- **Subclips (audit pack 41)** — as VEGAS's Help describes them: Create
  Subclip (the Trimmer's button and an event's menu: a name, the file's
  with a number appended, and Reverse) keeps a part of a media as an item
  of Project Media, listed after its media; its events are of the parent
  media itself, that part, backward when reversed (one undo); opened in
  the Trimmer it opens its parent with its part selected; Remove from
  Project removes it alone, and it goes with its media. The event's Open
  Parent Media in Trimmer selects the event's part of its media. Still
  to come: VEGAS's own look for subclips in Project Media.
- **The Trimmer's editing (audit pack 40)** — as VEGAS 2026's Help
  describes it: three-point edits from the cursor (A) or up to it
  (Shift+A, now in the timeline too), Fit to Fill (the selection
  stretched or compressed over the timeline's time selection, one undo),
  Enable Timeline Overwrite (what is added replaces the time it takes on
  its tracks, `Project::overwrite_under`), the history's Sort (Ctrl: the
  other way; H, Shift+H), Clear, Remove Current Media (Ctrl+H) and the
  media no event uses taken out (Ctrl+Shift+H), T and Shift+T (the
  selection as long as the timeline's, from or up to the cursor), Tab and
  Shift+Tab cycling Video and audio, Video only, Audio only, a waveform
  under the picture (Video or Audio Height Larger, Equal Height), the
  wheel zooming the time bar and Zoom to Selection, a double-click
  selecting the whole clip again, In, Out and Length typed after a
  double-click, Copy Snapshot to Clipboard, Save Snapshot to File (the
  media's frame at its own size, joining Project Media) and Open Copy in
  Audio Editor in its shortcut menu. Still to come: playback in the
  Trimmer and media previews, subclips, the Trimmer's own video monitor,
  hover scrub, More with Edit Visible Button Set, VEGAS's own buttons.
- **The Marker Tool (audit pack 39)** — as VEGAS's Help describes it: the
  flag button at the timeline's top right turns it on; it picks tags on
  the marker bars — the marker and region bar, the CD layout bar or the
  command bar, whichever was clicked last —: a click picks one, Shift+click
  the tags between two, Ctrl+click one more or one less, ← and → the
  previous or next (Shift extending the pick); dragging one of them moves
  them all (a region or CD track whole, a CD track with its indices),
  Delete removes them; a press elsewhere turns it off. An index left
  outside every CD track (its track made shorter) now stays, dimmed and
  out of the track list, instead of being deleted; Alt-dragging a CD track
  carries its indices.
- **Edit Details, command markers and the CD layout (audit pack 38)** —
  as VEGAS 2026's Help and its picture show Edit Details: Show (Audio CD
  Track List, Commands, Events, Markers, Media — BananaV's own, kept —,
  Regions, Selected Events), the Preset box (a view kept under a name with
  Save, Delete, "All fields" giving every column back), the table's
  columns as VEGAS lists them (events: Track, Start, End, Length, Number
  of Takes, Active Take Name, Take Start, Timecode In and Out, File Path,
  Tape Name, Select, Mute, Lock, Normalize, Snap Offset), dragged into
  another order, sorted by a click on their heading, hidden by dragging
  them off the window and resized; cells typed in with a double-click or
  F2 (times in the ruler's format) and the timeline following, check
  boxes switched by a click, cells picked as in a spreadsheet, rows by
  their number and all by the corner box; Copy (tab-separated text,
  whole rows with their headings: the PQ sheet), Paste (one value into
  every picked cell, or a block), Cut, Delete, and for events Choose
  Active Take, Open in Trimmer, Select in Project Media, Selectively Paste
  Event Attributes. Command markers (Insert › Command…, C: Command
  Properties with templates, the command bar's blue flags, Edit and
  Delete) and the audio CD layout (Insert › Audio CD Track Region, N, and
  Index, Shift+N; the CD layout bar's tags; Tools › Lay Out Audio CD from
  Events; Project Media's Add as CD Track with two seconds between tracks;
  Ctrl+, and Ctrl+. to the tracks, , and . to tracks and indices; the
  time display's "tt+mm:ss:ff" in Audio CD Time) follow ripple edits as
  markers do. Media Properties' Tape name and Use custom timecode work
  (Edit Details' Timecode In and Out count from it, or from the file's).
  Still to come: writing command markers into rendered files, burning
  the disc (needs a CD writer), VEGAS's own Edit Details shortcut menu
  once a print shows it.
- **Project Notes as VEGAS 2026's (audit pack 37)** — as its Help and its
  picture show the window: notes left in the project, each at a timeline
  time, in a color and with a label, its text and when it was written,
  resolved or not; Hide resolved notes; each note's bar — its label
  ("Untitled") and date, Update Timecode to Timeline Cursor, Jump to
  Timecode, its timecode (clicked, typed in the ruler's format),
  Resolve/Unresolve, Delete, More (its color, its label); at the bottom
  the Note Default Color, the default label and Add New Note (a note at
  the cursor); the old single text becomes a project's first note when it
  opens. Still to come: VEGAS's own icons and colors once a print shows
  them, notes on the timeline's ruler.
- **Visible buttons and the track list's width (audit pack 36)** — as
  VEGAS's track list Help describes them: More › Edit Visible Button Set
  (check boxes for the buttons a video or an audio track header shows
  besides Mute and Solo — Track Motion, Track FX, Automation Settings,
  Compositing Mode, Minimize, Maximize, Color Grading; Arm For Record,
  Invert Track Phase, Record Input, the Auto Ducking ones —, Show All,
  Hide All), drawn in the header's top row (lit while their setting is
  on; Bypass Motion Blur greyed); the border between the track list and
  the timeline dragged to its width (the meter, the sliders and the
  multipurpose slider giving way, down to only the color strips and their
  numbers) and double-clicked to the full width or only the color bars.
  The video track's dialog, buttons and More menu now follow the user's
  prints (see Done). Still to come: Default Layout resetting the buttons,
  icons for the audio buttons still drawn as letters (Record Input, the
  Auto Ducking ones).
- **The track headers' controls (audit pack 35)** — as VEGAS's "Video
  track controls" and "Audio track controls" describe them: Ctrl+click on
  Mute mutes that track alone (on a muted one, none), on Solo solos it
  alone (on a soloed one, none); the selected tracks' Level, Volume, Pan
  and typed values move together (the volume by the same dB) and Invert
  Track Phase takes them all; a video track's mute mutes its compositing
  children, and its solo bypasses only its siblings that are not soloed
  (the Help's sample: other levels play as they are); with Automation
  Settings a video track shows its Fade slider (fade to color, an
  automation control: its envelope, or the value it holds with
  automation off); the output meter's own menu (Reset Clip, its scale,
  Show Output Meters). Still to come: the multipurpose send slider for the
  selection, vertical meters, the Restore Track Height button.
- **Default track properties (audit pack 34)** — a track header's Set
  Default Track Properties, as VEGAS's Help describes it: check boxes for
  the track's Volume and Pan Type (audio), Composite Mode (video), Height,
  Track FX and Automation mode (Input monitor greyed: no input monitoring
  yet) that new tracks of its kind start with, and Restore original
  defaults; kept with the settings; every track an edit makes gets them,
  in that edit's undo (undo, redo and opening a project give none:
  `History::edits` and `moves`).
- **Track groups (audit pack 33)** — as VEGAS's Help and its pictures
  show them: two or more adjacent tracks (Shift+click or Ctrl+click their
  headers) grouped from a track header's menu (Track Group › Group
  Selected Tracks; a video track's compositing parent and children come
  along); the group's header row over its tracks — Collapse/Expand, its
  name (a double-click renames it), Mute Track Group (overriding its
  soloed tracks) and Solo Track Group (overriding its muted ones) — and
  the grouping lane down the left of its tracks' headers; a click on
  either selects the group's tracks, their menu's Ungroup Selected Tracks
  clears it, the header dragged moves the group; collapsed, its tracks
  are out of sight and dark gray bars on its row show where their events
  are; a track dragged among a group's tracks joins it, out of them leaves
  it, Ungroup Selected Tracks takes the picked ones out (one from the
  middle moves below the group); groups stay together whatever changes
  the track list, in project files too. Still to come: the group's own
  icons for its mute and solo (letters now), groups in the Mixing Console.
- **The Surround Panner (audit pack 32)** — View › Window › Surround
  Panner (Ctrl+Alt+3), as VEGAS's Help and its pictures show it: in 5.1
  projects every audio track, bus and assignable effects chain that plays
  into the Master is panned in the room (what goes to a bus goes there
  stereo, and the bus pans; never panned: the middle of the room). The
  window: the owner's badge, the move mode (Move Freely, Left/Right Only,
  Front/Back Only), LFE Only; the five speakers (a click mutes one,
  Ctrl+click solos it), each one's shading and level, the grid where the
  left-right and front-back ratios are 6, 0 and −6 dB (with the pan type
  and the Center's trim), the pan point (a diamond for the trim, a cog as
  an automation control) and the keyframes' path as a trail of dots; the
  pan type written under it, the Center fader (hidden while the center is
  muted), Smoothness with two keyframes or more. The point dragged
  (Shift: on a line through the middle; Alt: at its distance; Alt+Shift:
  on the largest circle; Ctrl: fine), double-clicked back to the middle or
  to where the double-click was, the arrows, Page Up/Down and the wheel,
  the numeric keypad's places (Ctrl: the largest circle's corners), the
  joystick; the menu's Pan Type and Surround Pan / LFE Only. Small
  panners replace the pan in the track headers, the bus tracks and the
  Mixing Console (double-click: the window). Surround Pan Keyframes
  (Insert › Audio Envelopes; a bus track's Insert/Remove Envelope; View
  › Show Envelopes › Audio Envelopes hides them): a row under the track
  — keyframes as diamonds colored by curve, a double-click adds one, a
  click brings the cursor, dragging moves (Ctrl: copies), a double-click
  opens the window there, the menu sets the curve or deletes —; with the
  automation controls the point sets the keyframe at the cursor (at the
  playback position while Touch or Latch records), else the trim (added
  to the keyframes); Smoothness bends the path (Catmull-Rom). The mixer
  places each pair by the pan type: Add Channels and the Balance types
  share it between the rows in proportion and keep its sides (every
  speaker at −6 dB in the middle, as the Help's picture); Constant Power
  and Film pan a point source between neighbouring speakers by cosine and
  sine (the pictures' levels to a few tenths of a dB). Still to come: the
  pan laws measured on VEGAS (listening tests). Since then: the
  per-channel faders and the point recorded while playing (audit pack
  79), the Master's pairs on the device's (135; busses on hardware
  outputs are not panned, as the Help says). A multichannel event's
  channels play as they are: the Help says nothing of panning them.
- **Bézier Masking (audit pack 31)** — VEGAS's plug-in as its Help
  describes it, as BananaV's own OFX effect: up to five masks (Rectangle,
  Oval, Diamond, or a Curve of Bézier anchors drawn on the preview in Edit
  Mode with the Normal, Delete, Add and Split tools, its shape animated
  through the curve's keyframes), each placed by Width, Height, Angle and
  Location (the preview's handles: inside moves it, white corners scale
  it, yellow sides stretch it, the ring turns it), with Opacity and a
  Feather out, in or both ways; Invert, Blend and Mask FX (the effects
  before it show only inside the masks: BananaV's host gives the chain's
  input on its "Original" clip, for events' and media's effects); in
  Video FX's Utility with presets, and the Color Grading panel's More ›
  Bézier Mask adds it. Checked in Natron too. Still to come: the motion
  tracker (its settings greyed out), Mask FX on track and output
  effects (the Pan/Crop window's own masks came with pack 71).
- **The video bus track (audit pack 30)** — View › Video Bus Track
  (Ctrl+Shift+B), as VEGAS's Help describes it: the video output's track
  over the audio bus tracks, with Video Output FX, Mute and Bypass FX and
  Envelopes, and its envelopes — Fade to Color over everything the tracks
  make (after the output's effects), Motion Blur Amount (the span each
  frame averages: half before and half after, or only before for the
  asymmetric types), Video Supersampling (the frames worked out between
  the project's: a smoother blur, or a frame's span averaged alone) and
  Mute; File › Properties › Video › Motion blur type (Gaussian, Pyramid,
  Box and their asymmetric versions) weighs the frames, which the engine
  renders and averages in linear light for the preview and renders;
  View › Show Envelopes › Video Envelopes shows or hides the two new
  ones. Still to come: each track's Bypass Motion Blur, the automation
  sliders, the output effects' keyframes on the bus track, its fade
  colors' menu.
- **Audio bus tracks and bus envelopes (audit pack 29)** — View › Audio
  Bus Tracks (B), as VEGAS's Help describes them: a track for each bus and
  each assignable effects chain under a splitter at the timeline's bottom
  (each sized by its bottom edge, Minimize and Maximize), its header with
  Bus FX (the Plug-In Chooser while it has none), Mute, Solo, volume and
  pan, its lane with its volume, pan and mute envelopes edited as a
  track's (points dragged, a double-click adds one, the envelope's menu:
  curves, Delete/Add Point, Thin, Flip, Reset All Points, Remove
  Envelope); a track's send envelopes (VEGAS's bus envelopes: Insert/Remove
  Envelope › a bus or chain, the send made at 0 dB when there was none),
  all followed sample by sample by the mixer. FX parameter envelopes came
  with pack 66, the bus tracks' automation settings and Ctrl+Shift+Up/Down
  on the bus area with pack 70; still to come: input busses (a live input
  path).
- **The Color Grading panel's pickers and Video Output level (audit pack
  28)** — Lift's Specify a Black Point and Gain's Specify a White Point
  (the color under the pointer read out in the panel), White Balance's
  Select Neutral Tone and the HSL Curves' Color Selector (a click: a
  point at that hue, saturation or luminance; a rectangle: its two ends
  and its mean) take a click — the 5 × 5 pixels around it — or a
  rectangle on the Video Preview, read from the effect's input there;
  Show Limits on Scopes draws the RL thresholds on the waveform, the
  parade and the histogram; the Video Output level grades the output's
  chain; the event's More menu, the track's, Project Media's and the Video
  Preview's shortcut menu open the panel at their level; the HSL Curves'
  grey swatch opens the color picker for any hue, and their spectrum,
  dragged sideways, turns the hues round. Still to come: the HSL Curves'
  tangents (Lock Tangents, Smooth Curves, Lock Axis) and arrow keys, the
  Color Curves' Select Curves and zoom, Bézier Mask, Samsung Log,
  VEGAS's own math (the kernel came with audit pack 139).
- **Brightness and Contrast's Automatic Contrast** — VEGAS 23's section
  under Contrast center: Input's Histogram Cut-Off, Output's Minimum and
  Maximum Brightness, Analysis Mode (Current Frame, Current Clip, Set for
  each Frame independently) and Calculate Brightness & Contrast. VEGAS's
  prints after Calculate (the card, a photograph) measured what it sets:
  Contrast center at Minimum Brightness, Brightness Minimum less the
  picture's dark end, Contrast the spread that takes the range to
  Minimum–Maximum; Set for each Frame independently keys the three at
  every frame (nothing greyed). Still to measure, with the test pictures
  in "Automatic Contrast - testes": how VEGAS cuts the histogram (its
  bright end a little higher than the cut's: Contrast −0.127 on the card
  where BananaV gives −0.120, −0.052 on the photograph for −0.048), the
  light's weights, Minimum and Maximum moved, and what Current Clip does
  (on a still, VEGAS's left the values as they were); its progress shows
  in the status bar (VEGAS's in a window with Cancel).
- **Levels' Gamma between and past its presets** — VEGAS's Gamma is a
  video camera's curve (a power of 1 ⁄ γ on a straight toe; below 1 the
  inverse), its offset measured at 2 and 0.7 only: `0.1367 |ln γ|^1.5`
  joins them, other values await the card through Gamma 0.5, 1.5, 3 and
  5; Gamma's lowest value (0.45, where VEGAS's knobs sit) and how Gamma
  meets the input and output ranges wait for prints too.
- **Gaussian Blur, Sharpen, Vignette, Mirror and Glow, past their
  presets** — measured on the card through VEGAS's presets; still to see
  in VEGAS's pictures: Extreme Blur's settings (its ranges, 0.185, are
  what its picture's blur measures), Sharpen at Amount 0 (the formula
  still sharpens there), which way Vignette's Rotation turns (every
  preset has 0), Glow's White Intense Glow (2 % stronger in BananaV:
  VEGAS's blurs keep their channels in 8 bits, unexplained there), how
  Glow and Gaussian Blur treat a picture bright at the frame's edges
  (the card's edges are black), and Mirror's background at an alpha
  between 0 and 1 (at 1 VEGAS's pictures hold its color with no alpha).
  On the graphics card the five go through the processor until they
  have kernels.
- **The FX window's chain scrolls when it does not fit** — its buttons
  stop before the buttons at the row's end (Plug-In Chain, Save, Remove):
  in a window narrower than the chain they scroll between two scroll
  buttons, as VEGAS 23's (its Build 302 notes say the FX chain's scroll
  buttons now show properly), by the mouse wheel and by a button dragged
  past the chain's ends, and the button picked comes into view. A window
  sized to its page takes the whole chain again (the row's width was
  counted 24 points short, so the last button ran under the icons). The
  scroll buttons' look and place are BananaV's own until a print shows
  VEGAS's.

## In progress

- **VEGAS's video effects, one by one (awaiting the user's review)** —
  every effect of VEGAS's list checked against BananaV's and made to
  behave as VEGAS's: its panel (names, labels, groups, ranges, defaults)
  and factory presets, and its math where VEGAS's behavior is known;
  each one made for BananaV alone, as fast as it can be, with its kernel
  on the graphics card (compared with the processor's) and a picture of
  its panel for the user. Done that way: Bézier Masking (VEGAS's panel,
  the masks added up by their Opacity, the feather by the exact distance
  to the outline, Mask FX on the card), Threshold, Black Restore,
  Channel Blend, Mask Generator, Broadcast Colors, LAB Adjust,
  Spherize, Swirl, Pinch/Punch, Wave, Offset and Wrap, Tiny Planet,
  Deform, Convolution Kernel, Quick Blur, Bump Map, Chroma Keyer, Chroma
  Blur, Linear Blur, Radial Pixelate, Radial Blur, Median and Min and
  Max; the measured Gaussian Blur, Sharpen, Vignette, Mirror and Glow;
  the generators Checkerboard and Test Pattern; the compositors
  Displacement Map, Height Map and Bump Map's. Still to put on the card:
  the other effects that still go through the processor (Color Match,
  Flicker Control, Mesh Warp, …).
  Done so far: Invert (as it was), Black Restore, Broadcast Colors,
  Channel Blend, Chroma Blur, Mask Generator, Median, Min and Max, Quick
  Blur, LAB Adjust, Convolution Kernel, Linear Blur, Radial Blur, Offset
  and Wrap, Spherize, Radial Pixelate, Tiny Planet, Chroma Keyer,
  Bump Map (filter and compositor), Displacement Map and Height Map
  (their vertical direction awaiting a print) and Fill Light (its Hue, Bias and
  Tint scales awaiting the calibration card's measure), the generators
  Checkerboard and Test Pattern (its White, Yellow and Pluge "porches"
  awaiting prints); Swirl, Pinch/Punch, Threshold, Wave and Deform with
  VEGAS's panels and presets on BananaV's math. Next: Lens Correction,
  Defocus, Newsprint, the looks, Noise Texture,
  Color Gradient, the transitions, the measured effects, then the
  sixteen VEGAS has that BananaV lacks.

- **Linux and Windows ARM64, still to check on real computers** — the
  graphics cards' decoding and encoding on Linux (VA-API on AMD and Intel,
  NVDEC and NVENC on NVIDIA: the CI has no card), sound and MIDI devices,
  the AppImage on several distributions, Windows ARM64's hardware
  encoders. Wayland's own windows (`BANANAV_WAYLAND=1`) cannot place
  dialogs nor reach the mouse, keypad or screen outside them; camera RAW
  pictures do not open on Linux (FFmpeg develops none); ASIO is Windows'.

- **Timeline and Video Preview, still to do** (from the VEGAS-like menus,
  shortcuts and mouse work) — reverse playback (J) shows pictures only,
  no reversed sound; Copy Here (right-drag) moves only the dragged event's
  copy to another track; a double-click on an event and a drag to extend
  the time selection to the next fade or event edge; checking the
  Ctrl+Alt+Shift split/trim drag against VEGAS (here dragging right trims
  the right part, left the left one); the audio track and event shortcut
  menus (waiting for the user's VEGAS screenshots); Getting Started's
  links (Discord, site, YouTube, social media). Still greyed out: Motion
  Tracking, nested timelines (create, open, edit source project),
  Open Parent Media in Trimmer,
  Create Subclip,
  Sync Link, the resample modes (frame blend, optical flow, disable
  resample), Video Output Color Grading; the shortcuts of CD indexes and
  tracks, takes (T,
  Shift+T, F2), pitch (− = Shift+− …), multicamera, bus tracks, command
  markers (C), Set Marker Time (Ctrl+K), Render to New Track (Ctrl+M), the
  Trimmer's cut from/to cursor (A, Shift+A), FX envelopes (E, Shift+E),
  group mute/solo and the shuttle keys F17–F23. "Adjust Size and Quality
  for Optimal Playback" halves the preview while playback lags (VEGAS's
  rule is not documented).

- **Track header menus, greyed-out items** — Bypass Motion Blur, Track
  Motion, automation (Activate Automation Controls, Automation Settings),
  the Custom compositing mode (compositor plug-ins: Displacement, Height
  and Bump Map) and track FX before or after compositing, Output Bus, input
  monitoring, Auto Ducking Controller, Edit Visible Button Set.
- **Project Properties, greyed-out settings** — Video tab: interlaced
  output (field order), HDR10 and HLG, video-levels pixel formats, ACES's
  HDR, gamut-limited, D60-simulation, DCDM, P3-D60 and Log view transforms
  and the monitor's own, the ACEScc, ACEScct and ACESproxy working
  spaces, motion blur type, deinterlace method, frame blend and optical flow
  resampling. Advanced tab: the master display
  (needs HDR).
- **Renders, further** — letterboxed frames (a file of another shape
  than the project) and ProRes or image sequences still convert on the
  processor; the card's frames could go to hardware encoders without
  coming back to memory.
- **Main window, greyed-out items** — Explorer: Auto Preview and the
  Thumbnail, Regions and Summary views. Project Media: Capture, Extract
  Audio from CD, Get Media from the Web, Swap Media, Properties, the preview
  buttons. The choosers: the views
  button, the AI/ML and 360° groups. Video Preview: Split-Screen View,
  HDR, the closed captioning overlays. Master Bus: solo, and the fader
  lock in stereo projects.
- **Preferences › General, still greyed out** — they need features BananaV
  does not have yet: 24p DV pulldown removal, the two AAF export options,
  and the two nested timeline warnings. Video:
  take names and numbers wait for multicamera editing.

- **Customize Keyboard** — capturing two-key shortcuts in the key box (they
  can be listed, used and removed, not typed in yet).
- **Track Motion, further** — its keyboard context's commands (layouts,
  constraints, F8 and Shift+F8), Edit in Object Space, keyframe presets,
  3D Track Motion (3D Source Alpha); Edit
  Details row commands (copy, cut, delete, edit, paste); Mixing Console:
  channels dragged to reorder, ganged edits, the Sends region's divider,
  input and preview busses.
- **Trimmer** — playback in the Trimmer, the history list (delete, purge,
  sort), subclips, fit to fill and overwrite.
- **Recording, further** — input monitoring, punch-in over selected events,
  takes while looping.
- **Preferences, still greyed out** — Preview Device: the external hardware
  devices (AJA, DeckLink, IEEE 1394); Audio: tempo-stretched loops, the
  legacy track send gain, 32-bit VST plug-ins; Audio Device: ASIO drivers, rear and center
  channels on other devices, Advanced; VST Effects: choosing the effects
  and the generic editor (with VST hosting); Editing: the gap between CD
  tracks (with disc-at-once CD layout); CD Settings (with disc writing);
  External Control & Automation: the FaderPort and TranzPort surfaces,
  Mackie Extenders and custom mappings, effect
  automation and keyframes; Display: the colors of envelope
  types BananaV lacks (bus sends, effect parameters, event velocity, motion
  blur, supersampling, transition progress) and of the CD and command
  markers' snaps.
- **VST plug-ins** — hosting VST 3 effects (now MIT-licensed) on events
  and tracks: processing, parameters and automation, their editors, the
  plug-ins' own I/O in the VST Effects list; then the effects chosen there
  offered as audio plug-ins.

## Next

- **The VEGAS 2026 documentation audit's work order** (the interactive
  report keeps it, with the Help's topics for each item): Titles & Text
  and the Compositors window; compositing modes and parent/child tracks;
  the Loudness Meters' True Peak Range; reversed events decoding backward
  ahead of time (smooth reversed playback) and the reversed subclip in
  Project Media; the transitions' graphics card kernels (audit pack 138:
  BananaV's own; the GL ones through the processor), Gradient Wipe (an
  image's gradient) and Warp
  Flow; the graphics card kernels of the
  table-driven effects; the Video FX still missing (Mesh Warp, Color
  Match, Flicker Control, Glint's mask, Bump Map as a compositor, Gradient
  Map's free control points); subtitles from files and regions; the Take Chooser's preview
  (Play, Stop), Add as Takes from several media at once and takes
  recorded over events; non-real-time event FX; media timecode from
  MXF, AVCHD and DV files (and in Media Properties); audio
  tools (Modern EQ, beat
  and tempo detection, ducking, the audio FX); input busses; motion tracking; stabilization; nested
  timelines;
  closed captions;
  resampling, deinterlacing and field order; video-levels pixel formats;
  then what needs the user (HDR monitors, CD writers, control surfaces).

- **Industry standards** — VST3 and CLAP audio plug-ins, OpenColorIO
  further (ACES's HDR outputs and the rest of VEGAS's view transforms,
  the log working spaces, the ADX film-scan encodings, the cameras' log
  curves on the graphics card, loading other configurations), AAF, HDR output; the
  order, the licenses and the decisions to take before 0.1.0 (ProRes,
  "Dolby Digital", the codecs in the Linux package) are in
  [STANDARDS.md](STANDARDS.md).
- **OpenFX, further** — the graphics card suites (OpenGL, OpenCL, CUDA),
  parameters' own interacts, track and output effects reading other
  times; the multi-plane extension of
  Nuke and Natron (Natron's Premult, Unpremult and Shuffle 3 need it) and
  their stereo views; on Linux, catching a plug-in's crash as Windows
  does (there it still ends BananaV), or trying each new plug-in once in
  a process of its own before it runs inside BananaV; overlays drawn with
  OpenGL on Linux (GLX) and on cards without OpenGL's framebuffer
  objects.
- **Built-in video effects** — VEGAS 23's Video FX, BananaV's own code
  written as OFX plug-ins in `bv-effects` (Crop shows how), in this order
  (the user's progress page tracks each one and lets them move effects up
  or skip them):
  1. Essentials: Crop (done), Picture In Picture (done), Color Curves (done), Color Corrector (done),
     White Balance (done), HSL Adjust (done), Saturation Adjust (done), LUT Filter (done), Border (done),
     Black Bar Fill (done), Unsharp Mask (done), Add Noise (done), Layer Dimensionality (done),
     Cookie Cutter (done), Timecode (done).
  2. Very useful: Color Corrector (Secondary) (done), Color Balance (done), Broadcast
     Colors, Denoise, Video Stabilization, Lens Correction, Radial,
     Linear and Quick Blur, Defocus, Motion Blur, Lens Flare, Fill Light,
     Flicker Control, Chroma Blur, Mask Generator, Color Match.
  3. Creative and technical: Spherize, Mesh Warp, Starburst, Newsprint,
     Bump Map, Offset and Wrap, Radial Pixelate, LAB Adjust, Channel
     Blend, Black Restore, Median, Min and Max, Convolution Kernel, and
     AutoLooks under a name of BananaV's own.
  4. 360°: Scene Rotation, Tiny Planet, 360° Stabilization (Dual Fish Eye
     Stitching to be decided).
  5. AI: Slow Motion, AI Auto Reframe (Scene Detection to be decided,
     maybe as a command).
  The twelve BananaV already has (Brightness and Contrast, Color
  Grading, Levels, Black and White, Sepia, Invert, Gaussian Blur, Sharpen,
  Vignette, Mirror, Chroma Keyer, Glow) get VEGAS's missing controls
  alongside their neighbours; Pixelate is VEGAS's already (done).
- **Toolbar buttons still greyed out** — input busses, nested timelines,
  takes, capture, disc burning, AI tools, undo/redo history drop-downs.
- **Preferences** — the greyed-out settings of File I/O (Wave64, MXF, no-recompress
  rendering, VFR, MKV reader, RAW processor). The color picker for the
  other colors (effects, generators).
- **Render formats still greyed out** — MPEG-2 (DVD, Blu-ray), Ogg Vorbis,
  Dolby Digital AC-3 where Windows has no encoder, AI Auto Reframe.
- **Import formats still missing** — MXF, FLV, raw DV and MPEG-2
  elementary streams; DNxHD/HR, CineForm, QuickTime Animation, HuffYUV,
  FFV1, H.264 4:2:2 and 10-bit (Windows has no decoder for them; FFmpeg
  decodes them on Linux); E-AC-3, DTS, WavPack, TTA on Windows; JPEG 2000, PCX, PPM, QOI, SGI; EXR's DWAA/DWAB/B44
  compression; Blackmagic RAW, RED R3D, ProRes RAW. Animated WebP and APNG
  come in as stills. Xvid AVIs with B-frames show their frames one late.
- **Codecs in pure Rust** from the public specifications: H.264, HEVC,
  VP8/VP9, AV1, DNxHD/HR, MJPEG decoding and encoding; AAC, Opus, MP3,
  Vorbis; containers MKV/WebM, AVI, MXF — so that BananaV reads the same
  files on every system (today Media Foundation decodes most of them on
  Windows).
- **The processor's compositor** (the fallback without a graphics card):
  buffers kept from frame to frame and a pool of threads instead of new ones
  for each step.
- **Hardware acceleration, further** — decoded pictures straight from the
  decoder to the compositor without passing through memory (Direct3D 11 to
  12 sharing; VA-API and CUDA surfaces on Linux); 10-bit VP9 and AV1 kept
  10-bit on Windows; VideoToolbox on macOS; thumbnails and the render's
  encoding on the card.
- **Text and titles, further** — Titles & Text's rich text (formatting
  per selection, as VEGAS's text box), kerning and complex scripts, its
  Animation list; subtitle formats (SRT, WebVTT, closed captions).
- **Audio** — own DSP and plug-ins, bus tracks, automation modes.
- **VEGAS features shown as "Coming soon"** in the menus: multicamera,
  takes, adjustment events/tracks, motion tracking, nested projects, AAF
  export, scripting, disc burning…
- **Languages** — more translations once the interface settles; convert the
  remaining Portuguese comments in older code to English as files are touched.
