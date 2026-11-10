# BananaV's architecture

How BananaV is built: its crates and their layers, and for each part of
the program where its code is and how it works — the project model, the
media, frames on the processor and on the graphics card, the effects and
plug-in hosts, audio, the interface. The behaviour each part follows is
VEGAS Pro's, the spec ([CLAUDE.md](../CLAUDE.md)); "the user's prints"
are the maintainer's screenshots of it. Read the part you are about to
change; add to it what your change makes true.

## Crates, layers and subsystems

Cargo workspace; each crate declares a layer and may only depend on lower
layers (`cargo xtask layers` enforces it):

| Layer | Crates |
|---|---|
| 0 | `core` (ticks, rates), `bitstream`, `deflate`, `cuda` (NVIDIA's CUDA driver, loaded at run time where the computer has it), `apple` (what the plug-in hosts and OS services share on macOS — Core Foundation's values, bundles, work sent to the main thread, the guard around calls into plug-ins — and Mach-O files read on any system), `lut` (`.cube` look-up tables), `track` (motion tracking: a region followed from frame to frame; dense optical flow), `mpeg2` (MPEG-2 and MPEG-1 video encoding), `dvd` (DVD-Video's video objects, information files and disc image), `ofx-sys` (OpenFX's C API), `vst3-sys` (VST 3's interfaces), `vst2-sys` (VST 2's binary interface, declared in BananaV's own words), `bridge` (the 32-bit bridge's frames, process and server) |
| 1 | `zip` (ZIP archives: OpenTimelineIO's bundles), `color`, `png`, `wav` (also Wave64, AIFF, AU, CAF; ADPCM), `image` (BMP, GIF, TIFF, TGA, DPX, EXR, PSD, DDS, JPEG), `flac`, `prores` (encoder and decoder), `ac3` (Dolby Digital decoder), `mp4` (MP4/MOV writer; MPEG-2 program streams, MPEG-1 system streams), `mf` (the system's codecs: Media Foundation on Windows, FFmpeg on Linux and macOS), `wic` (HEIC, AVIF, WebP, JPEG XR, camera RAW), `platform` (OS services; burning discs), `ofx` (the OpenFX host), `ocio` (color management: OpenColorIO's configurations and transforms, ACES 1.3), `effects` (BananaV's own video effects, written as OFX plug-ins), `ofx-testplug` (OFX plug-ins for the tests, never shipped), `vst3` (the VST 3 host), `vst3-testplug` (VST 3 plug-ins for the tests, never shipped), `vst2` (BananaV's VST 2 compatibility), `vst2-testplug` (VST 2 plug-ins for the tests, never shipped) |
| 2 | `timeline` (project model, editing, undo) |
| 3 | `media` (files and generators → frames and samples) |
| 4 | `render` (video compositing on the processor, the reference), `gpu` (the same on the graphics card, wgpu), `mixer` (audio) |
| 5 | `engine` (playback, preview, export, loudness) |
| 6 | `ui-egui` (the whole interface) |
| 7 | `apps/bananav`, `apps/ofx-bundle` (BananaV's effects as `BananaV.ofx` for other OFX hosts), `apps/bridge32` (`bananav-bridge32.exe`: 32-bit plug-ins in a process of their own, built for i686) |

- Time is in ticks: 254,016,000,000 per second.
- Video is composited in 32-bit float, premultiplied alpha. New projects
  are 8-bit (full range), as VEGAS's (`PixelFormat`'s default): composited
  in video gamma, effects get the pictures' values — what VEGAS gives its
  effects, and what BananaV's effects were measured on (the user's prints
  are of 8-bit projects; in 32-bit linear projects effects get light and
  look weaker: the Color Corrector's presets 2–12⁄255 off instead of 1.4).
  32-bit floating point (full range) composites in linear light (2.222
  compositing gamma: on the values); projects saved before the pixel
  format was a setting keep 32-bit linear. The video levels formats
  (Legacy 8-bit and 32-bit floating point (video levels),
  `PixelFormat::is_video_levels`) composite in video gamma and keep
  Y′CbCr media as studio RGB both ways: decoded with full-range math
  (`bv_color::frame_to_linear_levels`, the readers told by
  `VideoReader::set_video_levels` through `Ask::video_levels`, the card's
  `decoded`) and written back the same way (`linear_to_420_levels`,
  `linear_to_frame_levels`, `Output::Yuv420`'s `video_levels`); RGB
  files, stills and generated media stay as they are. In 32-bit projects effects
  keep what passes white or black, as VEGAS's effects do there (the
  calibration card through ACES's view shows the light past white); 8-bit
  projects clip what each effect leaves to 0–1 (`FrameScene::clips`:
  `bv_render::effects::stored`, the card's `clip_range` kernel; also the
  engine's frames of other times), as VEGAS's 8-bit pictures hold — so
  BananaV's effects never clip their own results.
  Pictures' values (PNG, JPEG…: `files.rs`, `picture_to_linear`) and the
  colors picked for generators and Fade to Color are video's (BT.709's
  curve), as VEGAS takes them: a still shows its file's values (a grey of
  128 is video's 128), where reading them as sRGB light showed them darker
  through video's curve (128 as 115).
- Color management (File › Properties › Video's ACES settings, with
  32-bit floating point (full range), as VEGAS's) is `bv-ocio`:
  BananaV's own OpenColorIO — configurations read (`config.rs` on its
  `yaml.rs`), their transforms worked out (`processor.rs` into `ops.rs`'s
  steps; `builtins.rs`; ACES 1.3's RRT, SDR ODTs and Looks in `aces.rs`,
  ported from the Academy's CTL, its license in `crates/ocio/licenses`,
  shipped with the packages) — and its ACES 1.3 configuration
  (`configs/bananav-aces-1.3.ocio`, written by `tools/ocio_config.py` from the
  primaries: Bradford to ACES's white, CAT02 for the camera makers'
  gamuts, as their published matrices). The model's settings are
  `bv_timeline::ColorManagement`; `ProjectSettings::color_pipeline` gives
  a frame's `ColorPipeline` (`FrameScene::color`): media and picked
  colors are taken as ACES2065-1, VEGAS's default media color space under
  ACES — with the 1.000 compositing gamma their light (BT.709's curve),
  with 2.222 their encoded values, as VEGAS's view shows the calibration
  card (with ACES 1.3 every color to an 8-bit step; with 1.2 VEGAS's
  output is its ACES 1.2 configuration's baked lattice, ~1% of the most
  saturated colors up to 44 steps off BananaV's exact math) — and enter
  the working space (ACES2065-1 or ACEScg)
  through its matrix (`bv_render::enter_chain`), compositing on them,
  and the finished frame leaves through the look (only in linear light:
  VEGAS greys it out with 2.222, `ProjectSettings::look_applies`) and the
  view — the display's code values, taken back as video's light so the
  preview and encoders show and write them
  (`bv_render::leave_through_view`). Picking 32-bit floating point (full
  range) in Project Properties sets the 1.000 gamma and ACES's sRGB view
  when none was chosen (`picked_pixel_format`, VEGAS's: a lone picture
  looks the same in 8-bit and in 32-bit linear, only blends and effects
  differ; ACES's view gives the look), and the Video Preview's bar names
  the view ("sRGB (ACES)"). Project Properties lists what VEGAS
  lists (ACES 1.2's 27 view transforms and 1.3's 45, named as VEGAS names
  them, the log working spaces); what BananaV cannot draw yet is greyed
  out. A media's Properties (Project Media's Properties button,
  `media_properties.rs`, VEGAS's Media tab as the user's prints show it —
  Format as VEGAS writes it, the MPEG-4 family's major brand as the file
  type registry names it and the stream's format, "MP4 Base Media v1 -
  AVC" (`media_general::brand_name`, `codec_name`); the profile button
  beside Stream; how a video file's pictures are read,
  `VideoStreamInfo::reading` (`VideoReading`): Alpha channel —
  `AlphaChannel`: Undefined as its reader takes it, None, Straight,
  Premultiplied, Premultiplied (dirty) against its Background color, which
  the color picker sets; stills' readers follow it exactly (`files.rs`'
  `to_linear`), ProRes 4444's too, `VideoReader::set_alpha` —, Color range
  — Undefined as the file says, Limited (`reading.limited`, the frame's
  range made limited), Full (`full_range`, decoded as the video levels
  formats do), in full-range projects — and Rotation (`MediaRotation`: the
  stored width and height are the turned picture's, `turned_to`,
  `coded_size`; `bv_media`'s `reading.rs` turns the pictures, in linear
  light) — readers opened with other settings open again (the pool's
  `Slot::video`), thumbnails are made again (`media_key`) and smart
  rendering leaves such media alone; the lists show what the file says
  where nothing is set (its color range and alpha, `shown_reading`) —, and
  its General tab, `media_general.rs`, as VEGAS's: the file's name,
  folder, type, size, dates and attributes, its streams — formats,
  sizes, color depth, frame and sampling rates, bit rates and their mode,
  each stream's length and sample size, from an MPEG-4 family file's boxes
  (`bv_mp4::facts`: `tracks`, `major_brand`; H.264's depth and range from
  its sequence parameters) — a Wave file's loop chunk, its markers, its
  tags and its Plug-In: the system's library that reads it, or BananaV,
  with the version resource's version and company (`bv_platform::
  version_texts`) —; an event's Properties, its shortcut menu's, are
  `event_properties.rs`: VEGAS's Video Event tab — the active take's name,
  the switches, the playback rate, its length kept, Conform to Project
  Frame Rate, the undersample rate, with VEGAS's spin buttons — or an
  Audio Event tab, then the Media and General tabs, OK one step of the
  undo history; several picked: their field
  order, pixel aspect and color space together, "(differing values)"
  kept; its video profiles, `UiSettings::video_profiles`, give imported
  files of a kind their settings) and a render template's
  Project tab (`render/customize.rs`) offer VEGAS's Color space list
  (`bv_timeline::ColorSpace`: the ACES studio configuration's ACES and
  camera spaces; the ADX film-scan encodings still to come): a media in
  one (`MediaInfo::color_space`) enters from its file's values — BananaV's
  light encoded again with BT.709's curve — through
  `color::media_input` (the calibration card through VEGAS in ACES2065-1,
  ACEScct, S-Log3 S-Gamut3.Cine and Linear S-Gamut3.Cine matches to
  0.05⁄255 with ACES 1.3; `bv_render::Enter::Space`, the card's
  `enter_from_space`; frames whose steps the card lacks, the cameras'
  log curves, go to the processor), and a render whose template has one
  leaves in it instead of through the view (`ColorManagement::output`,
  set on the render's copy of the project with the template's rendering
  quality: `VideoParams::color_space`, `rendering_quality`). The graphics card runs each
  step as its own kernel (`shaders/color_*.wgsl`, `compositor/color.rs`,
  the same math in 32 bits); never a baked lattice, which misses badly
  near AP1's edge, where ACES's output clips. Preferences › General's
  "Use GPU processing for ACES color management" off sends those frames
  to the processor (`Engine::set_aces_on_gpu`).
- The interface only talks to the rest through the project model
  (`bv_timeline::Project` edited via `History`) and `bv_engine::Engine`.
- Importing: `crates/media/src/files.rs` recognizes a file by its content
  (then its extension) and picks a reader: BananaV's own (`bv_image::read`,
  `bv_wav::read`, animated GIF, image sequences), the system's codecs
  through `bv_mf` (most video and compressed audio) and its picture
  readers (`bv_wic` on Windows, `bv_mf::read_still` on Linux: WebP, HEIC,
  AVIF, RAW…). ProRes and AC-3 come out of the file as packets
  (`bv_mf::PacketReader`, `PacketDecoder`) for BananaV's own decoders, on
  every system. The extension lists the dialogs filter on are
  `bv_engine::{VIDEO,AUDIO,IMAGE}_EXTENSIONS`; refusals are
  `MediaError::UnsupportedCodec`/`UnsupportedFile`, worded by
  `crates/ui-egui/src/media_files.rs`.
- Interchange (File › Import/Export, and File › Open of timelines —
  OpenTimelineIO's, Final Cut Pro 7's `.xml`, `.fcpxml`):
  `bv_timeline::interchange` (EDL; FCPXML's export), `bv_timeline::xmeml`
  (Final Cut Pro 7 XML — Final Cut Pro 7's, Resolve's, Premiere's and
  VEGAS's interchange — read into OpenTimelineIO's document and written
  from a project as those programs write it: a file described wherever
  it first appears, VEGAS's in its bins' master clips; next to a
  transition a clip item's start or end −1, the clip reaching over the
  transition, so crossfades are BananaV's overlaps and transitions to or
  from black fades; sound stereo pairs of mono clip items on two tracks,
  their `trackindex` the file's channel counted across its streams — one
  event each, its stream and `ChannelMap` from them (`otio::build`'s
  `channel_place`; tracks playing the same channels of a file at once
  each its next stream, `spread_streams`: VEGAS names only its first
  stream's) —; links as groups; opacity and gain, their keyframes' ramps
  at the ends as fades; Time Remap's constant speeds, titles and colors,
  nested sequences in their place, markers, the sequence's frame and
  sample rate; VEGAS's files — `by_vegas`: clip items named after their
  master clip and VEGAS's event number — list their audio tracks from the
  bottom up; what the format cannot say, effects, Pan/Crop, frozen or
  reversed frames, is counted), FCPXML's import `fcp_import` (into the
  same document, on the small XML reader `xml.rs`: constant speeds —
  `timeMap`, a frozen frame —, titles as Titles & Text saying their text
  (`TITLES_PLUGIN`, the plug-in's parameters otherwise at their
  defaults), compound clips and multicam clips' active angles laid out in
  their place (`Walk::nest`), its roles naming the tracks) and
  `bv_timeline::otio`, OpenTimelineIO's JSON written and read from its
  specification (the schemas OpenTimelineIO 0.15 to 0.18 write).
  `otio::export` writes video tracks from the bottom up, then audio; a
  crossfade is a transition centred on a cut, a fade a transition to or
  from a gap (of no length where events butt), a speed a
  `LinearTimeWarp`, a frozen frame a `FreezeFrame`; overlaps a transition
  cannot say (an event inside another, three at once, overlaps without
  crossfades or with a muted event) go to lanes, tracks over their own;
  everything else (the project, each track, event, media and effect)
  rides in the objects' `metadata["bananav"]`, so a project comes back as
  it went. `otio::read` places a document's events (transitions' handles,
  nested stacks' first track of the kind) and lists its media;
  `otio::build` makes the project once the caller found them, the
  document's timing winning over the metadata (another program may have
  edited it): lanes merge back, fades a crossfade hid come back, other
  programs' video and sound of one file are grouped, missing files stay
  offline (`check_invariants` lets events run past their media's end, and
  with Loop start before it, as editing does). The interface's sides are
  `interchange_ui.rs` and `otio_ui.rs`: bundles — `.otioz`, a zip written
  and read by `bv-zip` (stored media, a deflated document, ZIP64 past 4
  GiB, CRC-checked, names leaving the folder refused by `safe_parts`), and
  `.otiod`, a folder — packed and unpacked on a thread, the status bar
  showing how far with Cancel, never over files already there (what a
  canceled job wrote goes); an `.otioz` opens unpacked into a folder
  beside it. A timeline opens as a new project, not saved yet; a project
  or a timeline dropped on the window opens as with File › Open — a
  project dropped on the timeline is nested instead (media dropped go to
  the project).
- The graphics card is usable as soon as it opens (`accel.rs`): frames draw
  on it while `Gpu::warm_up` compiles the kernels on a few threads of low
  priority, each frame waiting only for the kernels it uses (`Kernels::
  pipeline`: a cell each, the compiler outside the map's lock). The window
  paints while playing when the project's next frame is due
  (`until_next_picture`, 60 times a second at least), not as often as the
  screen allows; the preview's thread wakes it only for a frame already
  late. What the Explorer's tree shows of the disks is read on a thread of
  its own (`TreeRoots`): no frame touches a disk.
- An event's Pan/Crop (`bv_timeline::pancrop`): a box in its media's pixels
  over time (`PanCropKey`: width and height, negative when mirrored, center,
  angle, where it turns, smoothness; `Clip::pan_crop`), the file's terms, kept
  on the model so nothing rounds it. `PanCrop::at_time` interpolates at the
  100 ns the frame's time truncates to: size, angle, pivot and smoothness in a
  straight line, the center's rotation undone before the line and done again
  after, the pivot along an X-spline whose tension is the smoothness.
  `place_in` then `place()` give a `Placement` of the frame: a turned or
  mirrored box a parallelogram of its corners, with soft edges (coverage by
  16 bands) and the media's edges read as the renderers' `held`/`open` do; a
  straight one a rectangle in fixed point with hard edges, and at Best a
  Catmull-Rom whose rectangle's end pixels land on each other
  (`Placement::in_picture`). `bv_render::layer::place_box` and the card's
  `place_hard`/`place_soft` draw it. Projects saved before come in through
  `Project::upgrade_pan_crops`. Measured frame by frame against another
  program's renders through the `.veg` reader (`bv-engine`'s
  `pan_crop_renders`).
- VEG projects (File › Import and File › Export › VEG Project, and File ›
  Open of `.veg` files): `bv_timeline::veg` reads and writes the format
  from what its files hold — Wave64's RIFF of 64 bits (`riff.rs`: chunks
  named by GUIDs, their sizes counting their 24-byte headers, each on a
  multiple of 8 bytes from the file's start) — into a document
  (`read.rs`: the header's settings, the media, the tracks with their
  events, takes, fades and curves, envelopes, the effects, transitions,
  Pan/Crops and masks, the tracks' motions, markers, regions, command
  markers, the CD layout, groups, the summary's texts, the mixer's busses
  and bins; times in hundreds of nanoseconds, `ticks` and `units`) that
  `build.rs` makes a project of once the caller found the media
  (`Imported` counts what stayed out: generated media's events, effects
  without a plug-in, envelopes BananaV has no place for, transitions at
  events' ends), and back (`write.rs`, in the chunks' order and with the
  default structures the format's own files have). An envelope's kind is
  at its structure's byte 8; a transition's progress places count parts of
  a million of its fade. An effect's parameters are records of their
  value's size followed by their keyframes (`read::param_body`: the sizes
  tell a value's and a keyframe's kind apart; keyframes in frames from the
  effect's start, handles as absolute frames and values) or a text; the
  file's own kind of effect, its seven transitions (`DocEffect::native`),
  is a class, a preset's name and a plain record of its settings, which
  `veg_ui::native_settings` reads. The audio effects' chains are their
  plug-ins' classes, names, bypass switches and settings
  (`read::audio_chain`), and `audio.rs` turns the 51 BananaV has into its
  effects: DirectX records field by field (a number's byte, whole, with a
  point or long, and how it becomes the parameter), graphs by their counts
  and points (`graphs`), the studio effects' programs of normalized values
  (`studio`: a bent line by where each range's middle falls), the Modern
  Equalizer's state (`modern_eq`: its text holds its bytes as letters of
  6 bits). `pancrop.rs` reads and writes the Pan/Crop's chunk, a RIFF of
  32 bits inside the block. `veg_ui.rs` is the interface's side: media
  where the project says or beside it, the video effects and transitions
  BananaV's own of the same names or the same plug-ins installed
  (`effect_in`, `effect_out`), and the
  status line saying what did not come in.
- Nested projects (VEGAS's "Nesting projects"): a project file is media
  (`bv_media`'s `nested.rs`: its length, its output's picture when it has
  video events, stereo sound when it has audio events, its markers and
  regions as media markers; never one nested in itself, `nests_itself`).
  The engine draws it on the processor over a transparent background
  (its prerendered parts' files where they still show what it does) and
  mixes its master bus folded down to stereo (`bv_engine`'s `nested.rs`,
  given to `bv_media::set_nested_readers`), live — no audio proxy to
  build. Import Media and a drop on the timeline nest it (never the open
  project, nor one nesting it); Edit Source Project opens it in another
  window; nested projects saved meanwhile are read again once BananaV is
  the active program again — as are any media files changed meanwhile,
  a file rendered again from its source project say
  (`follow_changed_files`, `read_media_again`: length, picture size and
  rate, sound and markers described again, the user's settings kept, the
  waveforms made anew) —, and Tools › Rebuild Offline Nested Audio reads
  them all again (`nested_ui.rs`); their sound never goes to an audio
  editor. Nested timelines (`nested_timelines.rs`): Create Nested Timeline
  (Alt+C) takes the selected events out into a project of their own
  (`Project::nested_timeline`: plain tracks of their kinds and names, the
  first event at zero, groups and sync links kept; `nesting_risks` for
  the preference's warning), saved beside the parent, an event of it in
  their place on their topmost tracks (one undo step); Open Nested
  Timeline (Alt+N) edits a nested project in this window, the parent
  waiting in `parent_timelines` (saved first when changed); Open Parent
  Timeline (Alt+P) saves the nested one — asking first, by the
  preference — and brings the parent back with it read again.
- File › Screen Capture (`screen_capture.rs`; VEGAS's Vegas Capture):
  sources as cards that show live — displays and application windows
  (`bv_platform`'s `screen.rs`: GDI, the pointer drawn in; a window with
  PrintWindow), cameras (`bv_mf::camera`, Media Foundation's capture
  devices), inputs and what the computer plays (cpal, the default
  output's loopback) — each with its own settings (resolution, frame
  rate, codec; codec, channels, sample rate). Start Recording records
  them all at once (`bv_engine`'s `capture.rs`: a thread a source; video
  as NV12 into the render's MP4 writer, frames repeated when pictures
  come late; sound resampled linearly into MP3, AAC or Wave), each into
  a file of its own in the recording folder; Add to Project puts each on
  a new track named after its source at the cursor. Its Settings keep
  the folder and the codecs and quality new sources start with
  (`UiSettings::capture`).
- Frames: `bv_timeline::scene::frame_scene` describes what a frame shows;
  `bv-render` (processor, the reference) and `bv-gpu` (wgpu compute
  kernels in `crates/gpu/src/shaders`, the same math) draw it. Video
  tracks composite as VEGAS's: each through its Compositing Mode
  (`Track::composite_mode`, `CompositeMode::blend` — the W3C's blend
  formulas, Multiply (Mask) as a mask — and the card's `blend` kernel;
  Custom runs a compositor plug-in, `Track::compositor` and
  `parent_compositor` — VEGAS's Displacement Map and Height Map,
  `bv-effects`' `compositors.rs` (VEGAS's panels, presets and 8-bit
  math: Displacement Map moves each pixel by its map's channel there —
  128 none; the pixel twice and its diagonal neighbours averaged where
  their squared differences stay under 101 steps, `displacement_at`; in
  128ths of the scale's reach, a part of the picture's size —, Height
  Map along the slopes of its map's heights — Amplitude of the longer
  side, smoothed by a box of Smoothness 720ths of it four times each way,
  `height_numbers` — times their depth; both sampled bilinearly, nothing
  or the picture tiled outside it, rows counted from the top as VEGAS's
  plug-ins count them — its Swirl's "Clockwise" presets turn clockwise
  only so —: brighter brings pixels from the right and from below), and
  Bump Map,
  `looks.rs`, with a
  second input, `Map`, in OpenFX's General context —: the track's
  picture guides how what is below it is drawn, `level` of the way
  (`FrameScene::compositor_call`, owner `FxOwner::Compositor`,
  `PluginEffects::composite`; on the card BananaV's own draw as kernels
  — `accel::compositor_plan`, `Compositor::own_compositor`:
  `kit_displace`, `kit_heights` in fixed point (765ths of the height
  and `HeightNumbers::bits` more) smoothed by `kit_box_rows` and
  `kit_box_columns` four times each — integer running sums the
  processor's `box_rows` and `box_columns` repeat exactly, as the slopes
  magnify the smallest difference — then
  `kit_height_map` (the slopes' differences taken in fixed point before
  any product: a fused product and difference leaves a last bit on a
  flat surface), `kit_bump_with` with the filter's light, the
  prelude's `bump_light`; the maps' 8-bit steps are `kit::step8`, the
  shaders' `step8` (a half step up, nudged a ten-thousandth of a step so
  floats a last bit apart round alike);
  `own_compositors_draw_alike_on_the_processor_and_on_the_card` —, others'
  pictures go to the processor and back); the Compositors window is
  `tabs/compositors.rs`,
  the compositor's settings window and Custom in the header's menu
  `compositors.rs`);
  compositing children (`Track::composite_depth`) composite over nothing,
  their parent through its mode onto them, and the group through the
  parent's Parent Composite Mode onto what is below
  (`scene::composite_units`; the tracks of a group keep their places
  with `TrackContent::Nothing` when they show nothing); a group's
  track's effects after its chain's Composite button
  (`Track::composite_at`, VEGAS's post-compositing effects;
  `TrackScene::post_from`) work on the group's picture once the track
  composited into it (`post_composite` in both renderers), and outside
  groups all of a track's effects work before compositing. Adjustment
  events (`Generator::Adjustment`) take what is composited below them
  as their picture, through their effects, Pan/Crop and level; an
  adjustment track (`Track::adjustment`) lays its own events over the
  tracks below, or over its compositing children, then runs its track
  effects on all of that, by its level (`bv_render`'s `adjust_into`,
  the card's `adjust`: the events always show, the level is the
  effects' strength) times its Composite envelope, which a new one has
  along its top as VEGAS's (`insert_adjustment_track`). Media let go on
  it, or events moved there, go on it as on any video track (as
  VEGAS's); video effects let go on its empty part, or on any video
  track's header (VEGAS's way to add track effects), become the track's
  own and its Video Track FX window opens
  (`headers::drop_track_effects`); with no track given, media go to the
  first video track that is not one. A track's Track Motion and a parent's Parent Motion
  (`Track::motion`, `parent_motion`: `bv_timeline::motion`, a
  Pan/Crop-like framing of the frame-sized picture keyframed on the
  timeline, with a 2D shadow and glow — the picture's shape moved,
  scaled, blurred by up to `HALO_BLUR` of the frame's width and
  colored, drawn under it) place the track's finished picture (its
  group's) before its compositing mode (`bv_render`'s `moved`; the
  card's `place`, `blur` and `shade`); the window is `track_motion.rs`,
  on the Pan/Crop editor's box geometry (`fx_window`'s `Space`, `hit`,
  `resize`, `paint_f_box`); its Preset row saves the row edited at the
  cursor (`MotionPreset`, `UiSettings::track_motion_presets`, 2D and 3D
  apart), Edit in Object Space holds the box's own axes (`held_move`),
  and the TrackMotion keyboard context (`TrackMotionKey`: the 3D layouts'
  1 to 6, the toolbar's toggles) is dispatched while the window, an OS
  window of its own, has the keys (`BananaApp::track_motion_keys`). 3D compositing (VEGAS's): the 3D Source
  Alpha mode (`CompositeMode::ThreeDSourceAlpha`, first of the menus'
  `CompositeMode::MENU`; `ALL` stays the card's kernel's 2D list) puts
  the track's finished picture — or, as a Parent Composite Mode, its
  group's — on a box in space (`bv_timeline::space`: `Box3d`'s position,
  size, orientation and rotation keyframed in `TrackMotion::space`,
  `space_keys`; `TrackScene::space`, `parent_space`), seen by a camera
  `CAMERA_DISTANCE` frame widths in front of the frame; 3D tracks next to
  one another are drawn together by `bv_render`'s `space.rs` (each
  pixel's hits sorted far to near, so boxes that cross intersect) and
  laid over what is below, a 2D track between them a barrier
  (`draw_units`' `flush_space`); frames with 3D go to the processor
  (`accel.rs`). The Track Motion window shows the box's Position (X, Y,
  Z, Width, Height), Orientation and Rotation when its mode is 3D
  (`properties_3d`, `workspace_3d`: the Layout's panes, `PaneLayout`,
  each showing the box from the `View` its menu picks — the camera in
  front or behind, a perspective, or flat along an axis; in Camera Front
  the trackball of VEGAS's Help, `Trackball`'s `Zone`s: the corners bring
  the box nearer or farther, inside moves it, with Ctrl nearer or
  farther, the circle's top and bottom turn it about x, its sides about
  y, the rest of its rim about z, the guide in bold; in the flat views the
  box moved along their axes; Prevent Movement (Z) holds its depth). The video bus (`bv_timeline::video_bus`,
  `Project::video_bus`, View › Video Bus Track): its Mute (and mute
  envelope) mutes the scene, Bypass FX and Envelopes leaves out the Video
  Output FX and its envelopes, its Fade to Color (the colors from its
  menu's Fade Colors, `VideoBus::fade_colors`) is
  `FrameScene::output_fade` (after the output's effects; the
  `FadeToColor` kernel on the finished frame), and its Motion Blur Amount
  and Video Supersampling (`VideoBus::control_at`: the envelope where it
  plays, else its header slider's own value; with Automation Settings
  the sliders and Mute are automation controls, `AutoOwner::VideoBus`,
  `TrackControl::MotionBlur` and `Supersampling`, recorded as a track's;
  under it a keyframes row for each animated Video Output FX plug-in,
  `timeline/output_keys.rs`)
  give `Project::motion_blur_at`'s sub-frames,
  weighed by File › Properties › Video › Motion blur type
  (`ProjectSettings::motion_blur`), which the engine's `Painter` renders
  and averages in linear light (`accel.rs`'s `blurred`); a video track's
  Bypass Motion Blur (`Track::bypass_motion_blur`: its header's MB, its
  More menu, a Mackie Control's Motion Blur in Video mode) keeps it at the
  frame's time in every sub-frame (`scene::frame_scene_held`,
  `bv_render::render_output_held`). The engine's
  `accel::Painter` uses the card picked in Options › Preferences › Video
  (the best by default) and the processor when there is none or it fails.
  Kernels must compile with Direct3D's old FXC compiler (many computers
  have no DXC): keep them small, no `switch` over effects, no dynamic
  indexing of vectors; `bv-gpu`'s `the_shaders_are_valid_wgsl_with_every_kernel`
  checks every module with naga where no card runs the tests (a WGSL
  reserved word, `from` say, makes the card fail, and the card's tests
  pass without one). A media's effects work on its own picture; an
  event's work on its framed picture — the frame's size, after its
  Pan/Crop — as VEGAS's do by default (its Pan/Crop first in the event's
  chain), but for those before its Pan/Crop in the chain
  (`Clip::pan_crop_at`: the Video Event FX window's Pan/Crop button,
  dragged or moved by its menu), which work on the media's picture too:
  `MediaLayer::framed_from`, and `Framing` for plug-ins reading other
  times. An event's Pan/Crop masks (VEGAS 2026's: `Clip::mask`,
  `bv_timeline::mask` — closed Bézier paths in the media's pixels, each
  positive, negative or disabled with its opacity, feather and
  anti-aliasing, their shapes keyframed anchor by anchor, kept through
  cuts, trims and joins; `mask_raster.rs` is the rasterizer, checked
  pixel by pixel against rendered frames: paths cut into monotone pieces, 16
  bands a line with their coverage summed by groups, even-odd, a 4th-order
  recursive Gaussian on 32-bit floats for the feather, each path combined
  with those before)
  reach both renderers as `MediaLayer::mask_picture`, the masks' coverage
  at the picture's size — the last two kept by `PanCropMask::fingerprint`,
  so masks that do not change are worked out once, and kept on the card
  too — framed as the picture is (`place_box`, the card's
  `place_hard` and `place_soft`): after the event's framed effects it cuts the picture, or with
  Apply to FX mixes the picture without them and with them (the card's
  `blend` kernel's modes 101 and 102, then Add). The Pan/Crop window's
  Mask row (`pancrop_keys::KeyRow`) gives its toolbar, properties and
  workspace to `pancrop_mask.rs` (`MaskEditor`: the tools, drags and menu,
  each path's feather drawn dotted where it ends, `mask::offset_polygon`,
  edits coming back as the event's new masks, one undo step a gesture; a
  click's place is where it was let go — `Response::interact_pointer_pos`,
  egui forgets the press's —, a drag's where it was pressed. As the
  user's VEGAS: anchors are picked (`MaskEditor::anchors`, their tangents
  shown, a drag of one moving them all, Delete taking them away — the key
  consumed, not the keyframes' under it) by a click, Ctrl and a click,
  Shift and a click, or a rectangle dragged off the paths with the Normal
  Edit tool (`MaskDrag::Marquee`, picking as it goes); the picked path's
  box — its corners scale it, the handle above it turns it,
  `MaskDrag::Rotate`, the small toolbar under it — shows only after a
  double-click on its line or anchors with the Normal Edit tool
  (`MaskEditor::boxed`), a click off the paths or another tool hiding it;
  the Anchor Creation tool's click is a corner — the line to it straight
  —, its drag a curve, a click on the first anchor closes the path; its
  and the Split Tangent tool's click on another anchor makes it a corner,
  its tangents kept aside (`MaskEditor::hidden`), and again brings them
  back (curved along its neighbors when none were kept, `toggle_corner`);
  their click on a line — any path's, the picked one's first — puts an
  anchor at its point nearest the click, the curve kept
  (`PanCropMask::split_segment`, de Casteljau's, in every keyframe's
  shape too; `segment_hit`: 64 samples a segment, then narrowed down
  between the samples next to it); the Normal Edit tool's drag of a line
  bends it from where it was taken, `bend` — that point follows the
  pointer, curved anchors stay smooth —, its two anchors picked so their
  tangents show as they follow; its properties VEGAS's grid — Mask,
  Position, Path, Workspace). The Pan/Crop window is
  laid out as VEGAS 23's: Show Properties (the gear, off at first,
  `UiSettings::pan_crop_properties`; the properties' width a splitter's,
  `widgets::pane_width`) — VEGAS's property grid (`section`, `prop_grid`,
  `prop_name`, `prop_value`, `prop_choice`, `prop_yes_no`: bands with ⊟,
  a dash and the name with a colon, cut short in a column shared by every
  section, values plain text with thousands grouped, `fmt::num_grouped`),
  its sections VEGAS's: Position, Rotation (the Angle and its X and Y
  Center, the pivot — `Transform::pivot`, in the box's own axes; the box
  turns about it, `pancrop::turned`, and the keyframes' positions
  interpolate straight between them —), Keyframe interpolation
  (Smoothness), Source (Maintain aspect ratio and Stretch to fill frame:
  `Transform::keep_aspect` — `keeps_aspect`, on for projects from before
  unless they stretch the picture; Edit › Switches' Maintain Aspect Ratio
  and the event's Properties keep the same switch,
  `BananaApp::set_maintain_aspect` — and `fill_frame`, the box within what
  the frame shows `box_size`, `pancrop::transform_for`: a box of another
  shape than the frame's keeps its picture's proportions, what is around
  it filling the rest; off, a pixel of the media a pixel of the frame;
  renderers draw the framing as before, these only say how the box maps
  to it, so `is_identity` leaves them aside) and Workspace (Zoom (%), X
  and Y Offset, Grid spacing) — over VEGAS's tools in its order —
  Normal and Zoom Edit Tool, Enable Snapping, Lock Aspect Ratio, Size
  About Center, the movement's mode in one button —; VEGAS's grid
  (`paint_grid`: the media's frame repeated in lines, a dot at each
  sixteenth of it, `PanEditor::grid` parts, which snapping follows — a
  move puts the box's middle on them, a resize the edges dragged on its
  lines, `snap_resize`: with the aspect locked the side nearer its line,
  the other following; a turned box sized in whole steps; Track Motion's
  box alike); the
  box, its rotation circle (0.55 of its diagonal) and the letter F in
  VEGAS's lines, a pixel wide, 3 pixels dark and 3 light in turn so they
  show over any picture (`ants`), its handles light, dark and light
  rings; a drag round the box, up to the circle, turns it, the pointer
  VEGAS's turning arrow there (drawn, the system has none:
  `fx_window::rotate_cursor`, Track Motion's and the masks' too); the shortcut
  menu VEGAS's six (Restore, Center, Flip Horizontal and Vertical, Match
  Output and Source Aspect). Its media is the event's media at its own
  needle, as VEGAS's — whatever the Video Preview shows (Sync Cursor off,
  its quality, proxies, effects) and however long its frames take —,
  decoded by readers of its own on a thread of its own (the engine's
  `source_view.rs`, `Engine::source_picture`: only the latest picture
  asked for counts; while its reader seeks the pictures on the way show,
  as scrubbing's, the exact one after) at the size the workspace shows it
  (the readers' halvings, `bv_media::reduction`; a still's pixels
  averaged down, kept for every time — the texture sent again only when
  its pixels change); the last one stays while the next is decoded, the
  media's thumbnail before the first, and the readers go when no window
  shows (`BananaApp::source_watched`, `Engine::close_source_view`).
  Its keyframe controller is `pancrop_keys.rs`, VEGAS 23's as the user's
  prints show it: the ruler over the event and a little past its end —
  that part white —, the Position and Mask rows, VEGAS's needle (a light
  pentagon on the ruler, an orange line), VEGAS's grey diamonds (picked
  ones white in a grey ring; the first at the start half shown), the
  rows' scroll bar at the right and the time's under them (its thumb's
  ends zoom, + and −, the wheel zooms round the pointer, Shift scrolls).
  Keyframes are picked with a click (the needle there), Ctrl and a click,
  Shift and a click (a run in the row), by dragging the needle from an
  empty spot across them (`StripDrag::Sweep`) or Ctrl+A; dragged together,
  the first one too, none past the event's ends (with Ctrl, copies), one
  undo step a drag; their menu (Cut, Copy, Paste, Delete and the curves,
  VEGAS's) and the keys (Ctrl+C, Ctrl+X, Ctrl+V at the needle, Delete;
  with nothing picked, the row's keyframe at the needle — Ctrl+C its value
  there —; Ctrl+← and Ctrl+→) work on all those picked (`Keys`: `moved`,
  `deleted` — a row keeps one —, `curved`, `copied`, `pasted`); a row
  without keyframes shows its value as one at the start, made when edited.
  Other keys are the program's (the keyboard map, Global), as VEGAS's.
- Motion Tracking (Tools › Video, Alt+M; an event's menu and More
  button) is VEGAS 2026's: an event's regions (`Clip::motion_tracks`,
  `bv_timeline::motion_tracks` — four corners 0–1 of the frame at each
  frame tracked, from the event's start, moved with it by
  `rebase_envelopes`) followed by `bv-track` (Shi–Tomasi features inside
  the region or outside it, pyramidal Lucas–Kanade there and back, the
  mode's transform — shift, turn, scale, affine, perspective — fitted by
  RANSAC) on the event's own picture, drawn alone by the engine's
  `motion_tracking.rs` in the background (`Engine::start_tracking`,
  `tracking`); the window and the region on the Video Preview are
  `motion_tracking.rs`, whose motion transfer puts the region's middle
  into an effect's location parameter as keyframes
  (`MotionTrack::location_keys`) or into a new Bézier Masking, and whose
  import menu (shown when the event has one) makes a region following a
  Picture In Picture's picture or a Bézier Masking's mask — at each of
  the event's frames when the effect is animated —, their corners from
  `bv_effects::tracked_region` (`Engine::tracked_region`).
- Video readers: `bv_media::VideoPool` gives each event its own reader (a
  crossfade between two parts of one file would otherwise seek every
  frame); file readers run on their own threads and decode ahead — played
  backward, the stretch before the picture asked for
  (`pool::stretch_behind`).
  `bv_mf` decodes on the graphics card (DXVA through Media Foundation,
  Options › Preferences › File I/O) and copies the pictures out a few
  frames late, so nothing waits; HEVC Main10 stays 10-bit (P010).
  MPEG-1/2 and thumbnails decode on the processor (Windows' MPEG-2
  decoder loses a file's last frames on the card after a seek). While scrubbing, a newer position
  interrupts a reader's seek, and a seek slower than the preview's patience
  shows the frames it decodes on the way (`VideoPool::set_patience`).
- The Video Preview's Transform mode (`transform_ui.rs`): the event at the
  cursor (the selected one there, else the top one) framed on the picture
  as the Properties panel frames it (`pancrop::transform_from_key`: a
  picture fitted, moved, scaled, turned), its box the picture's corners
  through the placement's `to_media` turned around; drags and the
  Transform window's fields write the box back (`key_from_transform`,
  `Project::set_clip_box_at`: the keyframe at the cursor when the event
  has keyframes, else its one box).
- Docking (`dock.rs`, egui_dock): a tab dragged lights up the room it
  would take (`OverlayType::HighlightedAreas`); Float in Its Own Window
  (the tab's menu) moves a window into an OS window of its own
  (`floating_windows`, `BananaApp::floating_panels`, kept in the session),
  Dock puts it back where windows open.
- Playback waits for its first frame (`BananaApp::preroll`): while the
  Video Preview shows and has not drawn the frame where playback starts
  exact (a seek in a long-GOP recording decodes for a second), the sound
  starts only once it has (3 s at most), so picture and sound start
  together. The decoders know an MP4's or MOV's keyframes
  (`bv_mf`'s `mp4::video_keyframes`, from `stss`, `stts` and `ctts`): a
  later frame with no keyframe on the way is decoded on to, however far,
  instead of seeking back to the keyframe before (`seek_speed` and
  `playback_start`, ignored tests, measure a file).
- The preview's frames (the engine's `preview.rs`) are drawn by two
  threads, each with its own painter, readers and plug-in instances; the
  frames asked for (a still, scrubbing, playback's first) and Dynamic RAM
  Previews are the first's (its readers are where they go on from).
  While playing, frames keeping up are drawn one after another; slower
  ones for when they will show — the frame the position will be in when
  it is done, by how long frames take lately (`Pace`), however long that
  is (`next_ahead`'s horizon never nearer: frames slower than `LOOKAHEAD`
  froze the preview for the rest of the playback) —, so a preview
  slower than the project's rate drops frames evenly instead of drawing
  frames that come after their time; when one thread cannot keep up
  drawing on the card and the frames do not wait on the decoders (effects
  do the work), the second draws alternate frames at once, tried and kept
  only while that shows more frames (`Pace::plan`; on the processor a
  frame keeps every core busy already). Its readers must open and seek
  first, decoding up to a keyframe's distance beside the first thread's:
  it is tried only when the frame playback started from seeked quickly
  (`SEEK_LIMIT`) or reads a single media (two 4K long-GOP decoders more
  stalled a transition's frames for seconds); its first frame is not
  counted in what frames cost, and a frame being drawn for a time the
  position passed holds no lane (the other thread waited on it).
- The Transform window (`tabs::transform`, a docking tab opened beside
  the browsers, `dock::open_beside_browsers`): the controls of
  `transform_ui::target`'s event as other editors' effect controls —
  foldable sections, each property's name, frameless numbers in the
  accent's color (`hot`), a reset button per row (`prop`) — written
  through the same framing as the Transform mode's box
  (`transform_ui::framing`/`write`) and the event's fields; the Pan/Crop
  keyframes beside the rows on a strip of the event's time. The time
  selection the timeline shows (`BananaApp::shown_selection`): the loop
  region unless a click elsewhere let go of it (`hidden_selection`), when
  only its bar shows it.
- Frames for the screen come back at the size the screen shows them
  (`Engine::request_preview_in`'s room; `bv_gpu::room_size`, by
  sixteenths so that resizing the window a little keeps the size): drawn
  at Preview Quality's size — Full, Half or Quarter, their effects as at
  that size —, the card reduces them (2 × 2 averages, then bicubic,
  `Output::DisplayIn`) before they are brought back, so a 4K frame in a
  1280 × 720 window brings back and uploads a sixth of the bytes. Auto
  draws at the room's size already; the 360° View and the scopes take
  whole frames, and a snapshot is drawn on its own at the project's
  size. The status line names the size drawn (`PreviewFrame::drawn`).
  Frames' bytes are used again (`bv_gpu::give_back` when a
  `PreviewFrame` goes, the interface's image when the texture took its
  pixels): fresh memory for each 33 MB 4K frame cost more than the copy.
  A decoded picture's planes are written straight into the card's upload
  memory (`write_buffer_with`), and an event a pixel for a pixel on the
  frame is placed by its nearest pixel, not bicubic's sixteen (as
  `bv_render::place_box`). Video thumbnails wait while playback or a
  render runs (`thumbs::hold_thumbnails`): a 4K long-GOP recording's
  decoded on the processor took the cores the preview needed. Before events start (cuts,
  transitions) their readers
  are positioned — those left elsewhere by an earlier playback too,
  `VideoPool::ready_at`; never those of events playing on — and their
  plug-ins readied (`Painter::ready`: a thread of the painter's draws
  those first frames tiny, sharing its plug-in instances, the media
  standing in as black, so that instances — a second to make for some
  plug-ins, Sapphire's — and their slower first frames are ready; the
  other threads ready theirs too; instances slow to make are kept five
  minutes unused). Plug-ins load, and instances are made, outside the
  registry's and the instances' locks, once, others waiting only for
  them. `BV_PREVIEW_TRACE=1` prints each frame drawn (the thread, why,
  how long, how much of it waited for pictures), the long waits for a
  reader and the readers asked ahead; `bv-engine --test desempenho
  playing_through_a_transition` measures playback through transitions,
  `--lib third_party_parallel_speed` one painter against two,
  `--lib third_party_speed` other makers' plug-ins (`BV_SPEED_WATCH`:
  also as while their FX window is open).
  Frame Blend (File › Properties › Video's Resample mode,
  `ProjectSettings::resample`, or an event's switch, `Clip::resample`):
  where a file shows at another rate than its own, the scene gives the
  next frame and its share (`MediaLayer::blend`, `scene::frame_blend`),
  read by a second reader of the event (`bv_media::blend_owner`,
  `needs_of`) and mixed in by both renderers in the composite's space.
  Optical Flow (`ResampleMode::OpticalFlow`, `MediaLayer::flow`) makes
  that frame along what moves instead: `bv_track::flow`'s dense flow
  from the frame before to the next (`pictures_flow`, the luma brought
  down by halves) and `between`, each pixel from both frames along its
  move, mixed — on the processor (`bv_render`'s `flow.rs`, the last
  flows kept for the frames between the same two; `accel.rs` sends those
  frames there).
  File › Properties › Video's "Adjust source media to better match
  project or render settings" (`ProjectSettings::adjust_source`, on for
  new projects) enlarges a standard definition media (or a media in a
  standard definition project) whose shape is within 3 % of the frame's
  so that it fills it (`scene::adjust_factor` on the event's transform,
  as a Pan/Crop would): DV beside HD, 486 lines in 480.
  A video's field order (`bv_core::FieldOrder`) comes from the system's
  codecs (`bv_mf::VideoInfo::field_order`: Media Foundation's interlace
  mode, FFmpeg's field order) into `VideoStreamInfo::field_order` (Media
  Properties change it); where it is not the project's
  (`ProjectSettings::field_order`), the project's Deinterlace method
  (`DeinterlaceMethod`) makes whole frames at Good and Best quality
  (`MediaLayer::deinterlaced`): Blend Fields as the flicker filter,
  Interpolate keeping the field shown first (`layer::interpolate_fields`,
  the card's `interpolate_fields` kernel), Smart adaptive moving each line
  of the other field toward Interpolate's by how far its luma stands out
  (`layer::smart_fields`, the same kernel with `j.y`,
  `SMART_THRESHOLDS`). Renders write fields when the template says
  (`VideoParams::field_order`): each frame the field shown first drawn at
  its time and the other half a frame later, woven (the engine's
  `weave_packed`, `weave_image`); Media Foundation's encoders are told
  (`VideoEncoderSettings::field_order`, progressive where they refuse),
  ProRes codes each field as its own picture (`Encoder::interlaced`). An
  event's Undersample rate (`Clip::undersample`) holds its media's frames
  (`scene::media_time`).
- Smart rendering (File I/O's "Enable no-recompress rendering",
  `RenderOptions::no_recompress`; the engine's `render/smart.rs`): a frame
  whose scene is one plain event — no effects, motion, fades, opacity,
  Pan/Crop, masks, frame blending or color management on its way — of a
  media of the size, rate, fields and ProRes profile being rendered, shown
  at one of its frames, goes into the file as the media's own compressed
  frame (`Output::passable`/`compressed`, `Work::Compressed`; the header
  checked by `bv_prores::frame_info`); `RenderProgress::passed` counts
  them.
- Options › Preferences (`crates/ui-egui/src/preferences.rs`) edits a draft
  applied on OK/Apply; the General and Video tabs' settings are
  `settings::Prefs` (`UiSettings.prefs`), read where they act (timeline
  thumbnails, the Video Preview's overlays, the File menu, Space/F12 —
  Play/Pause or Play/Stop, Enter the other, `BananaApp::enter_key`; the
  timeline's playback runs on past the project's end until stopped,
  `PlayOptions::past_end`). An
  item is `Item::On` when it works and `Item::Later` (greyed out) while its
  feature is missing. File I/O's Log Plugin loading, Log Reads and Log
  Renders are `logs.rs` (levels set globally by `logs::set_levels`, the
  scans' threads read them; `Plugins.log`, `Reads.log` and `Renders.log`
  in `%LOCALAPPDATA%\BananaV\Logs`, in English; the audio output's
  failures — opening it, the device failing while open — always go to
  `Audio.log` there, `logs::audio` from `Engine::take_audio_errors`, and
  the status bar's "Audio: unavailable" tells why on hover); Display's "Display
  timeline at bottom of main window" moves the timeline's pane
  (`BananaApp::place_timeline`: taken out, `dock::ensure_timeline` puts
  it back on the side `timeline_top` says) and "Reverse Event Background
  color" swaps events' fill and waveform colors (`timeline/lanes.rs`).
  Questions to the user go through `ask.rs` (one modal
  at a time); autosave and recovery are `autosave.rs`; proxies
  `proxies.rs` (engine `proxy.rs`); Tools › Selectively Prerender Video
  `prerender_ui.rs` (engine `prerender.rs`, model `timeline::prerender`);
  recording `recording.rs` (engine `record.rs`, cpal input; Loop
  Playback's laps and the selected events' punch-ins become takes,
  `passes`, `Project::add_take_at`; a track recording from an input bus
  records its input through the bus's effects, `Project::resolved_input`)
  and live input `follow_live` (engine `live.rs`: each armed track's
  input monitoring, `Track::input_monitor` On or Auto, and each input bus
  heard through the Master — `bv_timeline::input_busses`, their strips in
  the Mixing Console, `FxTarget::InputBus`; their sends to assignable
  chains, `InputBus::sends`, played through the bus's effects and the
  chain's (`live_routes`) — routes of an input device's
  stream through effects chains into rings the output's stream mixes);
  Trimmer
  markers live in `MediaInfo.markers` (saved by `bv_media::save_markers`:
  inside WAV files, beside the others as `.bvmarkers`); edge trimming and
  Expanded Edit Mode are `TimelineView.edge_trim` (the split preview is
  `Engine::request_edit_point`); the edge the pointer trims is
  `lanes.rs`' `edge_at` (the nearest in reach, a crossfade's dark lines
  from either side: the start of the event coming in, the end of the one
  going out under it); an envelope's point under the pointer is VEGAS's
  white diamond (`envelopes::paint_hot_point`) with its tooltip "<name>
  at <time> is <value>" (`envelopes::paint_value`, the time as the ruler
  shows it), and a double-click adding a point puts the cursor on it (no
  time selection then); envelopes are drawn as VEGAS's (`paint_one`: the
  line two points wide edged darker, points diamonds of its color; volume
  and composite level VEGAS's purple, saved old defaults following it —
  `Prefs::follow_new_defaults`), a volume envelope linear in gain, +6 dB
  at the top and 0 dB in the middle (`EnvelopeKind::to_unit`,
  `VOLUME_ENVELOPE_TOP`); V and P are VEGAS's "Insert/show/hide track
  volume (panning) envelope" (`show_track_envelope`: inserted on the audio
  tracks picked that lack it, else shown or hidden; the track list takes
  V, P and their Shift too); a video track's Expand Track Layers is
  `TimelineView.layers` (its events alternating between the A and the B
  roll, `layer_starts` worked out each frame, `TimelineView::band` placing
  an event, the transition roll drawn between by `lanes.rs`'
  `paint_transition_roll`, the track twice as tall); VEGAS 2026's
  timeline OFX keyframe envelopes are `timeline/ofx_envelopes.rs` (View ›
  Show OFX Effect Envelopes (Beta), `TimelineView::show_ofx_envelopes`; a
  video track's keyframes area under it — Expand Track Keyframes,
  `track_keys`, in the row's `keys` height — with a line per plug-in
  effect and Track Motion and the picked effect's envelopes; an event's
  effects' envelopes over the event): a plug-in parameter's number over
  its display range (`bv_ofx::ParamInfo` from the registry), its points
  the parameter's keyframes, interpolated by `bv_engine::plugins::animation`,
  dragged, added by a double-click, a Bézier point's handles dragged, and
  from their menus (`ContextTarget::OfxEnvelope`) typed (Set to…,
  `TimelineView::ofx_set_to`), given a curve, deleted, flipped, thinned or
  reset; the keyframes area's menu (`ContextTarget::OfxArea`) adds a point
  at the cursor to every envelope it shows and shows or hides each
  (`TimelineView::ofx_hidden`), and its lines' keyframes are dragged in
  time with their values kept (`OfxDrag::Times`); the external monitor is
  `external_monitor.rs` (an egui viewport full screen with
  `ViewportBuilder::with_monitor`, the displays from `bv_platform::displays`);
  Audio Device's buffering, output device and metronome are statics of the
  engine's `playback.rs`, as are the Master Bus's Downmix and Dim Output
  (playback only: `Engine::set_downmix`, `set_dim`; `bv_mixer::downmix`)
  and the Loudness Meters' live meter (`bv_mixer::loudness::LiveMeter`:
  BS.1770 loudness and true peaks of what plays, started again with each
  playback; `Engine::loudness`, the window `tabs/loudness.rs`, its
  options `UiSettings.loudness`),
  track buffering its `track_mix.rs` (a thread per track,
  `bv_mixer::mix_track` + `finish_mix`). Editing preferences reach
  the model as `bv_timeline::NewEvents` (`Project::insert_media_as`;
  `Prefs::new_events`), grouped edits as `delete_events`/`copy_events`
  with `grouped`; the quick fade length is a static of `bv-mixer`
  (`set_quick_fade`), pitch-kept stretching its `stretch.rs` (WSOLA, grain
  shifts worked out in runs from fixed places so blocks are identical
  however playback got there; an event's pitch change, `Clip::pitch` in
  semitones from = and − and its Properties, reads each grain
  `Clip::pitch_factor` times faster, its length kept); MIDI ports and Thru are
  `bv_platform::{midi_outputs, midi_inputs, MidiThru}` (winmm; opened by
  `ui-egui/src/midi.rs`), VST scanning is `ui-egui/src/vst.rs` on
  `bv_platform::{read_library, version_texts}` (headers only: plug-in code
  never runs while scanning). MIDI ports open once each in
  `ui-egui/src/midi.rs` (`MidiPorts`: thru, timecode/clock out, timecode in,
  control surfaces; `bv_platform::{MidiOut, MidiIn}` over winmm); MTC and
  MIDI clock math and the sending thread are `sync.rs` (`Generator`, fed
  each frame by `publish_transport`; following timecode is `Chase`); the
  Mackie Control protocol is `surface.rs` — its buttons as VEGAS's Help
  lays them out, a Mackie Control and up to four Extenders
  (`ControlDevice::extender`, each over its Channel Mapping's tracks,
  banked together), the V-Pots' assignments (`Assign`: Output, Input,
  Pan, Sends, Inserts' PL, PE and PS — PE's V-Pot 6 the edited
  effect's presets, `tabs::console::effect_presets` —, Settings with F1–F6
  held), Flip,
  Video, Shift+Meters/Values (`Channels`: the strips over the tracks,
  the busses or both, `surface_channels`), Automation, the display's
  values or meters, the time in the
  ruler's format; the user's mappings of its buttons to keyboard map
  commands (`ControlDevice::mackie_map`) and its Configure Mackie Control
  dialog are `mackie_config.rs` —, a Generic Control (any MIDI
  controller doing what its mapping says, `ControlDevice::generic`, up
  to five) `generic_surface.rs` — its host commands by function group,
  channel strips banked over audio or video tracks, trims or automation,
  and its Configure Generic Control dialog, whose Learn listens to the
  input `BananaApp::midi_learn` names (`midi.rs`' `learned` queue);
  track automation (trims or
  automation controls, Touch/Latch recording as one gesture, thinning) is
  `automation.rs` with `Track::automation` / `Track::automation_at` (the
  mixer and the scene builder read envelopes through it; `set_controls`
  moves the selected tracks' controls together, the volume by the same
  dB; a video track's Fade slider, `TrackControl::Fade`, is an automation
  control only, held in `TrackAutomation::fade` with automation off; the
  Mute button, `TrackControl::Mute`, edits the mute envelope — with
  automation off it mutes the whole track, in Read it only shows the
  envelope —, and the multipurpose slider, `TrackControl::Send`, the
  send's envelope — with automation off `Send::held`, a second trim —; a
  bus track's and a Mixing Console bus strip's controls are automation
  controls alike, `AutoOwner::Strip` with `Channel::automation` and
  `Channel::automation_at` — the mixer ignores a bus's envelopes, and its
  Surround Pan Keyframes, with its automation off and plays what its
  controls hold —, recorded through `Project::write_automation_to`;
  Ctrl+Shift+Up/Down size the bus tracks while their area has the focus,
  `TimelineView::bus_focus`);
  audio effects' automation (FX Automation) is `bv_timeline::fx_automation`
  — envelopes `EnvelopeKind::FxParam` (0–1 of the parameter's range,
  `ParamSpec::to_unit`) and `FxBypass` (held, as a mute's) among a
  track's or a bus's or chain's envelopes, naming the effect by its number
  in the chain (`Effect::id`, given when first automated) — which the
  mixer applies each block (`fx_automation::automated`; the DSP's
  `Processor::retune` follows new values keeping filters' memory,
  envelopes and delay lines, so nothing clicks), `Effect::automation_bypassed`
  leaving them out; the interface is `fx_automation_ui.rs` (the FX
  Automation Chooser, a track header's FX Automation Envelopes; a VST 3
  effect's parameters as its plug-in describes them, `vst_ui::param_infos`,
  their envelopes its normalized values by id, named from
  `VstFx::param_names`) and the
  FX window, where automated parameters show and set their envelopes'
  values at the cursor;
  video tracks' mute and solo follow their compositing parents
  (`Project::visible_video_tracks`: a parent's mute hides its children, a
  solo bypasses only its siblings that are not soloed); a track's Pan
  Type is `bv_timeline::PanMode` (`matrix`: the mixer places the pan and
  the pan envelope with it); switches that follow edits (Auto
  Normalize's gain, Trim Event to Include All Frames' length) are kept by
  `BananaApp::keep_switches` each frame through `History::amend`, as part
  of the edit that changed them (one undo), and so are sync links
  (`Clip::sync_link`: an event follows the one it is linked to by its
  offset, not the other way round; `keep_sync_links` tells which moved
  from the undo target, and waits while a drag is under way); Synchronize, Add Missing
  Stream, Shuffle Events, events' snap offset and Trim Event to Include
  All Frames are `bv_timeline`'s `arrange.rs`, the envelopes' Thin, Flip
  and Lock Envelopes to Events (`carry_envelopes`) its `envelope.rs`;
  an envelope is edited as VEGAS's: a point dragged moves, its line
  dragged moves its segment (`envelopes::segment`, `Drag::EnvSegment`:
  the points around it up or down, their times kept, Ctrl fine, after a
  few pixels so a click changes nothing), a double-click on the line adds
  a point, and over the needle the envelope comes first; a track
  envelope point's menu is VEGAS's (`lanes.rs`' `point_menu`, the user's
  prints: a composite level's Set to 100% and 0% Composite Strength, Set
  To… — `envelopes::SetTo`, a white box at the point, its value picked,
  read back by `parse_value` —, Delete, the fades, Flip and Thin All
  Points — Selected ones with a time selection — and Reset All);
  video events' own envelopes (`Clip::envelopes`: Velocity — −100 % to
  300 %, Freeze Frame at Cursor its drop to 0 — and Transition Progress,
  their times from the event's start; edited through `EnvelopeOwner`,
  drawn over the event by `timeline/envelopes.rs`) and Reverse
  (`Clip::reverse`, the media time an event mirrors around; its subclip
  in Project Media, `Project::add_reversed_subclip`) go through
  `Clip::source_time` (`source_offset`: the rate times the velocity
  summed, never back past the media's first frame), so frames, sound,
  thumbnails and waveforms follow them, and trims and splits keep what
  an event shows (`rebase_envelopes`); an event's takes (VEGAS's: other
  media it can play instead) are `bv_timeline`'s `takes.rs` —
  `Clip::takes`, all of them in order with their offsets from the active
  take's `source_in`, so trims and splits move them together, and
  `active_take`: the event's own `media` and `source_in`; a slip moves
  only the active one unless Options › Slip All Takes
  (`Project::slip_all_takes`, `slip_takes`) — and the interface's side
  is `takes.rs` (Next and Previous Take, the Take Chooser — its Play and
  Stop preview the take picked through the preview bus —, Rename
  Active, Add as Takes when media are dragged onto an event with the
  right mouse button; Open Copy in Audio Editor keeps the copy as the
  event's active take); multicamera editing (Tools › Multicamera) is
  `bv_timeline`'s `multicam.rs` — Create Multicamera Track (the cameras'
  tracks made into one of each kind, a take per camera, an empty
  `Take::none` "(no camera)" where one did not record, video and audio
  of the same time grouped), Expand to Multiple Tracks, the cut to a take
  at a time (`switch_take_at`) or over a time selection (`recut_takes`:
  neighbours that then play on in one take are joined again, `rejoin`
  keeping events of other takes apart) and the event the view shows
  (`multicamera_event_at`, at the frame drawn) — and the interface's
  `multicam.rs`: its mode shows every take in tiles in the Video Preview
  (a project of the takes framed in a grid, drawn by the engine's own
  renderer, `Engine::request_multicam`; on the external monitor too
  unless Show Full Frame on External Monitor), the active one outlined in
  Preferences' indicator color, and a click or a number key cuts to a
  take at the cursor — over all of the time selection when the cursor is
  in it and nothing plays; with Ctrl a crossfade, as Preferences ›
  Editing's cut-to-overlap conversion says (`Project::overlap_cut`) —
  (Edit Multicamera Audio with Video: the grouped audio too); Sync … to
  Align Events lines the picked events up with the one picked first
  (`BananaApp::first_pick`; one of each group, an audio one where there
  is one): by their sound, read in the background by the engine's
  `audio_sync.rs` (each event's onsets' envelope fitted to the target's
  at every offset at once through the FFT — `bv_mixer`'s `align.rs` and
  `fft.rs`, the fit believed by its margin over the next best offset or
  an almost exact match, thresholds measured on test sounds — then a few
  seconds of the sound itself compared around it, to a fraction of a
  sample; the status bar shows how far, with Cancel), or by their media's
  timecode or recording date (`bv_media::stamps`: an MPEG-4 family file's
  `tmcd` track and `mvhd` creation time through `bv_mp4::read`, on
  `bv_bitstream::isobmff`'s boxes; an MXF file's timecode component and
  a DV frame's timecode pack, `bv_mp4::tape`; a Broadcast Wave file's `bext` time
  reference and origination date; else the file's last write less its
  length); events that would have to start before the timeline stay, as
  VEGAS's;
  Beat Detection (an event's menu, Project Media's, the Trimmer's
  metronome button) and Tempo Detection (Project Media's menu) are
  `beats_ui.rs`, on the engine's `beat_detection.rs` reading a media's
  sound in the background and `bv_mixer`'s `beats.rs`: the flux of its
  low end (under 500 Hz, on the sound brought down to 4 kHz, 100 values a
  second), its peaks the beats — each with how far it stands out, so the
  dialog's Sensitivity picks among them at once (`beats_kept`), placed
  where the low end's energy rises fastest — and the tempo the flux's
  autocorrelation weighed toward 120 beats a minute, refined on a
  multiple of the period; the Trimmer shows the beats while the dialog is
  open, OK makes them the media's markers, and the tempo's OK sets the
  project's tempo, the ruler in measures and beats, the metronome and
  snapping to the grid as asked; Tools › Audio › Apply Non-Real-Time
  Event FX is `nonrealtime_ui.rs`, on the engine's `nonrealtime.rs`: the
  media samples an event plays (between the media times of its start and
  end; not reversed or frozen) through its audio effects
  (`bv_mixer::dsp::Chain`, their lateness taken back; the effects that
  change the sound's length — `EffectKind::works_offline` — on all of it
  at once through `bv_mixer::render_offline`) into a new WAV file, in the
  background, which joins the project as the event's new take
  (`Project::add_take`), the effects taken off the event as asked. While
  it plays, an event whose chain has such an effect plays its sound made
  through the whole chain the same way (`bv_mixer`'s `prerender.rs`: made
  on a thread of its own when the pool mixes live — `AudioPool::set_live`
  — the last one made playing meanwhile, at once when rendering), as long
  as the event; Auto Ducking is
  `bv_timeline::ducking` — a controller track's reductions
  (`Track::ducking`: Floor Reduction while it sounds, Peak Reduction more
  as it gets louder) and listener tracks (`Track::duck_listener`); each of
  the controller's events keeps its sound's loudness in its media's time
  (`Clip::duck_levels`, 25 levels a second, read by the engine's
  `ducking.rs` in the background, with its own reductions after
  Re-Analyze Auto Ducking), so `Project::ducking_gain` follows the events
  wherever they go; the mixer lowers listeners by it (`duck`, a gain every
  32 samples ramped between) and the timeline draws it on them dotted —
  the interface is `ducking_ui.rs`; the Modern Equalizer (an audio
  effect, `EffectKind::ModernEq`, its parameters laid out by
  `bv_timeline::fx::modern_eq`: four global, then eight a band) is
  `bv_mixer`'s `modern_eq.rs` — up to eight bands (high and low passes 6
  to 36 dB an octave as Butterworth sections, a band pass as steep, its
  width kept, shelves and peaks from the RBJ cookbook's biquads), each on
  both channels, the left, the right, the middle or the sides; Mono Below
  high-passes the sides, Stereo Width scales them, Global Scale scales
  every band's gain (below 0 a boost becomes a cut) — and its editor is
  `ui-egui`'s `modern_eq.rs`, drawn in the FX window instead of the
  generic rows: the graph of the mixer's own curves (`band_curve`,
  `total_curve`, `mono_below_curve`: what is heard), each band's area in
  its routing's color, its circles dragged for frequency and gain, the
  wheel for Q, a double-click adding a band (its type from where) or
  deleting one, a right-click's Cut, Copy and Replace, the bands'
  toolbar, the picked band's type, slope, routing and knobs, the global
  knobs, A/B and the zoom; while it plays the mixer measures the effect
  the editor shows — `modern_eq::watch` names its chain's owner and
  place, `Chain::process_watching` notes what goes in and out — for its
  FFT analyzer behind the curves (`spectrum_db`) and its peak meters, and
  Settings' Oversampling (a parameter after the bands,
  `modern_eq::OVERSAMPLING`) runs the bands at 2, 4 or 8 times the rate
  between two Butterworth low passes;
  `camera_media.rs` holds DVD camcorder import and the Device Explorer.
  The Display page: colors are `theme` functions (`theme::panel()`, never
  constants) over a palette per interface type (`theme::Interface`,
  applied by `app/display.rs` when it changes); text and icons drawn on
  events stay light (`ON_EVENT` in `timeline/lanes.rs`) whatever the
  type. Track colors are an index in the model (`Track::color_index`) into
  `Prefs::track_colors` (VEGAS's palette, `TRACK_COLORS`); a new track
  takes the palette's color of its place, round again after the eighth, as
  VEGAS's (`Project::add_track`, `insert_track_at`, `new_track_color`);
  the snap line remembers what it snapped to
  (`TimelineView::snap_line`, `SnapKind`). The docked windows' tab bars
  and the docking of floating windows come from `vendor/egui_dock` (see
  its `BANANAV.md`). The timeline is one of the layout's windows
  (`dock::Tab::Timeline`), fixed in its place as VEGAS's: no tab bar,
  never closed or floated, windows dropped on it dock around it
  (`TabViewer::fixed`); the others dock around it, as in VEGAS's Color
  Correction layout (the preview and the scopes down the right side, the
  timeline over Color Grading). Layouts without it (saved before) get it
  below the docking area (`dock::ensure_timeline`); windows opened go to a
  pane that is not its (`dock::open_tab`). As VEGAS's docking areas: F11
  hides and shows every window docked around the timeline, Alt+F11,
  Ctrl+Alt+F11 and Ctrl+Shift+F11 those above, below and to its right
  (`dock::toggle_area`, `toggle_all`: the windows on that side as last
  drawn taken out, the layout kept in `BananaApp::docks_hidden` to bring
  them back as they were), Ctrl+F11 folds the track headers to their color
  bars and Ctrl+Alt+Shift+F11 does both (the timeline maximized); each
  window's tab bar ends with Maximize and Restore (egui_dock's
  `TabViewer::leaf_buttons`; `dock::maximize` gives its side of every split
  above it almost all the room, `restore` puts the shares back); keys and
  buttons ask through `BananaApp::dock_request`, done where the layout is
  drawn. View › Window Layouts' five are
  `dock::preset_layout`, as VEGAS 23's screenshots show them (windows,
  order, open tabs, room); saved layouts are `.bvlayout` files
  (`layouts.rs`: Save Layout As — name, Alt+D number, folder —,
  Organize Layouts — the folder's and the menu's, Assign/Replace, Move
  Up/Down, Activate, Clear, Delete Layout —, Save Layout, Reload Selected
  Layout, Ctrl+Alt+D and a number), their folder `Layouts` beside the
  settings (`UiSettings::layouts_folder`); a layout never drawn has NaN
  places, read back as 0 (`layouts::dock_from_value`). BananaV opens as it
  was left, as VEGAS does: `settings::Session` — the layout (the areas
  F11 hid back, a maximized window at its size), the toolbar and status
  bar, the View menu's timeline switches (`TimelineSwitches`) — read by
  `startup`'s `restore_session` and written by `keep_session` a second
  after it changes (never while a button is down; the file's other
  settings left as another BananaV window wrote them, `save_session`) and
  on exit. Only the program does this: the tests' apps, never started,
  keep the default layout and never write it.
  The Video Scopes are `tabs/scopes.rs`, as VEGAS 23's: eight views
  (`View`), each scope's mode and Video Scope Settings in
  `UiSettings.scopes` (`ScopePrefs`); their scales are drawn with or
  without a frame (none where no video track has an event: VEGAS's look
  without video), measured on VEGAS's screenshots — NTSC's U and V of
  BT.601's luma, the 100 ring a chroma of 0.5, the 75 % bars' targets
  ±10° and ±20 %, the waveform −20 to 120 IRE — and the traces worked out
  once per frame (`Trace`) as VEGAS's draw the calibration card (its
  screenshots, matched pixel for pixel): the waveform and the
  vectorscope read the last row of each 8 (`sampled_rows`), the parade
  the last of each 9, the histogram every pixel; the waveform and the
  parade one column of the frame per column of theirs, the vectorscope
  the first of each 8; a waveform point brightens a quarter of the way to
  grey 220 per pixel (`brighten`), the parade sets its channel's 235, a
  vectorscope dot is a cross in its chroma's color (`dot_color`: U and V
  as Cb and Cr at 0.91 around grey), the histogram's bars rise with
  ln(1 + count) ⁄ ln(1 + pixels), its mean and standard deviation under
  it; Studio RGB is on, as VEGAS shows the card. They read the Video
  Preview's frame, as VEGAS's do (at Best (Full) the project's whole
  picture); without the preview drawing they ask for the cursor's frame
  themselves (`PreviewState::requested_at`).
  The start area's chooser windows (Media Generator, Transitions, Video
  FX) share `tabs/chooser.rs` (categories, the plug-in list — a plug-in
  dragged out of it is chosen and its first preset, (Default), goes where
  it is dropped, `Frame::dragged`, as VEGAS's —, the preset
  grid — in columns, names on up to two lines, what is see-through over
  VEGAS's gray squares — where only the preset under the pointer
  animates); effect previews are drawn on the engine's
  `sample_picture`/`effect_sample` (straight colors with their alpha); a
  Third Party category shows a Filter of its plug-ins' makers
  (`Plugin::provider`, `ofx::provider`: the `.ofx.bundle` they come in).
  Filter packages (VEGAS's FX packages: a chain with its settings, a
  `.bvfx` file each in `%APPDATA%\BananaV\FX Packages`) are
  `fx_packages.rs` (`BananaApp::fx_packages`, in memory in tests and
  through `fx_packages_for_review`; the Save Plug-In Package dialog),
  listed by Video FX's Filter Packages category (`Category::packages`),
  the Plug-In Chooser's Filter Packages folder (its Save As…, and Save
  LUT…: `bv_engine::effect_chain_cube`) and the Plug-In Manager. Project
  Media's item menu has Select Timeline Events, Add at Project Frame Rate
  (`Project::conform_to_project_rate`, also the event Properties' Conform
  to Project Frame Rate: the playback rate within 0.25–4, the rest a
  velocity envelope) and Match Project Video Settings (`tabs/media.rs`).
  Project Media's bins are `bv_timeline::bins` (`Project::bins`); its tags, Search
  Media Bins' conditions and smart bins are `bv_timeline::tags`
  (`Project::media_tags`, `smart_bins`, `search_media`) with the Media
  Tags panel, the Quick Tags (`UiSettings::quick_tags`, Ctrl and a number
  while Project Media has the keys: `BananaApp::keys_tab`, the docked
  window clicked last) and the dialogs in `tabs/media_tags.rs`; its
  storyboards are `bv_timeline::storyboards` (`Project::storyboards`; the
  Main Timeline's is the timeline itself, `timeline_cells`,
  `move_in_sequence`) drawn by `tabs/storyboard.rs` (cells, drops,
  `MediaDrag::Cells`, the Main Timeline's red while it shows an older
  state of the timeline; a drop on a storyboard moves media out of the
  custom bin or storyboard they came from, `MediaWindow::drag_from`,
  unless Ctrl); a part of a media picked on its thumbnail (a drag across,
  I and O over it: `tabs/thumb_part.rs`, `MediaWindow::parts`, the keys
  read from last frame's `hovered` in `shortcuts`) or a storyboard cell's
  (`StoryCell::range`) goes to the timeline as `MediaDrag::MediaPart`; a
  window's inner splitters are `widgets::pane_width`. Edits that a drop
  makes one by one (imports, events placed) undo as one step:
  `BananaApp::as_one_undo` (`History::undo_mark`, `join_since`).
  Color swatches open VEGAS's color picker, `color_picker::ColorPicker`
  (shown after the dialog that opened it, like the Render window's
  Customize; its eyedropper samples the screen through `bv_platform`).
- OFX plug-ins (OpenFX 1.5, `vendor/openfx` has the headers): `bv-ofx` is
  the host — its suites (the variadic ones received in stable Rust per
  platform, `varargs.rs`), instances, parameters, overlays drawn with the
  draw suite (`interact.rs`) — or, OpenFX's first kind, with OpenGL
  themselves, as Sapphire's are for hosts it does not know (`gl.rs`: on
  Windows an offscreen context of the graphics card's drawing into a
  framebuffer object of the view's size, its projection canonical
  coordinates through `View::to_frame`, read back as `Mark::Picture`,
  which `ofx.rs`' `overlay_ui` lays over the preview's whole picture
  area, the frame's place in it taken into `to_frame` and the viewport —
  every overlay's controls go on past the frame's edges, where they are
  seen and grabbed, as Motion Tracking's region does — drawn again only
  when the view, the effect's values, the pen or the plug-in ask, and for
  a new time alone while playing at most every 100 ms (`OverlayDrawn`:
  reading it back waits for the card, which slowed the whole interface
  down) —; none where the card has no OpenGL for it,
  `bv_ofx::opengl_overlays`) — and binaries loaded
  at run time; plug-ins get the main window as the host's
  `kOfxPropHostOSHandle`, the parent of their own dialogs
  (`bv_ofx::set_host_window`, `app.rs`' `follow_host_window`). BananaV
  describes each binary in a process of its own when it starts
  (`--ofx-scan`, `bv_ui_egui::plugin_scan_child`; cached in
  `%APPDATA%\BananaV\ofx_cache.json`, written only when it changed; the
  child joins a job of its own, so what plug-ins start there ends with it,
  `end_with_the_child`; a describe that crashed is tried again twice and
  never kept — the next start describes it anew —, one that hung,
  `Error::Hung`, is; while a binary loads, Windows' own error boxes are
  off on its thread, `QuietErrors` — VST 3 modules' too —, so one whose
  libraries are missing or another program's of the same name fails with
  its error instead of an "Entry Point Not Found" box at every start) and
  loads plug-ins only when used, each alone (`bv_ofx::registry()`,
  `Binary::plugin`: Sapphire's binary holds 291, seconds to describe them
  all). The registry's lookups (`info`, `known`, `plugin`), which run
  every frame for every effect drawn, share the descriptions
  (`Arc<PluginInfo>`, an index by identifier) and never copy them:
  Sapphire's are tens of megabytes, and copying them took 23 ms a lookup,
  BananaV crawling. Projects keep a plug-in's values as `PluginFx`:
  effects (`Effect::plugin`), transitions (`Clip::transition_fx`, BananaV's
  own transition standing in when the plug-in is missing) and generated
  media (`Generator::Plugin`, read through `bv_media::plugins`). Frame
  scenes carry `PluginCall`s (with `FxSource` for plug-ins that read other
  times); the compositors hand the pictures to the engine's `OfxEffects`
  (`crates/engine/src/plugins.rs`: an instance per owner and purpose).
  Plug-ins that render with CUDA (OpenFX 1.5's CUDA render,
  `kOfxImageEffectPropCudaRenderSupported`: Sapphire's do) draw on the
  graphics card's pictures where they are, as `kOfxImageEffectPropCudaEnabled`
  renders: `bv-cuda` is NVIDIA's driver API, loaded at run time from the
  system's folder (declared from NVIDIA's documentation; nothing of
  NVIDIA's ships), and the host says it supports CUDA where the driver is.
  On Windows the compositor's Direct3D 12 buffers are made shareable and
  imported into CUDA (`bv-gpu`'s `shared.rs`, on CUDA's first device when
  it is the compositor's card — the device plug-ins use): the frame's
  picture is turned upside down into one (OpenFX's rows go up: the
  `mirror` kernel), the plug-in draws into another
  (`Instance::render_on_card`: the device's primary context current, its
  work finished before the pictures are used) and the picture comes back
  turned (`Compositor::plugin_on_card`, `transition_on_card`; the scene's
  `PluginEffects::on_card`, `CardPicture`). The engine's
  `OfxEffects::card_instance` picks them (RGBA floats, no Original clip,
  the interface not reading the input — it reads still frames' only —,
  never failed there: `OFF_CARD`);
  other times a temporal plug-in reads are sent to the card
  (`ClipSource::on_card`, `DevicePicture::upload`), and a plug-in that
  renders only with CUDA ("needed") gets the processor's pictures through
  the card (`render_through_card`). Elsewhere (no NVIDIA card, Vulkan)
  pictures go to the processor and back. The
  interface's side is `crates/ui-egui/src/ofx.rs`: the scan, the lists, a
  plug-in's settings as it describes them, its keyframe controller and
  its overlay over the Video Preview. Every call into a plug-in's code —
  its actions, its overlays, the threads it asks the multi-thread suite
  for — goes through `guard.rs`: on Windows `guard.c` (built by the
  crate's `build.rs` with the `cc` crate and the MSVC compiler,
  `cfg(bv_ofx_guard)`) runs it under structured exception handling, so a
  crash (an access violation, a C++ exception it lets out, a stack
  overflow) comes back as `kOfxStatFailed`, its binary is switched off
  until BananaV restarts (`Binary::crashed`: its calls fail at once) and
  the interface says so (`bv_ofx::take_crashes` — `Crash::at`, the module
  and offset that crashed —, `BananaApp::follow_plugin_crashes`,
  Plugins.log); elsewhere the calls go straight in. Before any plug-in
  runs the host puts a per-drive current folder ("=C:") in the
  environment when it has none (`plugin::keep_a_drive_folder`): the C
  runtime's `_spawnle`, which plug-ins start programs with, reads past the
  environment's end without one (Sapphire crashed one start in eight). Plug-in libraries are never unloaded
  (some hang in their shutdown code), but when BananaV closes every
  plug-in loaded hears OpenFX's unload action (`bv_ofx::unload_all`, after
  `run_native`, three seconds at most; `Plugin::unload`), as the scans'
  children do: Sapphire closes its preset browser then. Threads that call
  plug-ins get
  `bv_ofx::PLUGIN_STACK`, filter-like effects are made in the first of
  `FILTERS` that works, and reader/writer plug-ins (`PluginInfo::files`)
  are listed nowhere. `BV_OFX_TRACE=1` prints each action and how long it
  took, each multi-thread call and each property or handle the host could
  not give, to find out why a plug-in fails or is slow. The test plug-ins
  are `bv-ofx-testplug` (Invert with an overlay,
  Gradient, Wipe, Echo; Crash, a binary of its own, crashes as it
  renders; CudaInvert and CudaOnlyInvert, another, render with CUDA —
  where the card's memory is CUDA's, out and back with the driver's
  copies); `cargo test -p bv-ofx --test vegas_poc --
  --ignored` tries a folder of real plug-ins in processes of their own
  (`target/ofx-poc/report.md`): an installed VEGAS's by default,
  `BV_OFX_POC_DIR`/`BV_OFX_POC_LIBS` for others (Natron's free ones:
  `Plugins/OFX/Natron` and its `bin`, with `OCIO` set to one of its
  configs), `BV_OFX_POC_ONLY` only the plug-ins it names (the others
  described). Other programs' plug-ins are test material only, nothing of
  them ships, and BananaV never pretends to be VEGAS for them.
- BananaV's own video effects are OFX plug-ins too, as VEGAS's are:
  `crates/effects` (`bv-effects`, only OpenFX's API through `ofx.rs`;
  Crop, `crop.rs` (VEGAS's: each pixel kept, times Opacity, when its
  corner is inside the window — no smoothing —, the window turned
  clockwise by Angle), Color Curves, `color_curves.rs`, Color Grading,
  `color_grading.rs` (the Color Grading window's effect, version 2.0:
  VEGAS's Color Grading's own parameters — each wheel's red, green, blue
  and Y and its point, U and V; the RL wheels and their thresholds; the
  ten curves as VEGAS's text, `N:x:y:ix:iy:ox:oy:…`; Levels, Tone, White
  Balance, HSL; the Camera and Look LUTs (on, file — or Apple Log's
  published curve to linear or Rec. 709 —, interpolation, strength) —,
  all hidden, the window edits them; its work is `grading.rs`, as VEGAS's
  with the graphics card on: the Camera LUT, the HSL Curves one after
  another (each keyed on what the one before made), White Balance
  (Robertson's isotemperature lines and a cone matrix between the values
  raised to 2.4 and back), Levels, the RL wheels' and the Color Curves'
  tables (Bézier curves sampled with a single Newton step, as VEGAS's),
  Lift, two CDLs (Gain, Offset and Gamma by channel, then by Y),
  Saturation, Exposure, Brightness, Contrast, the panel's Exposure and
  the HSL always, then the Look LUT, the color held within 0–1 between
  the steps; `Grade::quantize` makes the steps 8-bit as VEGAS's are (cut
  to 12 bits, then rounded) for measuring: the ignored
  `matches_vegas_renders` compares the grade with VEGAS's own renders of
  20 cases, `BV_GRADING_INPUT`, `BV_GRADING_RENDERS`, `BV_GRADING_LUTS`
  (within a level in a few pixels; as BananaV renders, in floating
  point, within one, two in the chained HSL Curves); `wheel_values` and
  `wheel_point` turn a wheel's point into its color and back, as VEGAS's
  panel does; `cube` bakes a grade into a `.cube` file,
  `Engine::color_grading_cube`, and `pick` works out what the pickers set
  from the colors picked — Lift's and Gain's channels between their ends
  as VEGAS's eyedroppers, a white balance that leaves the color grey, the
  Color Selector's points —, `Engine::color_grading_pick`; on the card
  its LUTs are LUT Filter's kernel or Apple Log's, `apple_log`, holding
  the colors within 0–1, and its own steps the `grade` kernel on a buffer
  of its settings, a row each, and its tables, `accel::GradePlan`) — the
  panel is `tabs/color.rs`, laid out as VEGAS 23's (the user's
  prints): its header the FX level's menu — event, track, media, video
  output; the event's, track's and media's menus and the Video Preview's
  open it there — and what it grades, Bypass Color Grading, Reset All,
  More's Bézier Mask, Broadcast Colors and Export LUT; the navigation's
  groups folded at first (`ColorPanel::folded`), VEGAS's rows — the
  picked one blue, a changed item marked, `ITEM_PARAMS`, each wheel's
  view and the curves' pins at their right —; a group picked shows its
  items side by side in columns (`ColorPanel::section`), an item alone in
  half the width (`item`): a wheel's column (`wheel_column`: the wheel,
  R, G and B down its right over VEGAS's Y slider), the RL wheels after
  the Threshold column, the LUTs' boxes (`lut_column`), the Utilities in
  two columns (`utility`: VEGAS's sliders, `h_slider`, Levels' double
  ones, White Balance's and HSL's colored bands; their labels measured),
  the Color and HSL Curves filling the panel side by side; its Color
  Curves `grading_curves.rs` — Select Curves on the title row, the zoom
  in the graph's corner and pan, Page Up/Down and the tangents' keys,
  four graphs side by side once the column is wide (the pinned curves'
  left border dragged, `ColorPanel::pinned_width`) —; its values the effect's parameters at
  the cursor through
  `ofx::value_now`/`set_now`, the effect added by the first change; the
  wheel is `widgets::grading_wheel`, drawn as VEGAS's (a shaded ball in a
  ring of the colors its point gives, red at the top as on the
  vectorscope, its crosshair, the ball's meridian through the point and
  its equator) and handled as VEGAS's (`WheelChange`: the wheel dragged
  at a twentieth of the pointer, the point itself at a thirty-second,
  Ctrl ten times finer; a double-click resets it, a right double-click
  jumps; the mouse wheel moves it up and down, with Alt across, Shift out
  and in, Shift and Alt round; the arrow keys too), its point given in
  VEGAS's 85-pixel units; the sliders and boxes are VEGAS's (`slide`: the
  knob dragged, a click beside it a tenth, a double-click the default,
  the mouse wheel a hundredth, Ctrl a thousandth; `number_box`, three
  decimals); every curve point has handles, as VEGAS's
  (`grading_curves`' `add_dot`: a double-click adds one along its
  neighbours' line, `flatten`: on a point makes its handles level,
  `smooth`), the HSL Curves 0.5 where they change nothing; the HSL Curves'
  editor has VEGAS's tangents' handles, point menu and arrow keys
  (`ofx_curves`' `move_handle`, `lock_handles`), its swatches add a color
  and its two neighbours; the pickers — Lift's black point,
  Gain's white point, Select Neutral Tone, the HSL Curves' Color
  Selector — wait for a click or a rectangle on the Video Preview
  (`pick_on_preview`, the color under the pointer read out) and read the
  effect's input there (`bv_engine::plugins::watch_input`;
  `OfxUi::watch_held` keeps the FX windows off the watch meanwhile); Show
  Limits on Scopes has the Video Scopes draw the RL thresholds,
  `scope_limits`) —, Color Corrector,
  `color_corrector.rs` (VEGAS's math, measured on its pictures: in BT.601
  Y′UV each wheel moves U and V toward its angle by half its magnitude
  squared times its weight — whole at black, 0.5 and white, half a third
  of the way from black or white and a fifth from 0.5, 2^−(distance ⁄
  half)² —, the luma kept; Saturation scales U and V, Gain the luma, and
  Gamma raises the luma to a power in BT.709's encoding of it, as if it
  were light (`gamma_curve`; the power, `exponent`, on two lines through
  1: 1 + 1.663 (1 − gamma) below, 1 − 0.726 (gamma − 1) above — the
  calibration card at 0.5 and 2, the Secondary's presets at 0.85 and
  1.25), and Offset adds to the luma in 8-bit steps (`OFFSET_UNIT`:
  VEGAS's 0.2 is a fifth of a step, gone in 8-bit projects' rounding;
  its slider −254 to 254, as VEGAS's; values saved before 1.1 counted
  the whole range, brought up to date by `PluginFx::upgrade`),
  all measured on the card to an 8-bit step; `wheel` and `color_at`
  serve Color Corrector (Secondary)'s wheel and eyedroppers too),
  Brightness and Contrast, `brightness_contrast.rs` (VEGAS's math,
  measured on the calibration card through each of its presets: each
  channel moved by Brightness, then spread from Contrast center by 1 +
  Contrast below 0 and 1 ⁄ (1 − Contrast) above it, its 8-bit halves
  rounding down as VEGAS's — a 32nd of a step under the exact value,
  `TIE` —, negatives clipped; VEGAS 23's Automatic Contrast — Histogram
  Cut-Off, Minimum and Maximum Brightness (15 ⁄ 255 and 235 ⁄ 255),
  Analysis Mode, Calculate Brightness & Contrast — sets the three as
  VEGAS's did on the card and a photograph: Contrast center at Minimum,
  Brightness Minimum less the picture's dark end, Contrast the spread
  that takes its range to Minimum–Maximum; the ends the light's (Rec.
  709, every pixel) past the cut-off at each end — VEGAS's bright end a
  little higher, its cut still to measure (the test pictures
  "Automatic Contrast - testes"); over the frame at the cursor, the whole
  event measured by the host (BananaV's `kBananaVParamPropAnalysis`
  "Histogram": the engine's `motion_analysis.rs` `light`, its result
  reaching the plug-in, and what the plug-in sets in turn joining the
  same edit, through `ofx::response`) or each of its frames
  ("Histograms": the three keyed at every frame, as VEGAS's); on the card
  `Plan::Affine`), Levels, `levels.rs` (VEGAS's math, measured on the
  calibration card through each of its presets: each channel — all three
  alike under All, or the one Channel picks, alpha too — taken from
  Input start–end to 0–1 and clipped, through Gamma's curve, then to
  Output start–end, the input's span at least a 255th, its halves
  rounding down (`TIE`); Gamma's curve is VEGAS's exact one — the line
  `γ^1.883610 t` up to `0.0381838 γ^−0.941805`, then the power 1 ⁄ γ,
  above 1 `(1 + o) t^p − o` and below `((t + o) ⁄ (1 + o))^p`, the
  offset joining them —; Gamma 0.45–5; on the card `Plan::Curves`, Alpha
  through the processor), Black and White, `black_and_white.rs` (VEGAS's
  gray is the plain average (R + G + B) ⁄ 3, measured on the card's every
  color; below 1 Blend amount mixes the color and its gray, as Invert's
  blend — unmeasured), Invert, `invert.rs`
  (`c + (1 − 2c) × Blend amount`, measured at 1 and 0.6), Gradient Map,
  `gradient_map.rs` (VEGAS's: its panel's gradient bar is a group's
  interface of the plug-in's own — markers picked, dragged, added on the
  bar and dragged off below it, the picked stop's color in Color, kept
  in sync on instance changes —, the stops text in a hidden parameter;
  as VEGAS's renders show it: the premultiplied pixel's BT.601 luma through the
  stops' spline, `spline.rs`, each stop's weight 0.8, alpha and
  premultiplied colors held as VEGAS holds them, mixed in by the
  gradient's alpha times Amount or added; transparent pixels take the
  gradient's start, so it renders every pixel itself; its starting stops
  BananaV's until VEGAS's are known; on the card `Plan::GradientMap`,
  `kit_gradient_map` reading each piece's Béziers), Sepia,
  `sepia.rs` (measured through its six presets: with `A = 1.5 ×
  strength` each color goes toward that average gray by `min(A, 1)`,
  then toward Color by `A − 0.5 − falloff × gray`; its presets' colors
  8-bit values the picker shows rounded in HSV) — each `Plan::Affine` on
  the card (Sepia with falloff through the processor), Gaussian Blur,
  `gaussian_blur.rs` (the Gaussian as VEGAS blurs, `recursive_gauss`, its radii each
  range × the frame's longer side in pixels, the vertical one times the
  pixels' shape — the pixel aspect times the render scale's height over
  its width —, the edge pixels held, nothing blurred only with both
  ranges 0; the Channels group's channels blurred, the others kept, as
  the premultiplied values stand), Sharpen, `sharpen.rs` (the binomial 3 × 3 average, each channel
  moved from it by 16 over the kernel's sum, VEGAS's whole middle weight
  less 12 — 29 − ⌊16 × Amount⌋ from 0.25, steeper lines below —, edges
  held, nothing below an Amount of 10⁻⁵), Vignette, `vignette.rs` (an
  ellipse of the frame's pixels, each by its corner, widened by `s = (1 +
  Aspect) ⁄ 2` across and narrowed by it up and down around Center,
  turned clockwise by Rotation as VEGAS's, the half diagonal 1 — the
  wider way of non-square pixels shrunk by their aspect —: the color's
  share `t^(100 ⁄ (Feather + 0.001)²)` from Inner radius × Outer radius to
  Outer radius (a pixel's span when they meet), times its alpha, Invert
  the other share; over the premultiplied colors, the alpha kept, every
  channel held within 0–1), Mirror, `mirror.rs` (VEGAS's, in its floats:
  rows from the top, the line through `((w − 1) Center x, (1 − Center
  y)(h − 1))`; within half a degree of 0, 180, 90 or −90 rows or columns
  reflected between two (`Way`), any other Angle a slanted line at
  `tan(±180° − Angle)` crossing each row a whole column past it, the
  reflected pixels read between the four around their reflection; past
  the picture the Background's color with no alpha, as VEGAS's pictures
  hold it; on the card `kit_mirror` with `mirror::Layout`) and Glow, `glow.rs` (the plain average of the
  channels blurred by VEGAS's Gaussian of radius half of Glow percent ×
  the frame's longer side — the measured sigma, `0.15 × Glow percent ×
  the width`, is this radius's —, Suppression taken away, times
  Intensity, within 0–1, how far each pixel goes toward Color) — on
  `spatial.rs`, what effects working a whole picture out share (the
  source read once, Gaussian blurs with the edges held); on the card
  Gaussian Blur and Glow through the recursive filter
  (`kit_gauss_rows`/`kit_gauss_columns`, then `kit_gauss_pick`,
  `kit_glow_apply`), Sharpen through `kit_conv_h` and `kit_conv_v` and
  `kit_sharpen`, and pixels (`Plan::Vignette`,
  `kit_vignette`; `Plan::Mirror`, `kit_mirror`, a corner a thousandth of
  a pixel from the line on it) —, Color Corrector
  (Secondary),
  `color_corrector_secondary.rs` (VEGAS's math, measured on its pictures:
  luminance Y × 255, saturation |UV| × 255 — pure red's 0.632 is its
  slider's 162 — and hue the vectorscope's angle, each limit whole in its
  range and gone Smooth beyond, luminance's (and saturation's, unseen) as a
  smoothstep, hue's straight, a grey's hue 0°; the three's product squared
  mixes in U and V turned, moved by Chrominance as a wheel with all the
  weight and scaled, the luma through Gamma's curve times Gain plus Offset;
  Alpha scales the alpha where the mask is, through `colors::render_rgba`;
  Select effect range spans a click's 5 × 5 pixels or a rectangle dragged
  on the preview, drawn as it is), Color Balance, `color_balance.rs`
  (VEGAS's math, measured on its pictures and the calibration card: each
  channel moves by its slider (−0.5 to 0.5) times a weight of the
  channels' mean I — I(2 − I) for Highlights, 4I(1 − I) for Midtones,
  1 − I² for Shadow —, Preserve luminosity taking half of each slider's
  move from the other two channels), Cookie Cutter,
  `cookie_cutter.rs`, Layer
  Dimensionality, `layer_dimensionality.rs`, Pixelate, `pixelate.rs`
  (VEGAS's cells: `⌊w (1 − p) ⁄ 2 + 1⌋` across a frame `w` pixels wide at
  full size, none below 0.0001, a step of the picture's pixels over their
  number apart in 16,384ths from its first column and bottom row, each
  the plain average of the step's whole pixels from its first) —
  it took the place of BananaV's older Pixelate, `EffectKind::Pixelate`,
  no longer offered but kept for the projects that have it — and LUT
  Filter, `lut_filter.rs`, on `bv-lut`'s `.cube` tables (read once per
  file and again when it changes; VEGAS's has no presets, BananaV's are
  looks of its own, `lut_filter::looks`, written to the user's folder
  when first needed: a preset's File starts with `bv_effects::LOOKS`,
  which `presets::builtin_plugin` puts in place), Picture In Picture,
  `picture_in_picture.rs` (as VEGAS's pictures show: the picture
  placed by its middle, scales — a part of the frame's, their sliders
  logarithmic through BananaV's `kBananaVParamPropLogarithmic` —, a
  vertical shear in the frame's true proportions and a clockwise angle,
  or put at its corners in Free Form, a projective map; the corners and
  the other settings kept together, `keep_together`: the corners follow
  the picture in the fixed modes, and in Free Form the location is the
  corners' mean and the rest their nearest parallelogram; a corner
  dragged stops before the shape folds, a triangle; the ring's handle
  turns and scales; each pixel averages 2×2 to 4×4 samples of four
  pixels each, the same on the card; its regions of interest ask for the
  whole picture), HSL Adjust,
  `hsl_adjust.rs` (VEGAS's math, measured on its pictures: the HSL
  model's lightness, (max + min) ⁄ 2, the saturation scaled with no
  ceiling — past the cube, where 8-bit projects clip it —, the lightness
  held within 0–1),
  Saturation Adjust, `saturation_adjust.rs` (VEGAS's math, measured on
  its pictures: each color scaled about its BT.601 luma by 1 + the
  adjustment for its saturation — its distance from that gray, a part of
  pure green's —, Amount at Center fading smoothly into Low below and
  High above Spread ⁄ 2 away), Border,
  `border.rs` (VEGAS's math, measured on its pictures: a band Size × half
  the frame's width wide; Blurred the picture times the rectangle that far
  in blurred by 0.446 × the width, Beveled four facets parted by the lines
  from the corners — at Size 1 a pyramid — each times Color and 1.05 +
  0.70 cos(its side against the light's Angle), Solid the Color with its
  alpha; the angle and the colors hidden for the types that do not use
  them, as VEGAS's are, through `kOfxParamPropSecret`), Black Bar Fill,
  `black_bar_fill.rs` (VEGAS's math, measured on its pictures: the
  picture scaled about the frame's middle to each layer's Location, the
  background blurred by VEGAS's Gaussian of radius Blur × the frame's
  longer side (up and down times the pixels' shape; none at 0) with its
  edges held, the foreground's shadow its alpha blurred by 0.022 × the
  width × Shadow Blur, black at 0.95; two crosshairs on the preview; on
  the card Picture In Picture's kernel draws both layers), Unsharp Mask,
  `unsharp_mask.rs` (as VEGAS's renders measure: the picture blurred as
  VEGAS blurs, of radius Radius × the frame's longer side, up and down that
  over the pixel aspect, its edges held; each channel, alpha too, as the
  premultiplied values stand, moved away from the blurred one by Amount ×
  their difference unless the result is within Threshold of the blurred
  one; Amount ±100,000 as VEGAS's; nothing changes with a Threshold
  rounding to 255 8-bit steps; it took the place of BananaV's older
  Sharpen, `EffectKind::Sharpen`, kept for the projects that have it),
  Add Noise, `add_noise.rs` (VEGAS's math, measured on its pictures: each
  pixel's noise — one for the three channels when Monochromatic — even
  over 2⁄3 × Noise level, or Gaussian with a standard deviation of 2⁄3 ×
  Noise level × (1 − Noise level ⁄ 4), added and folded back past black
  and white; a hash of the pixel's place, the channel and the time,
  whole-number math the card's kernel repeats exactly), Timecode,
  `timecode.rs` (VEGAS's look, measured on its pictures: the frame's time
  on the ruler in one of the ruler's formats, white on a black box, an em
  1⁄14 of the frame's height, the text's end 0.102 of the width in, the
  baseline 0.895 down; its letters are Arimo Bold's, Arial's metrics —
  `arimo.rs`, glyph outlines kept in the code, the font's license in
  `crates/effects/fonts/OFL.txt`, shipped with the packages — drawn by
  `text.rs`, an accumulation rasterizer; the card lays the processor's
  drawing over the picture, `accel::Overlay`; numbers take the
  interface's decimal separator, `bv_core::locale`), Titles & Text,
  `titles.rs` (a generator: VEGAS's text generator as its Help describes
  it — the computer's fonts found by `fonts.rs` in the system's and the
  user's font folders, named from their `name` tables and weighed from
  their `OS/2` tables, read whole only when text needs them; their
  outlines and metrics through `skrifa` (MIT or Apache-2.0, already in
  egui's tree), kerned by the font's own pairs (`kerning.rs`: the GPOS
  table's kern feature, else the kern table), drawn by `text.rs`'s
  rasterizer, cubic curves too; a
  missing bold or italic face made up; sizes in points at 1080 lines;
  the outline from the letters' distance field, the shadow offset and
  blurred, a background; parts of the text in styles of their own (the
  FX window's text box's selection, BananaV's `kBananaVParamPropRichText`,
  kept as runs in its hidden Styles parameter, `parse_runs`; the host
  draws the text as a page — its name and clock, its bar of font, size
  with a word processor's sizes, B and I, the justification's buttons and
  Transfer Text Properties, `kBananaVParamPropSuggested`,
  `…ButtonStyle`, `…OptionIcons`, `…Icon` and `…Transfer`, then the text
  dark on white in its font, `bv_engine::plugins::font_face` added to
  egui's fonts); its box
  dragged and its corners scaled on the preview; VEGAS's defaults and
  Animation list wait for its pictures),
  the transitions, `transitions.rs` (VEGAS's Linear Wipe, Clock Wipe,
  Iris, Barn Door, Venetian Blinds, Star Wipe, Push, Slide, Squeeze,
  Split, Swap, Zoom, Spiral, Flash, Dissolve, Cross Effect, Portals, Page
  Loop, Page Peel, Page Roll and Gradient Wipe — `gradient_wipe.rs`: a
  gradient picture Search… picks,
  handed over on a clip of its own and kept small in the settings, or
  BananaV's shapes; as VEGAS's renders show: the gradient made 8-bit steps at the
  frame's pixels (the picture cut to the frame's shape and enlarged when
  Maintain aspect ratio), stretched from its darkest step to its lightest,
  mirrored, flipped and blurred as VEGAS blurs 8-bit pictures, kept
  between frames (`Settings::steps`); each step's share of the first
  picture VEGAS's ramp (`ramp`; a hard edge shows the second picture at
  the start where the gradient is black, as VEGAS's does); the card reads
  the steps and the ramp, `tr_gradient_wipe` —,
  and 3D Blinds, 3D Cascade, 3D Fly In/Out and 3D Shuffle — as OFX
  transitions, the Transition context —; Linear Wipe, Clock Wipe, Iris, Star Wipe,
  Dissolve, Flash and Cross Effect are VEGAS's own panels, presets and
  math (`transitions/pixel.rs`: in the frame's pixels, x from the left and
  y from the bottom or v from the top as VEGAS counts, the pixels' shape
  taken in as VEGAS takes it, `Frame::shape`, in floats; Linear Wipe's
  distance along its angle from the frame's corners, Clock Wipe's hand
  softened by at least asin(√2 ⁄ r), Iris's fourteen shapes' reaches —
  the nearest repeated cell's — over the largest on the frame's edge,
  Star Wipe's map of its arms' reach in cycles and waveforms, mirrored
  and stretched as VEGAS's, both with VEGAS's border band and feather,
  `Band`; Dissolve's ten types; Flash's white f² and the picture's light
  blurred; Cross Effect's fade over Fade range, its zoom by Max zoom to
  the power of the progress, its cells as VEGAS lays them, `Cells`, its
  blur) — Flash and Cross Effect blur as VEGAS's effects blur
  (`recursive_gauss.rs`: Deriche's fourth-order recursive Gaussian, the
  radius where it falls to 1⁄255, sigma (|r| + 1.5) ⁄ √(2 ln 255) and at
  least 0.6, a constant past the edges; the card runs it as its two
  oscillations side by side, each one complex pole, `z = x + p z` — which
  floats keep accurate however wide the blur —, `sections`,
  `kit_gauss_rows` and `kit_gauss_columns`) —, Iris's
  and Star Wipe's frame numbers kept between frames (`KEPT`), Dissolve's
  and Cross Effect's unused controls greyed or hidden as VEGAS's
  (`pixel::show`); Barn Door, Push, Slide, Swap, Squeeze, Split, Zoom and
  Spiral are VEGAS's own panels, presets and math too
  (`transitions/moves.rs`: each frame a list of steps, `Drawing`, over a
  frame that starts transparent, rows from the top and places in
  16,384ths of a pixel as VEGAS's — a picture or part of it drawn into a
  rectangle, `Blit`: the rectangle's edges covering pixels partly, read
  between pixels corners aligned (the first pixel at the source's start,
  the last a pixel before its end, the others at their left and top
  edges) or, Zoom's growing picture, averaged over each pixel's footprint
  when smaller; and VEGAS's colored border along some edges, `Border`: a
  band Size wide softened over Feather — at least two pixels, the size
  giving way — outside it and inside the edge, round past the ends of
  edges along x; each transition's rectangles, edges and order as VEGAS
  lays them out, `Move::drawing` — Spiral is Slide's move, its angle
  turning by Turns and its size from Zoom; in odd frame sizes Barn Door's
  doors meet in a middle column each covers half of, as VEGAS's do);
  Venetian Blinds is VEGAS's as well (`transitions/blinds.rs`: the
  blinds' openness a table of 8-bit steps, one a pixel, twice the frame's
  diagonal long, open within half the period times the progress of each
  multiple of the period — the frame's width over the number of blinds,
  whatever the angle — from the middle out, softened by VEGAS's blur over
  Feather times the table's length and rounded back to steps; each pixel
  reads it at the middle plus its column's and row's offsets along the
  angle, in 16,384ths, between steps); so is Portals
  (`transitions/portals.rs`: seeded random numbers — `R250`, a shift
  register of 250 words — place the
  squares, each showing a part of the second picture moved and scaled a
  little, appearing one by one through the first half, the second picture
  fading in under them at the middle, going in the reverse order through
  the second half; drawn by `moves`' steps: the pictures composited —
  `Blit::composite`, the alpha in the edges' 16,384ths, single columns
  drawn — and each square's border as strips of its color laid over,
  `Step::Fill`: whole pixels, then the outer edge's parts, then the
  inner's); and so are the Loop and Peels (`transitions/pages.rs`: the
  frame's pixels, rows from the bottom, turned to the direction — VEGAS's
  angle in tenths of a degree folded into the first quarter for the
  frame's extent along it, `extent` — and each pixel's place along it
  mapped onto a page bent around a cylinder, layer after layer, `Curl::map`:
  Page Peel's front and back, the back lying flat mirrored over the page;
  Page Roll's four turns, its inside lit or shaded black; Page Loop's
  rising paper and loop, the second picture coming in over the first, its
  opacity from Paper opacity to whole by the progress squared, and where
  the loop is not its two layers drawing the paper again, as VEGAS's do —
  each read between pixels where it lands, the pixels past the picture
  being what is drawn so far, lit in 8-bit steps and laid over by its
  share; a pixel that read past the picture, or whose layers end otherwise
  than its right, upper and upper right neighbours', is the mean of four
  samples a third of a pixel apart read at their nearest pixels, VEGAS's
  smoothing, its last column comparing with what VEGAS's row before last
  left there, `draw`); and so are the 3D ones (`transitions/cards.rs`:
  VEGAS's cards in space — polygons moved by 4 × 4 matrices, `Mat`, its
  rotations turning as VEGAS's, seen by a camera the frame's longer side
  in front of it, a card at depth 1 filling the frame, clipped to the
  view and drawn one after another over what is under them (nothing, or
  the still picture): each pixel by the exact part of it a card covers,
  `Outline::cover`, its color the average of the card's picture over the
  pixel's footprint as VEGAS's best quality reads it — from the
  footprint's corner, at least a pixel, three times wider less two pixels
  when more — summed pixel by pixel, lit by VEGAS's two lights,
  the frame's along the view and a point light, `lit`; 3D Blinds' blinds
  turning in turn by Stagger, the done and waiting ones as two cards;
  3D Cascade's strips falling through their keyframes, 3D Fly In/Out's
  picture flying from or to the far point, 3D Shuffle's two pictures
  changing places, their paths keyframes through a spline,
  `spline.rs`'s `Spline` (Gradient Map's too) — the point light's strength along the transition and a flying
  picture's opacity, VEGAS's curves not known yet, plain bumps and ramps);
  and in `transitions/gl.rs` VEGAS's GL Transition: GLSL as the open GL
  Transitions collection writes it, the code typed in "Enter your GL
  transition code" (a crossfade at first) or one of the 62 presets' (the
  collection's code, `gl/presets.rs`, made by a script from the public
  sources, their uniforms renamed after the panel's controls; the code
  hidden for a preset, `IsPreset`); uniforms named after the controls
  (`kParamFloat0`–`3`, `kParamInt0`–`1`, `kParamBool0`, `kParamVec20`,
  `kParamIVec20`, `kParamVec40`–`41`) show them with their comments'
  defaults and slider ranges (`// = 0.5; [0,1]`) and the names their
  `…Name` settings hold (`show`); the pictures read as VEGAS's textures
  (bilinear, repeated), each pixel within 0–1; the authors and the MIT
  license in `crates/effects/licenses/gl-transitions.md`, shipped with the
  packages. On the processor BananaV's own GLSL interpreter runs it
  (`crates/effects/src/glsl`: a small preprocessor (macros with
  parameters too), the parser, type checking — arrays of constant sizes,
  structs of up to 16 numbers — and closures over a block of values,
  every variable a place of its own — GLSL has no recursion —, loops cut
  short when endless; sine and cosine by its own reduction and
  polynomials and `pow` of a negative as nothing, as a card works them
  out); on the card `glsl/wgsl.rs` translates it to a WGSL kernel (its own
  type inference over the same syntax; the harness `glt_sample`,
  `glt_trig`…), `Kernels::run_made` compiling it at run time after naga
  checks it (`accel::gl_transition_card`, `own_transition`); random
  numbers from a sine's far digits differ from the processor's (a card's
  fused multiply-adds), the rest within a few thousandths
  (`own_transitions_draw_alike_on_the_processor_and_on_the_card`);
  the Transitions window lists them in VEGAS's
  categories by their grouping (and GL Transitions), next to the
  built-in Cross Dissolve and Fade Through Black; the older built-in
  wipes stay for old projects;
  on the graphics card BananaV's own transitions run as kernels
  (`shaders/own_transitions.wgsl`, an entry point each, from
  `accel::transition_plan`'s `TransitionPlan` — `pixel::plan`, with the work around
  the kernel, `TransitionSteps`: Flash's light blurred, Cross Effect's
  cells averaged — `tr_cell_rows`, `tr_cells`, `tr_cell_fill` — or its
  pictures or mix blurred; or a table beside the two pictures,
  `TransitionSteps::Table`, in kernels of four buffers (`Sig::Quad`):
  `moves::plan`'s and `portals::plan`'s frame steps, `Drawing::table`,
  that `tr_moves` in `shaders/own_moves.wgsl` runs for each pixel, and
  `blinds::plan`'s
  openness and offsets for `tr_venetian_blinds` (`own_blinds.wgsl`), and
  `gradient_wipe::plan`'s steps and ramp for `tr_gradient_wipe`
  (`own_gradient_wipe.wgsl`; its picture reaches the plan as text,
  `accel::Value::Text`); the pages are `tr_pages` (`own_pages.wgsl`, the
  curl's numbers from `pages::plan`, each pixel working out its
  neighbours' layers too, its arcsine its own — the card's is rougher);
  the 3D ones are `tr_cards` (`own_cards.wgsl`: the scene `cards::plan`
  works out as a table, `TransitionSteps::Cards`, what is under the cards
  put in place first, then each pixel through every card);
  `own_transitions_draw_alike_on_the_processor_and_on_the_card`
  compares every preset), the GL ones (Custom's included) on the
  processor, the pictures going back and forth; `bv-engine --test
  transition_sheet -- --ignored` draws them all, the GL ones on a sheet
  of their own), more of VEGAS's
  video effects, `filters.rs` (Tiny Planet, Defocus, Linear, Quick and
  Radial Blur, Median, Min and Max, Convolution Kernel,
  Spherize, Swirl, Threshold, Wave, Pinch/Punch, Offset and Wrap, Black
  Restore, Broadcast Colors, Channel Blend, Chroma Blur, Mask Generator,
  Radial Pixelate and Deform, and in `looks.rs` the
  color, light and film ones — LAB Adjust, Soft Contrast,
  Fill Light, Lens Flare, Starburst, Glint, Light Rays, Film Effects,
  Film Grain, Newsprint, TV Simulator, Bump Map, Denoise and Looks
  (VEGAS's AutoLooks: its panel — none, then the looks, and Strength —
  and its engine: each look a 16 × 16 × 16 table read trilinearly,
  `look_table`, the straight colors moved toward it by Strength; the
  looks BananaV's own; on the card LUT
  Filter's kernel, `Plan::Lut`) —, and in `classics.rs`
  VEGAS's Chroma Keyer with its controls — it and the own plug-ins
  Gaussian Blur, Sharpen, Vignette, Mirror and Glow took the places of
  BananaV's older built-in ones (no longer in `EffectKind::VIDEO`, kept
  for the projects that have them) —: settings from VEGAS's
  Help, each reading its source once and working the picture out whole,
  `Kind::process` (the frame's time seeds those with noise). Their panels
  are VEGAS's — names, labels, groups (`Spec::Group`), ranges with a
  narrower slider (`Spec::Ranged`), plain pairs (`Spec::Pair`), defaults
  and factory presets — and so is their math where VEGAS's behavior is
  known: Black Restore (a pixel stays when any channel is past the
  threshold), Broadcast Colors (luma, chroma and composite held in IRE
  with soft knees), Channel Blend (premultiplied weights, Normalize
  counting the offset), Chroma Blur (BT.601 U and V through three boxes),
  Mask Generator, Median and Min and Max (one 8-bit weighted percentile
  over an area whose edge pixels weigh the range's fraction, or an
  ellipse's coverage; Min and Max is it at 0 or 1, `ranked`), Quick
  Blur's Blend amount, LAB Adjust (Lightness scales L, a and b moved by
  100 times Channel a and b), Convolution Kernel (Row 1 above the pixel,
  Rotation turning the outer weights a place each 45°, Auto normalize
  1 ⁄ sum, the alpha summed when the weights reach 1), Linear Blur
  (VEGAS's Gaussian along the angle, clockwise, worked out on the picture
  sheared along its rows below 45° and from 135°, its columns between —
  the slope times the pixels' shape —, by Amount × the frame's longer
  side times how far the line runs along them (up and down times the
  pixels' shape), across them by VEGAS's least, a radius of 0,
  `line_blurred`), Radial Blur (each pixel mixed
  with the blurred picture a step nearer Center, outward quarter by
  quarter, `radially_blurred`), Defocus (`Defocus`: VEGAS's iris — a
  disc or a polygon of 5 to 8 corners, the first a side's turn short of
  the top, of 0.95 × Radius percent of the longer side whole pixels, its
  light from middle to rim VEGAS's gradient `200 (d (1 − t³) + t³)` of
  255, `d` from Disc curvature — below 0 a brighter rim, as VEGAS's
  "Visible Outer Ring" shows —, the picture smoothed by the binomial
  3 × 3 with Reduce flicker and its colors past Bloom threshold
  brightened toward Bloom strength, convolved through `fft.rs`, held
  within 0–1; `Plan::Defocus`), Soft Contrast (`SoftContrast`, `looks.rs`:
  VEGAS's panel — its units, the Vignette group — and three presets; its
  math BananaV's: the
  luma's curve, the tint, Diffusion's glow — blurred as VEGAS blurs,
  `recursive_gauss` — screened in, the vignette's outside blurred (across
  and down), black, white or transparent;
  `Plan::SoftContrast`, `kit_soft_tone`, `kit_soft_diffuse`,
  `kit_soft_vignette`), Rays (`Beams`, `looks.rs`: VEGAS's panel and two
  presets, its math BananaV's as Soft Contrast's — the lights, the
  picture's brightness or alpha past Threshold, streaming away from
  Position for Amount of the way, Quality readings each, brightened by
  Boost up to Peak, uneven by Noise, colored by Hue along Hue sweep —
  and VEGAS's Mask group, `effect_mask.rs`, also Glint's: a rectangle,
  bar, wipe or ellipse on the effect, its lights or shown alone, the
  card's `vm_mask` in the shaders' prelude; `Plan::Rays`,
  `kit_rays_lights`, `kit_rays`), Glint (`Glint`, `looks.rs`: VEGAS's
  panel — its units, Streaks, the Mask group — and two presets; its math
  BananaV's: the highlights past Threshold brightened by Boost, streaked
  along Streaks lines — a kernel of colored rays, gray toward Hue turning
  by Hue sweep at Saturation, `Glint::kernel` — convolved through
  `fft.rs` with kernels of two complex numbers a cell, `convolve_lanes`:
  the light in both real parts comes out red and green, blue and alpha;
  `Plan::Glint`, `kit_glint_lights`, `fft_add` through the mask),
  Starburst (`Starburst`, `looks.rs`:
  VEGAS's star — Points rays `2 × trunc(0.586 × radius)` pixels long, the
  first at Orientation plus 270°, slivers with their light 64 of 255 at
  the middle fading to the tip; with Center burst eight rays of set
  lengths a fifth as long and 27 short ones of set lengths mixed in;
  Aspect stretching it across — convolved through `fft.rs` with the
  lights: the picture faded near the edges by Soften edges (ramps a
  blur's reach wide, `edge_mask`), smoothed with Reduce flicker, each
  channel under Threshold gone, brightened past it toward Brilliance;
  Strength of the result added to the picture, or alone with Effect
  only; `Plan::Starburst`, the card's `fft_add`), Offset and Wrap (scaled about the middle,
  moved in frames, repeated by default), Spherize (`spherized`), Radial
  Pixelate (the picture laid out by angle and distance, averaged in
  blocks, laid back; `radially_pixelated`), Tiny Planet (an inverse
  stereographic projection, `planet`), Chroma Keyer (`classics.rs`: an
  RGB and HSL distance from an RGB key through the thresholds, the mask
  blurred by a Gaussian 1⁄255 at its radius, blue or green spill held
  under the next channel, the mask shown white), Bump Map (the bump
  channel's 3 × 3 differences as the surface, a directional, spot or omni
  light at a 3D Source, `Spec::Triple`, VEGAS's diffuse and specular
  sum; the compositor's too) and Fill Light (each pixel's BT.601 luma
  in 8-bit steps picks its light from a table — a bump 0.3 high at
  Bias's tone, gone from 85 % up, times the light's color: gray turned
  by Tint toward red turned by Hue about the gray axis, its luma kept —,
  blurred by VEGAS's Gaussian of radius 0.002 × Softness × the frame's
  longer side as the pixels show it, the larger pixels' way shrunk by
  their shape, half of Strength of it added, held under white),
  Film Grain (`Grain`: three tables of 64 × 64 normal numbers from
  seeded random numbers — `r250.rs`, Portals' too — through the polar
  Box–Muller, spread over 0–1, walked a value every (2 × Granularity +
  1) 240ths of the frame's height as VEGAS steps it and read between
  values, the same table for every color or, by Chromaticity, one each;
  each color times 1.3 its grain, Amount of the way, alpha kept) and
  Light Rays (`Rays`, `rays_drawn`: VEGAS's 8-bit math on the frame's
  8-bit values, rows from the top — from the light out quarter by
  quarter, each pixel mixed by the rays' weight, Strength to the power
  of 75 over the shorter side, with the average of up to three readings
  a pixel apart toward the light of what is drawn already (in double
  precision: steps held at 0.99, thirds summed as doubles), less where the
  ray is darker than the pixel by Sensitivity, the pixel first faded by
  the bound's ellipse and grained by a 16-bit generator, then the
  whole picture tinted and the source blended back; Radius X and Y and
  Feather greyed while Bound radius is off, `filters::show`) and Film
  Effects (`Film`: its gray as VEGAS's renders show it — the channels'
  average —, its grain — one random number a pixel, half of Grain each
  way, on straight colors, the processor's hash `film_hash` the card's too —
  and its tint toward Color, before the particles; the particles — dust,
  flicker, hairs, jitter, scratches, one Type at a time — BananaV's own,
  the same for every render of a frame, `Mark`, VEGAS's not being known
  yet; Type and Amount greyed while Enable
  is off) and TV
  Simulator (`Tv`: VEGAS's layers — the picture rolled down by (1 ⁄
  Vertical sync − 1) × the detail zoom frame heights a second, its rows
  torn across by five waves while Line sync is under 1, the aperture
  grill's stripes mixed in by 0.75 × the larger of Aperture grill and
  Static as VEGAS mixes them (`w ⁄ 4 + 1.99 c w`), the static over the
  stripes, the scanlines by Interlacing and the scan phasing band rolling
  down, held within 0 and the alpha, then the phosphors' powers, blue
  lifted most; the grill's, the scanlines' and the band's profiles
  BananaV's own),
  Newsprint (`Newsprint`, VEGAS's halftone: cells (the longer side × Dot
  size) ⁄ 20 + 3 pixels, across divided by the pixels' shape; the picture
  averaged over them, `averaged`; a dot a cell in brick rows, its radii
  1.25 × half the cell × its darkness — BT.601's luma of the averaged
  picture read between cells —, black, the cell's color or three inks a
  quarter radius apart; white paper, each dot's ink taken from it by how
  much of each pixel it covers, the rim table `dot_edges`; under Dot
  size 0.15 the source, gray for black dots, mixed back) and Lens Flare
  (`flare_spots`: VEGAS's five lenses' elements — shape, intensity,
  size, place along the line from the light through the frame's middle
  and color, `LENS_35MM` and the others — each at the light's place
  times one less its place, 2^((place − 1) × sin(−Perspective × 90°)) ×
  Size × its size × the shorter side big, its color the lens's toward
  Light color by Tint, times its intensity and Intensity; the elements'
  light added to the straight colors by Blend; the shapes' profiles —
  disc, ring, halo, the diaphragm's ring and glow, the star's rays —
  BananaV's, VEGAS's not being known yet);
  Threshold (each of red, green and blue against the threshold's 8-bit
  step, `threshold_step`) and Wave (each row's move across and each
  column's down in 16,384ths of a pixel, rows from the top, π taken as
  3.141, held off the last row and column, `wave_offsets`), Swirl and
  Pinch/Punch (`lensed`: about the center pixel, in an area of
  Horizontal × Vertical half frames, turned by 15 × Amount × (1 − r)²
  radians — read weighted by alpha — or each offset times 1 − (cos πr +
  1) × Amount × 0.4; rows from the top, `Lens`, `place_step`) are
  VEGAS's, and Deform (`deformed`, `Warp`: the top's and bottom's
  curves bend the rows, the left's and right's the columns, the squeezes
  scale the height and width from edge to edge, the shears slant it, about
  the middle; in the deformed picture's box — moved by Center image so
  the middle lands on the frame's — each pixel taken back as VEGAS undoes
  them and read between pixels, the middle of VEGAS's readings by
  render quality (the nearest pixel and a footprint's average await a
  render through VEGAS), the
  background past the picture) too. Effects redone so are version 2.0, their
  older values reset (`PluginFx::upgrade`'s `REDONE`; an older Quick
  Blur keeps blurring all the way). On the graphics card
  `Kind::card_plan` gives their kernels the same numbers the processor
  uses (shared helpers: `Broadcast`, `blend_weights`, `mask_levels`,
  `sphere`, `Lens`, `wrap_modes`, `kernel_weights`, `BumpLight`, `Key`,
  `LineShear`): `accel::Plan::Pixel` (Threshold, Black Restore, Channel
  Blend, Mask Generator, Broadcast Colors, LAB Adjust — `kit_*` in
  `own_inplace.wgsl`), `Plan::Wave` (`kit_wave`: the moves per row and
  column worked out once, `accel::wave_offsets`), `Plan::Lens`
  (`kit_swirl`, `kit_pinch_punch` in `own_triple.wgsl`: each column's
  and row's offset and its square from `accel::lens`, the same squares
  telling the area on both), `Plan::Deform` (`kit_deform` in
  `own_pair.wgsl`: the shape's numbers and box from `accel::deform`, a
  point a hair before the first column or row counting as in, `EDGE`),
  `Plan::Remap`
  (Spherize, Offset and Wrap, Tiny Planet — its sine and cosine rational, its
  latitude by atan2, exact near the poles —, Convolution Kernel, Quick
  Blur and Bump Map's filter — `kit_*` in `own_pair.wgsl`, the plug-ins'
  pixels with rows up, `kit_at`, `kit_bilinear`, `kit_sample`),
  `Plan::ChromaKey` and `Plan::ChromaBlur` (a pass, the processor's three
  boxes each way — `Compositor::boxes`, `accel::gauss_boxes` —, a pass
  back), `Plan::LinearBlur` (`kit_shear`, boxes along the rows,
  `kit_unshear`), `Plan::RadialPixelate` (`kit_polar`, its blocks'
  means, `kit_polar_back`), `Plan::RadialBlur` (`kit_radial_blur`: the
  processor's recursion quarter by quarter, a diagonal of each at a time
  — a pixel needs only nearer ones — by one group of threads,
  `storageBarrier` between diagonals) and `Plan::Ranked` (Median and Min
  and Max: `kit_median` in `own_triple.wgsl`, a group of threads a row,
  `Grid::Groups`, the area's 8-bit histograms in the group's memory slid
  along it, the table of weights and their changes from
  `ranked_table`); `Plan::FillLight` (`kit_fill_map` through the
  table, the blur's taps, `kit_fill_apply`); `Plan::FilmGrain`
  (`kit_film_grain`: the tables and each column's and row's places from
  `accel::film_grain_table`); `Plan::FilmEffects` (`kit_film_effects`,
  the frame's marks from `accel::film_effects_card`); `Plan::TvSimulator`
  (`kit_tv_simulator`, each row's move from `accel::tv_simulator_card`); `Plan::Newsprint`
  (`kit_newsprint_cells` averages the cells into the table after the rims'
  rows from `accel::newsprint_card`, then `kit_newsprint` draws each pixel's
  nearby dots — a rim pixel may land a table step apart); `Plan::LensFlare`
  (`kit_lens_flare`: each pixel through the elements `accel::lens_flare_card`
  lays out, two rows of a table each); `Plan::LightRays` (`kit_light_rays` in
  `own_triple.wgsl`, Radial Blur's wavefronts with the processor's integer math
  exactly: each reading's whole pixels and 256ths worked out with 64-bit
  integer comparisons instead of square roots, the doubles' thirds
  rounded as doubles round, `lr_round`, the noise's generator jumped
  ahead to each pixel; the bound's rows from `accel::light_rays_card`, so
  only its feather is worked out in floats; then `kit_light_rays_finish`;
  every thread the same rounds around each `storageBarrier`, as Direct3D's
  old compiler demands — a kernel it refuses keeps the card from
  starting, and the parity tests then pass without it: `bv-gpu --test
  parity every_card` compiles every kernel on the card); `Plan::Same`
  where the settings change nothing. The
  others still go through the processor until they have kernels;
  `bv-engine --test filter_sheet -- --ignored` draws them all) — both
  on `kit.rs`, what such tables of settings share (`Spec`, `define`,
  `Vals`, `Grid`, `Buf`; pictures worked out on the host's threads,
  `Buf::rows`, `bands` and `for_chunks`, float RGBA rows read and written
  whole, `colors::write_out`) —, light enough for the preview: long blurs
  worked out smaller and enlarged again (the streaks' `Soft` within 480
  pixels), kernels of any size through the frequency domain (`fft.rs`:
  the picture, its edges held, and the kernel on grids of powers of two,
  two complex numbers a value — red and green, blue and alpha —,
  Stockham's radix-2 steps; on the card `fft_image`, `fft_kernel`,
  `fft_pass`, `fft_multiply`, `fft_crop` and `fft_add`,
  `Compositor::fft_convolve`: Defocus's iris, Starburst's star), box
  blurs by running
  sums down bands of rows, Radial Blur by halvings (`radial`), Denoise
  as VEGAS's 8-bit processor (BT.601 planes in 8-bit steps, each cleaned
  by non-local means — 21 × 21 search, 7 × 7 patches, their mean
  absolute difference rounded, weights exp(−max(d − 2h², 0)/h²) — each
  move's differences summed across then down on the processor; on the
  card `kit_denoise_planes`, `kit_denoise_nlm` keeping a workgroup's
  42 × 42 pixels in shared memory, `kit_denoise_rgb`, and Sharpening's
  3 × 3 kernel `kit_denoise_sharpen`; about 60 ms a 1080p frame);
  `bv-engine`'s `own_effects_speed` (`--lib
  -- --ignored --nocapture`; `BV_SPEED_CARD`: on the card) times each at
  1080p —, VEGAS's media generators, `generators.rs`
  (Checkerboard, Color Gradient, Noise Texture, Solid Color and Test
  Pattern, on
  `kit.rs` too: each pixel from its place in the frame, `Kind::painter`;
  Checkerboard's tiles and edge blends are VEGAS's, in the frame's
  pixels, Test Pattern has VEGAS's nine patterns — the bars as SMPTE
  and EBU lay them out, VEGAS's gradient and ramp, its "porches" awaiting
  the user's prints — and Noise Texture is VEGAS's (`Noise`: its panel's
  groups, seven styles and twenty presets; Perlin's improved noise in
  layers, each twice as fine, weighed by Grain to the layer's power — the
  last a part by Number of layers, those over 0.01 —, VEGAS's styles
  — fractal smooth or turbulent, veins of marble and lines, rings — and
  colors' line in 65,536ths, premultiplied; its lattice's gradients hashed,
  BananaV's own) and
  Solid Color is VEGAS's (its Color, transparent at first, and ten
  presets, premultiplied; the Media Generator's Solid Color makes it,
  the older built-in one stays for projects and BananaV's own uses); on
  the card these four are drawn by kernels in the
  media's place (`Compositor::own_generator` in `media_picture`:
  `bv_media::plugins::generator_values` at the media's frame, the
  processor's reduced size from `covering`, `accel::generator_plan`,
  `Checker`'s numbers, `kit_checkerboard`, `kit_test_pattern`,
  `kit_noise_texture`, Solid Color the card's `fill`;
  `own_generators_draw_alike_on_the_processor_and_on_the_card`);
  the older built-in `Generator`s stay for the projects that have them)
  and Credit Roll, `credit_roll.rs` (VEGAS's text format — a title begins
  with a space, a tab parts two columns, `//` a comment —, a style each
  for titles, items and two columns (each with Kerning, off as VEGAS's
  Styles tab shows it), scrolling over its event's length or
  a timed sequence; its lines drawn by Titles & Text's `picture`; its
  Position group's box — the frame, the credits' part of it, the margins'
  yellow handles — a group's own interface, `position_interface`; the
  credits imported from and exported to text files (UTF-8, UTF-16 or
  Windows' code page read), the other settings to properties files
  (XML), through file parameters read or written as soon as one is
  picked, `file_picked`, the path then back to empty; a file to write —
  OpenFX's `kOfxParamPropStringFilePathExists` off,
  `ParamInfo::file_to_write` — gets a dialog to save, and a plug-in's
  persistent message shows over its settings, an own plug-in's
  translated; generator plug-ins hear their event's length as OpenFX's effect
  duration: the pool's `Ask::span`, `VideoReader::set_span`,
  `Instance::set_duration`; `bv-engine --test generator_sheet --
  --ignored` draws them all), Flicker Control, `flicker_control.rs`
  (VEGAS's panel, show rules, messages, presets and math. Temporal blend:
  OpenFX's temporal clip access, the analysis window's frames the clip
  has — through the effects before it, the engine's `Temporal` source —
  weighted by VEGAS's binomials (each factor a whole-number division)
  drawn by Sensitivity toward the frame or an even mean, each also by
  how much Type gives its largest channel; with motion compensation read
  along the homography between the frames' keyed corners. Histogram
  smoothing: each channel matched to the harmonic mean of the window's
  cumulative histograms, which Calculate counts through the host's
  analysis (`kBananaVParamPropAnalysis` "ColorHistograms", a hidden
  string parameter; Reset clears it). Addition: a sine of Flicker
  Frequency, or a random flicker drawn from the frame number. Show
  mask only: the change, a fifth white. On the card: `Plan::FlickerAdd`,
  `Plan::FlickerMatch` and, through `accel::temporal_plan`,
  `Plan::FlickerBlend` — `kit_flicker_gather` a frame at a time into
  sums, `kit_flicker_finish`;
  `flicker_control_draws_alike_on_the_processor_and_on_the_card`),
  Lens Correction, `lens_correction.rs` (VEGAS's panel, rules and math:
  each pixel reads the picture where the PanoTools model sends it — `a·ρ³
  + b·ρ² + c·ρ + d` of the distance from the pixel Distortion Center falls
  in (truncated; left of the picture, its middle), ρ 1 at half the shorter
  side, `d` keeping that radius, over Scale — or the hidden Model's radial
  one (the picture's parts, Scale unused), in single precision as VEGAS
  works them out, between the four pixels around weighed in truncated
  1024ths (right of the last column the next row's first pixel), nothing
  from 0.4 of a pixel before the first column or row and 0.6 before the
  end; Lens Model's Search for lists the lenses whose names have its
  words — Found Lenses' options set at run time (`Props::reset`, `count`)
  —, and a lens picked gives k1–k3 its coefficients and Distortion Center
  (0.5, 0), as VEGAS's does; the lenses an adaptation of lensfun's
  database, `lens_correction/lenses.rs` (636 of VEGAS's 639 — its three
  action cameras of its own await prints —, CC BY-SA 4.0,
  `crates/effects/licenses/lensfun.md`, shipped with the packages); Show
  Distortion leaves the picture as it is and draws VEGAS's grid over the
  preview — a 192 × 108 picture's 16 × 9 pixels through the PanoTools
  model times Scale, cardinal splines —; on the card
  `Plan::LensCorrection`, `kit_lens_correction`, its divisions done on
  the processor, `lens_correction_numbers`, so that the truncated weights
  land alike but where a fused product falls a hair across a whole
  1024th, at an edge),
  Mesh Warp, `mesh_warp.rs` (VEGAS's panel, rules and math: an 8 × 8
  grid of points dragged on the preview, each point where the grid at
  rest's goes — the hidden Mesh point refers to Target: every cell split
  in two triangles along its shorter diagonal, the other one when those
  overlap, drawn on the picture halved until its shorter side is under
  480 pixels and the map enlarged; or Source, each pixel's cell's four
  points mixed — the picture read as Lens Correction reads it
  (`lens_correction::read`); a dragged point stays inside the ring of its
  eight neighbours (0.2 past the edge where it has none; a tenth of the
  way when the pen leaves it), Elasticity pulls the others by a Gaussian
  of their distance (spread a fifth of it), with Shift only its
  neighbours — the overlays hear the keys held through BananaV's
  `kBananaVInteractPropModifiers` (`bv_ofx::interact::set_modifiers`) —,
  Fixed Border Points keeps the border's points, a double click puts a
  point back; the grid's sizes change only while the mesh is at rest and
  not animated; the mesh is a custom parameter, each point's place as
  text the plug-in interpolates between keyframes through OpenFX's
  custom interpolation (point by point when the grids match, else the
  second from half way) — custom parameters animate as a whole, as
  curves do: the FX window's Animate, keyframes in the controller's
  effect row, `ofx::custom_keys` asking the instance for their values
  in between —; on the card `Plan::MeshWarp`, `kit_mesh_warp`, the map's
  table worked out on the processor and kept while the mesh stays,
  `mesh_warp_table`), Bézier Masking, `bezier_masking.rs` (VEGAS's panel and
  math: five masks — an oval, a rectangle (at first), a diamond or a
  curve of Bézier anchors drawn on the preview in Edit Mode, masking
  nothing meanwhile, a custom parameter interpolated between keyframes —
  placed by Width, Height, Angle and Location with the preview's handles
  (its labels numbered, "Width 1"), each weighed by its Opacity and
  softened by its Feather — percent of the frame's shorter side, In, Out
  or Both ways —, the masks added up; the shapes filled by scanlines,
  the feather by the exact distance to the outline, measured only near
  it (`shape_coverage`, `softening`); Invert, Blend — what the masks
  leave out coming back —, and Mask FX: the effects
  before it show only inside the masks, the rest the chain's input on
  its optional "Original" clip, which BananaV's host fills,
  `bv_engine::plugins::ORIGINAL` — an event's or a media's picture now
  without that chain's effects, through the temporal source; a track's or
  the video output's, the picture its chain started from, which both
  renderers keep when a plug-in takes it (`PluginEffects::takes_chain_input`,
  `apply_in_chain`) —; on the card the `bezier_masking` kernel
  (`accel::Plan::Masks`, the same exact distance), Mask FX over the
  chain's input the card keeps — an event's picture before its effects
  too, `accel::takes_start`, `Compositor::own_effect_from` — through
  `bezier_mask_fx`; version 2.0 converted older settings, VEGAS's
  option orders and Blend's meaning; each mask's
  Tracking › Start has the host follow the mask's box from the cursor —
  BananaV's `kBananaVParamPropMaskTracking`, `mask_tracking.rs` on the
  engine's tracker — and keyframe its location, angle and size as Mode
  says), Color Match, `color_match.rs` (VEGAS's: its panel — Strength,
  Match brightness, the Source, Target and Result images —, its seven
  presets — source images' means and spreads — and its measure, every
  pixel's 8-bit values in L*a*b* (LAB Adjust's), their running means and
  spreads over n − 1; each pixel's a and b — L too with Match brightness
  — moved from the target's means by the ratio of the spreads to the
  source's, in single precision, Strength of the way, transparent pixels
  gone and opaque ones kept opaque, `Transfer`; nothing while an image is
  missing or a solid color; VEGAS's messages, `advice`: a solid color, too
  little variation, images far apart, the target still to take; the
  images kept as summaries with a mosaic drawn under their rows, the
  Result's the target's matched; OpenFX's General context, the images on
  optional clips; on the card `kit_color_match`), the optical flow effects, `optical_flow.rs` (Slow
  Motion: VEGAS's panel in its real-time mode — the Parameter
  group (Quality, Blending, Playback Rate), Visualization, Adjust Event
  Length — the source frame its Playback Rate, summed frame by frame,
  reaches, the frames between made as VEGAS makes them
  (`made_between`: along `bv_track::flow`'s dense flow back from the later
  frame — Lucas–Kanade at every pixel over box windows, coarse to fine —,
  the earlier read that far along the move, the later the rest of it
  back, mixed, or the nearer of the two moved with Nearest Frame, a
  transparent side giving way to the other), Show Optical Flow Field, and
  Adjust Event Length through
  BananaV's `kBananaVParamPropEventLength`, `flow_ui.rs`; Motion Blur:
  VEGAS's panel and math, `motion_blur.rs` — the frame's motion to the
  next (the last frame's from the one before), the pixels moving within
  Motion Threshold's range the mask, their mean direction picking an
  upright, level or diagonal line of ones through a square kernel, turned
  the rest of the way by an affine warp (in 32nds of a
  pixel), its reach Blur Amount thousandths of the width or, adaptive,
  the mask's mean motion times Blur Amount up to the maximum; the picture
  and the mask convolved with it through `fft.rs`, reflected past the
  edges (`Edges::Reflected`), the blurred picture
  where the spread mask is not 0 in 8 bits —, with its three
  visualizations; on the card through `accel::temporal_plan`: the
  compositor hands it the frame's picture as the card has it and the
  host's input at the other frame (`PluginEffects::input_at`, the
  engine's `Temporal` source), the mask and kernel worked out as the
  plug-in does, then `fft_convolve` twice and `kit_motion_blur_pick`
  (`motion_blur_draws_alike_on_the_processor_and_on_the_card`); the Warp
  Flow transition — VEGAS's Method and Blending, its frames between as
  Slow Motion's —: the first picture flowing into the second, which Edit ›
  Smart Split puts between the video parts it leaves
  (`Project::smart_split`'s `video`, `bv_effects::WARP_FLOW_ID`); temporal
  clip access for the first two, the last flow kept for the next frame; on
  the card Slow Motion through `accel::temporal_plan` — its playback rate
  summed through the host's `PluginEffects::numbers_at`, the source frames
  from `input_at`, `Plan::Frame` or `Plan::FlowBetween` — and Warp Flow
  from the pictures the card has, the flow worked out on the processor,
  the frame between drawn by `kit_flow_between` (`own_flow.wgsl`;
  `flow_effects_draw_alike_on_the_processor_and_on_the_card`)), Dual Fish Eye Stitching, `stitching.rs` (VEGAS's
  panel, five cameras' presets and math, in single precision: each pixel
  of the equirectangular picture a direction on the sphere, the frame's
  middle along the front lens's axis; the back lens's circle on the left
  half, the front's on the right, each an equidistant fish-eye — the
  angle off its axis over half Field of view, less Center's part, times
  Radius, turned by Rotation (hundredths of a degree) and the lens's
  orientation (VEGAS's -90 Degrees a quarter clockwise) — read between
  four pixels, edges held, from the frame's top, at most half as high as
  wide; past 180° both lenses see the sides, mixed along the overlap in
  Y′UV; Remove vignetting raises each channel to a power falling from 1
  by Intensity over the outer Range of each circle; opaque; on the card
  `Plan::Stitch`, `kit_stitch` with each column's and row's cosine and
  sine from `stitch_table`), 360° Video Stabilization, `stabilization_360.rs` (VEGAS's
  panel — Stereoscopy, Analyze and Clear Motion Data, Exclude Views,
  Smoothing a window of frames —: the sphere turned back against the
  camera's turns, which the host finds as rotation vectors —
  `kBananaVParamPropAnalysis` "Rotation", the engine's
  `motion_analysis.rs` fitting `bv_track::follow_features`' directions —
  over one eye of a stereoscopic picture, the poles left out as Exclude
  Views says (a hidden choice in the host's order, `excluded`), less
  their smoothing by Video Stabilization's Blackman window; the sphere's
  turn, `turned`, Scene Rotation's too: each output pixel's direction
  read where the turn sends it, bilinearly, edges held; on the card
  `Plan::SphereTurn`, `kit_sphere_turn` with each column's and row's
  sines and cosines worked out on the processor, `sphere_table`, the
  latitude by `atan2`), Scene Rotation, `scene_rotation.rs` (VEGAS's: a
  360° picture turned about x, then y — its angle the other way —, then
  z, `(Rx · Ry) · Rz`, each eye apart as Stereoscopy says),
  Video Stabilization, `stabilization.rs` (VEGAS's panel, rules and
  math: User View's Basic, Professional and Expert controls, which
  groups show and which grey out before and after an analysis — Stabilize
  and Reset in Basic, Analyze Motion and Clear Motion Data in the others,
  VEGAS's message until something is analyzed —; Analyze Motion asks the
  host, through BananaV's `kBananaVParamPropAnalysis`, for the picture's
  motion from each frame to the next — the engine's `motion_analysis.rs`
  draws the event or the media alone and follows it with `bv-track`
  (`track_tuned`) over the Expert view's region of interest, grid of
  cells, tracker and resolution (`kBananaVParamPropAnalysisRegion`,
  `…Grid`, `…Follow` — `bv_track::Follow`: Lucas–Kanade, the least sum of
  absolute differences or both — and `…Reduction`, `MotionSetup`),
  each move's affine part kept (`Step::linear`, `bvmotion 2`), the
  interface's `stabilize_ui.rs` keeping the result in the hidden Motion
  Data —; each move's six numbers (shift, angle, scales, shear — as many
  as Motion Compensation keeps) smoothed by a Blackman window Smoothing
  seconds long, its ends mirrored, in single precision, or held at
  nothing to freeze the Reference Frame Number's view (set after an
  analysis to the frame needing the least zoom), the moves composed from
  the reference into each frame's correction, the Smoothing Parameters'
  unticked components left as filmed; Auto Zoom the least zoom keeping
  the view inside the corrected frames where a correction is at its most
  — where their edges cross the frame's diagonals — a hundredth up and
  one more, written into Additional Zoom, which turns Auto Zoom off when
  set otherwise; the additional zoom, turn and shift put together with
  the correction as VEGAS puts them (`matrix`); the picture read between
  four pixels weighed in truncated 1024ths, edges held — transparent
  past 0.4 of a pixel before the first column or row and 0.6 before the
  end, or mirrored, repeated or held as Edge Filling says —; Add Motion
  Blur a box along the rows `smoothed pan ⁄ Threshold` each way, its ends
  mirrored; Expert's region of interest drawn and dragged on the preview
  over the analysis grid (VEGAS's colors and handles), and Show Points the
  frame unstabilized under the features the analysis followed —
  `bv_engine::FeatureTracks`, kept for the session in
  `BananaApp::stabilization_points` and drawn by `ofx.rs`'
  `stabilization_points` as VEGAS's green rings and lines —; on the card
  `Plan::Stabilize`, `kit_stabilize` with the map's whole pixels apart
  from its fractions, `stabilize_numbers`, and `kit_stabilize_blur`),
  Stabilize, `stabilize.rs` (VEGAS's older stabilizer: its panel — Pan
  smoothing, Stabilization amount, Rolling shutter correction, Apply —,
  its three presets and its rule: the values last applied kept apart
  (`current…`), so a change turns Apply on with VEGAS's message until it
  is applied again; Apply asks the host for the motion as Video
  Stabilization's buttons do. Its math is BananaV's, on Video
  Stabilization's: the moves smoothed over 0.2 to 5
  seconds as Pan smoothing goes, each frame's correction held within the
  zoom Stabilization amount allows (a quarter of the frame at most), the
  view zoomed as far as needed within it, and Rolling shutter correction
  each row put back where a sensor read top to bottom over 0.9 of a
  frame left it — a shear and stretch by the camera's speed —; on the
  card Video Stabilization's `Plan::Stabilize`), Stereoscopic 3D Adjust,
  `stereo_adjust.rs` (VEGAS's panel, rule — Crop greyed while Automatic
  crop is on, set only when it changes, as VEGAS's — and math: each eye's
  map of the picture to the frame, rows up, every correction halved and
  given to the eyes in opposite senses — the move across and up, the zoom
  about the middle, a keystone trapezoid through `perspective`, a turn of
  Rotation × π ⁄ 365, each eye's flips —; Crop or Automatic crop zooms
  both in about the points their maps take to the middle, the automatic
  one from the crossings of the lines to the frame's corners with the
  eyes' pictures' edges (`crossing`: integer geometry's rounding, half a
  pixel off, so it zooms in a hair even at rest) times 1 + 1.4 |Keystone|;
  drawn through an affine warp (no Keystone nor Rotation) or a
  perspective one, `reads`: each pixel read in 32nds of a
  pixel, between four pixels, nothing past the picture; then the
  eye's floating window, opaque black in 16,384ths of a pixel, `band`.
  BananaV's projects are not stereoscopic yet: the picture is the left
  eye's, as VEGAS draws a 2D project, and Auto correct — the picture at
  the cursor as both eyes, gray, 1080 lines, equalized, features followed
  by `bv_track` from one to the other, the vertical offset, zoom, keystone
  and rotation that line them up in 360-pixel cells found by Nelder and
  Mead's simplex where VEGAS's swarm is random — finds nothing to correct;
  fewer than 30 pairs, VEGAS's message; on the card `Plan::Stereo`,
  `kit_stereo_warp` reading `stereo_card`'s 32nds — an affine map's per
  column and row, a perspective one's per pixel, kept while the map
  stays), Smart Zoom, `smart_zoom.rs` (VEGAS's panel — Cutout's point
  and zoom 1–4, Noise sensitivity, Enable enhancement, the hidden Alpha
  mod, External alpha and Enhancement level — and its zoom: a window of
  1 ⁄ zoom of the frame around the point brought to the frame's size by
  a separable Lanczos of four lobes, eight taps a pixel, rows first,
  edges held; its enhancement, BananaV's,
  lifts the luma's detail over its binomial 3 × 3 by the level where it
  stands out of the noise allowed; the point and window dragged on the
  preview; on the card `Plan::SmartZoom`, `kit_lanczos_rows` and
  `kit_lanczos_columns` from `smart_zoom_table`'s taps, then
  `kit_smart_enhance`), AI Smoothen, AI Sharpen and AI Dehaze,
  `ai_guided.rs` (VEGAS's panels — Details, Amount, AI Dehaze's Artifact
  Reduction group: Border, Gain, Offset — in the AI/ML category, and
  the frame BananaV puts around the work: the picture framed by a reflected
  margin of Border (0.25 without the group) of its shorter side, taken
  as `(x + 0.1 Offset) Gain`, a bilinear small copy of it
  at the size Details picks, what is worked out there taken back to the
  full picture by a guided filter's layer — each channel's local linear
  map over 3 × 3, ε 10⁻⁴, averaged again and enlarged corners aligned —,
  then Amount of the way from the picture, held within 0 and its alpha;
  where trained networks would work, BananaV works the small
  copy out itself, a binomial 5 × 5, its difference from a binomial
  3 × 3 twice over, or the dark channel's dehazing — the air light the
  mean color where the 3 × 3 dark channel is within 1 % of its brightest,
  the transmission 1 − 0.95 × the dark channel over it, held at 0.1 —; on
  the card `Plan::Guided`, `kit_guided_low`, `kit_guided_op` (Dehaze's
  `kit_dehaze_dark`, `kit_dehaze_air` — one group's reduction —,
  `kit_dehaze_op`), `kit_guided_coef`, `kit_guided_mean` and
  `kit_guided_apply`), and
  White Balance,
  `white_balance.rs` (VEGAS's math, measured on its pictures: the
  neutral color read as ACES AP0 RGB gives the temperature and tint as
  Adobe's DNG SDK does; the picture moves in Y′UV as the Color
  Corrector's, U and V against the temperature and tint's color by
  Amount² ⁄ 2 and Y scaled by Brightness, the amount set by itself, as
  VEGAS sets it, to what takes the light's color to gray whenever the
  light changes; Equalize Histogram stretches each channel over the
  frame's own range, then holds red and green under blue — its regions
  of interest ask for the whole picture, and the card works the levels
  out row by row, then for the frame); the
  color effects' pixels and render loop are
  `colors.rs` (`render_at` tells each pixel's place); their factory
  presets are `bv_effects::presets`, listed with the user's — OpenFX has
  none). The engine registers them as built-in
  plug-ins (`bv_engine::plugins::register_own`, `is_own`); the interface
  lists them with BananaV's effects (Video FX's categories by their
  grouping, `BananaV/Utility`) and shows their texts in its language
  (`tr` of their labels, options and hints: add them to the catalogs).
  `apps/ofx-bundle` builds the same code as a binary any OFX host loads
  (`crates/effects/tests/bundle.rs` loads it as one; with `BV_NATRON`
  naming Natron's `bin`, `cargo test -p bv-effects --test natron --
  --ignored` renders with it in Natron, another program's host — check
  each new effect there too; `BV_EFFECTS_TRACE=1` prints what a host gives
  them). They follow the specification, not BananaV's host's leniency
  (hosts launch exactly the threads asked for). New effects from the
  user's list (`docs/ROADMAP.md`) go there, their settings and on-screen
  controls described through OFX, never special-cased in the interface
  (a handle on the preview — Cookie Cutter's center, Layer
  Dimensionality's light — is `ofx::Draw::handle` and `near_handle`).
  As VEGAS's standard overlay, the host shows each position (a
  two-dimensional double of a position type) of a plug-in that draws
  nothing of its own as a handle on the preview, dragged to move it
  (`ofx.rs`' `position_handles`: Spherize's, Swirl's and the
  transitions' centers, Rays' position); points counted otherwise get
  their plug-in's handles (`filters.rs`' `Kind::handles` and `Place`:
  Lens Flare's light, Offset and Wrap's offset, Bump Map's light; Lens
  Correction's Distortion Center).
  Positions count the frame 0–1 (`kOfxParamDoubleTypeNormalisedXYAbsolute`,
  as VEGAS shows and stores them: Crop's Location, Cookie Cutter's
  Center), so presets and the chooser's previews hold at any frame size;
  values an older version saved are brought up to date when a project
  opens (`PluginFx::upgrade`: Crop's Location counted pixels before 2.0).
  An event's effect counts its time from the event's start; the frame's
  time on the ruler (its offset added) reaches plug-ins as
  `kBananaVImageEffectPropTimelineTime` among a render's arguments
  (`PluginCall::timeline`, `RenderRequest::timeline`).
  Where VEGAS's pictures show how far or how soft something is, measure
  it on them (a fit of the user's prints) rather than guess: Layer
  Dimensionality's shadows, blurs and glows were, and Pixelate's cells
  (how many fit the frame, their average color: the periods of the
  prints' edges, aliased by the preview's scaling, and a fit of a print
  pixelated from the unpixelated one).
  On the graphics card BananaV draws its own effects itself (OpenFX hands
  plug-ins pictures in memory only): `bv_effects::accel::plan` works out
  what an effect needs from its instance's values at the frame's time
  (`PluginEffects::values_at`), `bv-gpu`'s `compositor/own.rs` runs the
  same math as kernels (`shaders/own_*.wgsl`) and the picture stays on the
  card; others' plug-ins go to the processor and back. `bv-engine`'s
  `own_effects_draw_alike_on_the_processor_and_on_the_card` compares both
  for every preset: a new effect of BananaV's needs its plan and kernels
  too.
  A plug-in's settings are drawn as VEGAS draws an OFX effect's
  (`ofx.rs` with `ofx_controls.rs`: the bar with About and help, a slider
  — VEGAS 23's: a dark groove lit from zero to the value, a white knob —
  and its number, the Animate clock — greyed with a "no" sign while its
  parameter is disabled — a position's box, a double click on a slider, a
  color's swatch, a position's box or any setting's name putting it back
  at its default (`resets`: not texts), groups as sections a triangle
  opens, their rows stepped in, a color's swatch — checkered where its
  alpha lets it show — and numbers — as few decimals as they need, its
  alpha last — with VEGAS's picker under them, `ofx_color.rs`: HSV, HSL,
  RGB or Lab, a plane and a slider or the numbers apart, the alpha's
  slider, an eyedropper on the screen; in one, two or
  three columns, the FX window's Columns buttons, `UiSettings.fx_columns`)
  with VEGAS's keyframe controller under them, the bar between them
  setting its height (`ofx_keys.rs`: the effect's row, a row per
  parameter and per number of a position; Lanes — each row's values —
  and Curves — every number's curve in one graph, the row picked edited;
  keyframes colored by their curve, picked by click, Ctrl/Shift and the
  box, moved, copied with Ctrl, spaced with Alt, their menu's Delete,
  Cut, Copy, Paste, "Set to…" and curves — VEGAS's Bézier ones too,
  locked or split tangents (`KeyCurve::Bezier`, `BezierSplit`; the
  handles are `PluginKey::handles`, the host's `bv_ofx::Handles`: a cubic
  over time and value through them, `Animation::bezier_at`, a curve
  beside one meeting it with its own slope, `Interp::slopes`; made
  Bézier, a keyframe's handles keep the curve as it was,
  `bv_engine::plugins::set_curve`, and `set_handle` moves one, a locked
  keyframe's other equal and opposite), their handles dragged in Curves;
  Ctrl+←/→, Delete; Sync Cursor). Edits go back as `ofx::KeyEdit`s. Parametric parameters
  (curves) are edited by `ofx_curves.rs`, as VEGAS's Color Curves: their
  points keep Bézier handles BananaV adds to OpenFX's curves
  (`bv_core::curve`: points without handles pass smoothly, monotone),
  which the plug-in reads through the host; "RGB" edits the curves most
  share (a curve of its own keeps it), points are picked with Ctrl and in
  runs with Shift and move together, the box's menu creates a point or
  makes the curves like one. Curves animate as a whole: no clock, the FX
  window's Animate (`ofx::animate_button`), their keyframes in the
  controller's effect row alone. A parameter may have an interface of its
  own drawn with the draw suite (`kBananaVParamPropInteract` in
  `bv_ofx_sys::extensions`, BananaV's until OpenFX has one: the Color
  Corrector's wheels, Layer Dimensionality's light; open at first unless
  `kBananaVParamPropInteractOpen` is 0): `Instance::param_draw`/
  `param_pen`, shown under its row by `ofx.rs` (`Rows::interact_row`);
  a group's shows under its heading, before its parameters (Credit
  Roll's Position box).
  A number's slider may run logarithmically
  (`kBananaVParamPropLogarithmic`: Picture In Picture's scales, 1 in the
  middle as VEGAS's); a value typed past its slider's ends shows as
  VEGAS's, a dot at that end instead of the knob (Unsharp Mask's Amount). A color parameter may ask its picker's first model
  (`kBananaVParamPropColorModel`: White Balance's neutral color in Lab,
  as VEGAS shows it; Lab's numbers written as VEGAS writes them).
  Values changed under the settings (undo, a preset, the keyframe
  controller) reach the editor's instance with an instance-changed
  action, as the specification asks: plug-ins grey out what their
  settings turn off. A plug-in that reads its
  picture when a button is pushed (Color Curves' Calculate) or when its
  overlay is clicked (the Color Corrector's eyedroppers) gets the one
  the preview gave the effect last (`bv_engine::plugins::watch_input`):
  the preview's still frames keep it (`keep_watched_input`; a still
  frame without it is drawn again once, `State::inputs_for`), playback's
  never — keeping it reads the picture back from the card and keeps a
  plug-in drawing there off it, which made effects slower while their FX
  window was open.
  A push button may ask for a picture (`kBananaVParamPropPicture`,
  BananaV's: the Video Preview's frame, the effect's input, the
  Trimmer's frame, a file; the clipboard and the screen greyed for now)
  on a clip it names (`kBananaVParamPropPictureClip`): `ofx_pictures.rs`
  gets it — the frames the app hands `OfxUi::set_pictures` each frame,
  as effects get pictures, `bv_engine::frame_picture`/`file_picture` —
  and connects it before the plug-in hears of the push (Color Match's
  images). Push buttons whose layout hint says the next one is on their
  line (OpenFX's `kOfxParamPropLayoutHintNoNewLine`) share a row. Horizontal scroll bars that
  zoom by their ends (the timeline's, the controller's) are
  `widgets::zoom_scrollbar`.
- VST 3 plug-ins (audio effects): `bv-vst3-sys` declares the SDK's
  interfaces from its headers (MIT since VST 3.8; the notice in
  `crates/vst3-sys/licenses`, shipped with the packages), their binary
  form by hand (COM's byte order and result codes on Windows,
  `extern "system"`); `bv-vst3` is the host — modules (a `.vst3` bundle's
  library for this system, or a lone library) loaded once and never
  unloaded (`load`; `linked` for plug-ins linked into the tests), the
  host's objects plug-ins hold (`host.rs`: the application, which makes
  messages and attribute lists, memory streams, parameter changes, the
  component handler, the editor's frame) and `Instance`: the component
  and processor, the controller when asked for (the component itself, or
  an object of its own connected through connection points, the
  component's state given to it), the main buses set for the channels
  given, blocks processed with the parameter changes queued (`flush`
  hands them over without sound), states read and given, and the
  editor (`Editor`, a view attached to a window the caller gives,
  resized through the frame in the same call). Projects keep an effect
  as `Effect::vst` (`VstFx`: the module, the class, its parameters'
  normalized values and its states, Base64 in the file through
  `bv_core::base64`); the mixer runs it in its chains (`bv_mixer`'s
  `vst.rs`: an instance for each chain, following the effect's state
  and values without being made again; chains now keep each effect's
  processor while it matches); the interface's side is `vst.rs` (the
  plug-ins found, with their classes from the module information) and
  `vst_ui.rs` (the Plug-In Chooser's VST3 folder on audio chains — an effect's
  presets in the FX window's preset bar are its values and states,
  `presets::VstPreset` by `vst_key` —, and an
  effect's page in the FX window: on Windows the plug-in's own editor
  inside the FX window, as VEGAS shows it (`vst_ui::embedded`: a child
  window, `bv_platform::EmbeddedWindow`, found under the FX window by its
  title, placed where the page leaves the editor's room and hidden while
  an egui menu is open over it; the window fits the editor; the preset
  bar has the plug-in's programs and VEGAS's Open VST Preset, Save VST
  Preset As, Open VST Bank and Save VST Bank As — `.vstpreset`, `.fxp`,
  `.fxb`, banks VST 2's alone —, the chain's button names its kind and
  width, "Modern Equalizer(VST3, 64 Bit)"); BananaV's generic rows where
  it has no editor or Preferences › VST Effects' "Generic VST editor"
  asks, and the plug-in's own window, `bv_platform::PluginWindow`, on
  Linux (an X11 window whose editor gets VST 3's run loop from its frame:
  `IRunLoop`'s file descriptors and timers, run by `bv_vst3::run_loop`
  each frame while one is open); what changes goes back to the effect,
  its state read when a gesture ends). A 32-bit plug-in's editor is made
  in its bridge, in a window of the bridge's own (the plug-in deals with
  its process's windows only), then put inside BananaV's (`EditorWindow::
  embed`: `SetParent` across the processes); the size it asks for comes
  back with the bridge's answers, and the interface's thread, marked by
  `bv_bridge::windows_here`, answers the messages the bridge's windows
  send it while it waits for the bridge. `bv-vst3-testplug` has the tests' plug-ins (a gain
  that is its own controller, with an editor; a polarity switch whose
  controller is apart and hears the component through a message; a gain
  with programs and a tail that keeps what its process context said; a
  module of its own whose gain crashes as it processes);
  `cargo test -p bv-vst3` also builds them into a bundle and loads it.
  Stability: every call into a plug-in's sound and states goes through
  `guard.rs` (`guard.c`'s structured exception handling on Windows'
  MSVC builds, `bv_apple::guard`'s signal handlers on macOS): a crash
  comes back as an error, its module is switched
  off (`Module::crashed`; never called again, not even to release its
  objects: `Instance` leaks them), the sound passes and
  `take_crashes` gives the interface what to tell
  (`follow_plugin_crashes`). Finding plug-ins: `bv_vst3::scan` tries
  each module in a process of its own (BananaV started with
  `--vst3-scan <module> <answer.json>`, `plugin_scan_child`; the
  `bv-vst3-scan` tool for the tests), a job object ending what the
  plug-in started, several at once, `Cache` keeping what they said by
  file stamp (crashes and hangs looked at again); the interface's
  `vst.rs` has one search for each list of folders, shared by the
  Plug-In Chooser and Preferences, the cache's known effects listed at
  once, names made unique (`unique_names`), the tests' programs reading
  modules without running them. `Instance` also has programs (the
  program-change parameter; names from `IUnitInfo`'s program list, else
  the parameter's texts), `tail`, a `Transport` and `locate` filling the
  process context, and `queue_ramp` (points spread over the blocks a
  call is cut into); `bv_vst3::preset` reads and writes the standard
  `.vstpreset` file. The mixer's `vst.rs` tells each block's place and
  the project's tempo, turns FX automation into ramps (a point every 32
  samples, `Chain::automate`), keeps plug-ins across seeks
  (`Chain::reset` restarts them) and lets one sleep once its input and
  output stayed silent past its tail (a second at least). Other editors'
  projects: `veg/audio.rs`'s `vst3` reads a VST 3 effect's states and
  `bv_timeline::vst_id` the identity its class gives (`VstWanted`), which
  `vst::find_wanted` matches to the plug-ins installed.
- VST 2 plug-ins: BananaV's own compatibility. `bv-vst2-sys` (layer 0)
  declares their binary interface from how plug-ins behave — the object
  (`PluginObject`), the requests (`op`), the host's answers (`host_op`),
  the time information — with no Steinberg code or header; `bv-vst2` is the
  host: `module.rs` (libraries never unloaded, their processor read from
  their header), `local.rs` (a plug-in in this process: requests behind
  `guard.c`'s guard, processing in pieces cut at ramps' points, BananaV's
  state format "BVV2", editors), `bridge.rs` (a 32-bit plug-in in
  `bananav-bridge32.exe` through `bv-bridge`: requests and blocks through
  the child's standard input and output; the bridge's side serves a
  `Local`, its editor in the bridge's window), `instance.rs` (either,
  alike), `scan.rs`
  (the child's scan, by the bridge for 32-bit libraries), `preset.rs`
  (`.fxp`/`.fxb`). `VstFx::format` tells a VST 2 effect (its class the
  plug-in's number, `bv_vst2::class_id`); the mixer's `vst.rs` and
  `vst_ui.rs` run both kinds through one path; `bv-vst2-testplug` is the
  tests' plug-in (built for i686 for the bridge's test).
- BananaV's 32-bit bridge (Windows x64): `bv-bridge` (layer 0) has what
  its hosts share — `wire.rs` (frames: a length and its bytes, written
  with `Out` and read with `In`), `Process` (the host's side: the bridge
  started, each request answered within a deadline — 60 s to start a
  plug-in, 10 s for a block of sound, 30 s otherwise —, a crash caught in
  the bridge answered with its details, `answer::CRASHED`; a crash, a hang
  or an end leaves it gone for good, `Lost`, the sound passing), `serve`
  (the bridge's side: requests read on a thread of their own and answered
  on the main thread between the windows' messages,
  `MsgWaitForMultipleObjects` woken by a posted message), `apartment`
  (OLE on the bridge's thread, as BananaV's interface thread) and
  `window.rs` (the editor's top-level window, sized by the plug-in).
  `apps/bridge32` (`bananav-bridge32.exe`, i686, an 8 MiB main stack)
  serves `--vst2-serve`/`--vst2-scan` (`bv_vst2::bridge`, `scan`) and
  `--vst3-serve`/`--vst3-scan` (`bv_vst3::bridge`, `scan`); `exe()` finds
  it beside BananaV (`BV_BRIDGE32` first, the build folders' i686 target
  last), `set_enabled` follows Preferences' "Enable support for 32-bit VST
  plug-ins". A VST 3 module goes there only when it has a 32-bit library
  alone (`bv_vst3::is_32bit`: a bundle's `Contents/x86-win`, or a 32-bit
  `.vst3` file; 64-bit modules never): `bv_vst3::Bridged` answers as
  `Instance` does (parameters, values and texts, blocks with their changes
  and ramps, the transport and place, programs, states, latency, tail; its
  editor in the bridge's window, `poll_editor` bringing what it reported),
  the scan's `Scanner::bridge` looks at it there (`ModuleScan::bits32`;
  `Failure::NeedsBridge` without one, not kept in the cache), and the
  mixer's `Plugin::V3Bridged` and `vst_ui.rs`'s `Plug::V3Bridged` run it —
  the page's `Asked` keeping a bridged plug-in's values a moment and its
  texts by value, each answer being a request. `cargo test -p bv-vst3`
  builds the test plug-ins and the bridge for i686 and runs them there.
- Audio busses and assignable effects chains, as VEGAS's Help and its
  signal-flow diagram describe them: `bv_timeline::busses` (`Project`'s
  `busses`, A to Z, and `fx_chains`, each a `Channel` — name, gain, pan,
  mute, solo, effects before or after the volume, output —, a bus's
  `sends` to chains, a chain's input level, `master_fx`; `Track::output`
  and `Track::sends`; the routing a graph of `Strip`s that never plays
  in a circle: `reaches`, `bus_can_route`, `chain_can_route`,
  `bus_can_send`; `strip_order` mixes each before what it feeds; a
  deleted bus's tracks, busses and chains go to the bus before it; a bus
  may play on a pair of the output device's channels, `Output::Hardware`:
  heard while playing, never rendered, still heard with the Master
  muted, other busses sending to it, `bus_can_send_to` — the playback's
  `add_hardware`, the pairs offered `Engine::hardware_outputs`). The
  mixer's `add_track` reads a track whose chain answers late (a VST 3
  plug-in's latency, `Chain::latency`) that much earlier — plug-in delay
  compensation, `AudioPool::chain_latency`; an event's effects alike, the
  event read ahead; along the routing too: `Delays`, each bus's and
  chain's own latency and that of what it plays into, the track read as
  early as its latest path needs and the others fed through the pool's
  delay lines, `DelayKey` — and sends from before the
  volume and mute (a track not
  heard still plays into them unless
  `set_prefader_sends_listen_to_mute`: `mixed_tracks`) or after them,
  into `Sums`; `finish_mix` takes each strip through its channel (a bus's
  pre-volume tap before its pan) into its output and a bus's sends, then
  the Master's effects before or after its volume, and keeps each strip's
  peaks (`take_strip_peaks`, through the playback to
  `Engine::take_strip_peaks` and `BananaApp::strip_meters`). A bus's and a
  chain's volume, pan and mute envelopes (`Channel::envelopes`, its bus
  track's: `EnvelopeOwner::Strip`, `toggle_strip_envelope`) and a track's
  send envelopes (VEGAS's bus envelopes: `EnvelopeKind::Send`, riding on
  the send — toggling one on makes the send at 0 dB, a send with one
  stays, a bus or chain deleted takes both) play sample by sample
  (`follow_envelopes`, `mix_with_envelopes`' send). In 5.1 projects
  the Surround Panner (`bv_timeline::surround`: `Track::surround`,
  `Channel::surround`, `SurroundOwner`; never panned, `MIDDLE`) places
  the pair of what plays into the Master among the six channels by the
  pan type (`SurroundPan::gains`; the trim added to the Surround Pan
  Keyframes, `point_at`; the mixer's `Placement`, every 64 samples); what
  plays into a bus goes there stereo and the bus pans
  (`Project::surround_pans`); the Surround Master's channel faders are
  trims after the master's volume (`MasterBus::channel_db`, Lock Fader
  `unlocked`); the Master Bus window's Audio Device Selector puts the
  Master's pairs on the device's (`MasterBus::device_pairs`, the
  playback's `route_master`). Its window is `tabs/surround.rs` (the room
  drawn after the Help's pictures, `paint_room`; the small panners of the
  track headers, bus tracks and Mixing Console are `mini_panner`; the
  point recorded while playing in Touch and Latch, as automation is,
  `SurroundWrite`, `follow_surround_write`), its
  keyframes rows `timeline/surround_keys.rs` (`Row::keys`, the rows'
  `body` without them). The
  interface: the Mixing Console (`tabs/console.rs`, laid out as VEGAS
  23's screenshots — Show Channels › Preview Bus adds the Preview
  channel, the first —: its toolbar (the name and sound format on the
  left, the rest on the right, the insert buttons with their texts when
  they fit), the Channel List over the View Controls (a splitter under
  the list and one between the pane and the strips, `ConsoleView::list_h`,
  `pane_w`; the channel kinds, the Meters and Faders regions — every
  region in Properties and Layout › Show Control Regions; new consoles
  show I/O and Faders —; Show All lit while everything shows,
  `all_shown`, `show_all`), a strip a column — `column`: a header with
  the track's number in its color (a bus's letter) and its kind, the
  regions, its name at the foot (none for a track BananaV named,
  `given_name`, as VEGAS's unnamed ones); a click anywhere picks it, blue,
  the Master too (`ConsoleView::master_picked`) — per track, chain and
  bus with the control regions Insert FX — effects as buttons —, Sends,
  I/O — flat rows, the outputs on the last, blue while their menu is open
  —, VU Meters, Meters — a meter for each channel, `peaks_at` — and
  Faders — the automation row (its mode's icon and name, blue while
  Automation Settings are on; its menu the modes with their icons,
  `mode_item`), the pan as a mark over a line, a button column (mute,
  solo, fx, phase, arm — its right-click the Input Monitor Mode —; a
  bus's effects' order) beside a fader of one square and a meter at the
  console's own scale (`METER_SCALES`: 90 dB numbered every 6, a dash
  every 3, as VEGAS's; the wheel goes through them), its value under it;
  with automation on the pan's mark and the fader's knob red and the
  mute VEGAS's mute-envelope sign — a channel
  dragged by its header or name to
  another place (`Project::move_strip_to`, `move_track_to`), several
  picked with Ctrl and Shift moving together (`pick_strip`,
  `track_selection`, `set_controls`), the Sends region's divider
  (`ConsoleView::send_rows`), each track's Input Monitor Mode —; its VU/PPM meters are `bv_mixer::meters`,
  IEC 60268-17 and -10 ballistics computed in the mixer only while the
  console shows them, `Engine::set_vu_meters`, their readings through the
  playback to `Engine::take_vu_levels` and `BananaApp::vu`, drawn by
  `vu_meters.rs`), the track header's bus button and multipurpose slider
  (`timeline/headers.rs`, `TimelineView::send_targets`), the FX window's
  `FxTarget::Bus`, `Chain`, `Master` and `NewChain` (Insert › Audio
  Assignable FX…: the chain made when the Plug-In Chooser's OK has
  effects), the Master Bus window's FX, Project Properties' number of
  stereo busses, and the audio bus tracks (`timeline/bus_tracks.rs`, View
  › Audio Bus Tracks, B: under a splitter at the timeline's bottom,
  `Layout::bus_rows` — the video bus track first when View › Video Bus
  Track shows it, `BusTrack::Video` —; a header — Bus FX, Mute, Solo,
  volume, pan, its menu's envelopes, Minimize and Maximize — and a lane
  whose envelopes are edited as a track's; the wheel over their headers
  scrolls them).
- The Video Preview draws as `bv_engine::PreviewLook` says (its quality,
  Split-Screen View's FX Bypassed part, the channel shown), set with
  `Engine::set_preview_look`; the quality and a retained split screen are
  `UiSettings.preview`. In a narrow window it is VEGAS's: its toolbar
  never widens it (what does not fit is cut off at its edge), the frame
  fits its width, the transport's buttons that do not fit go to More and
  the status line shows the items that fit. Its Copy Snapshot to Clipboard and Save Snapshot
  to File take the frame as a render draws it (`Engine::snapshot`, as
  VEGAS's at Best (Full): the output's size at Best quality, every effect
  — plug-ins too —, never a prerendered part or a proxy; the file a PNG
  with the frame's straight alpha over nothing, as VEGAS's files keep
  it, the copy over black as the preview shows it). Generated media are edited in
  `generated_media.rs` (VEGAS's Video Media Generators window: the
  generator and its settings' columns; the frame size and the generated
  media's own Duration — `MediaInfo::duration` under
  `Generator::DURATION`, the event's length where it has none — with the
  plug-in's help, Replace Plug-In's list of the generators of its kind of
  media, `tabs::generators::replacements`, Match Event Length and Media
  Properties; the preset; Titles & Text's Transfer Text Properties a
  button of its bar, BananaV's `kBananaVParamPropTransfer`; events pasted, copied by Copy Here or
  dragged from Project Media while another event shows their generated
  media ask for a copy of their own or a reference, VEGAS's question,
  `generated_copies.rs` on `Project::copy_generated_media`, the answer
  part of the edit's undo); the user's generator presets
  are `UiSettings.generator_presets`. Media Generator's Titles & Text and
  Insert › Text Media use BananaV's own plug-in
  (`bv_engine::plugins::TITLES_ID`); Insert Subtitles from File / from
  Regions are `subtitles.rs` (title events on a new top track). A 360°
  project (`ProjectSettings::output_360`, Project Properties' Advanced
  tab: equirectangular frames) shows in the Video Preview's 360° View
  (`view360.rs`: the frame drawn on a grid of the camera's view, its
  texture repeating across; dragging turns the camera, the wheel zooms),
  and its MP4 and MOV renders carry Google's spherical video metadata
  (`bv_mp4::VideoTrack::spherical`: V2's `st3d` and `sv3d` boxes, V1's
  XML `uuid` box); `bv_mp4::read::spherical` reads either, and a photo's
  panorama XMP (`GPano:ProjectionType`) marks a 360° picture
  (`VideoStreamInfo::spherical`), which File I/O's auto-detection
  (`Prefs::detect_360`) turns into the project's 360 Output on import.
- Rendering (File › Render…): `bv_engine::FileFormat` lists the formats and
  which encoders of this computer write them (`bv_mf::encoders()`: the
  processor's and the graphics cards' — NVENC, AMD, Quick Sync — found at
  run time); `crates/engine/src/render` renders, `crates/ui-egui/src/render`
  is VEGAS 23's Render window (templates in `templates.rs`, named in the
  interface's language from their settings); Tools › Render to New Track
  (Ctrl+M) is the same window (`RenderDialog::to_new_track`), its file put
  on new tracks at the render's start once rendered
  (`render::place_new_track`, `BananaApp::new_track_render`). The
  options' Enable multichannel mapping (`RenderOptions::multichannel`,
  Wave and Wave64 files) with its Channel Mapping dialog
  (`render/channel_mapping.rs`; the mapping is the project's,
  `Project::channel_mapping` of `bv_timeline::MappedStrip`s) puts each
  mapped strip's sound — the Master's mix, a bus's output kept by
  `bv_mixer::AudioPool::keep_bus_outputs` — in channels of its own (the
  engine's `mapped_planar`; one with Mono downmix). File › Real-Time
  Render (`realtime_ui.rs`, the engine's `realtime.rs`) plays the project
  from the start and writes what plays to a WAV file (the playback's
  `set_capture`, before the metronome's clicks). Hardware encoders are driven as
  MFTs (AMD's need a Direct3D 11 device); BananaV writes the MP4/MOV
  container itself. Frames for those encoders are packed 4:2:0
  (`Output::packed_420`): the card converts them (`Output::Yuv420`, the
  `encode_nv12`/`encode_p010` kernels, the same values as
  `bv_color::linear_to_420`, laid out as `VideoFrame::packed_420`) and a
  writing thread encodes them while the next ones are drawn (`render::run`).
  While a render runs the interface keeps out of its way: playback stops
  when it starts and stays stopped (`BananaApp::frame`), the render tells
  the interface how far it is a few times a second, not at each frame
  (`render::TELL_EVERY`; told at each frame the whole window drew itself
  as fast as the render went), its frames reach the preview four times a
  second with Show Video in Preview Window, and the progress window
  repaints as often. On the graphics card two painters draw a render's
  frames at once, every other frame each on a thread of its own with
  plug-in instances and readers of its own, the frames handed on to the
  writer in order (`render_painters`, `BV_RENDER_PAINTERS` to measure:
  Sapphire's S_ZapFrom over a 1080p60 recording on an RTX 4070 went from
  22 to 32 frames a second, VEGAS 25; a third painter added 7 %) when
  plug-ins draw them (`accel::has_plugins`; without them the encoder or
  the readers set the pace); on the processor and for smart rendering one
  painter draws them in turn.
  `bv-engine --test render_desempenho render_speed_with_a_plugin`
  measures a render through another maker's plug-in.
  `cargo test --release -p bv-engine --test render_desempenho -- --ignored
  --nocapture` measures the speed. MPEG-2 (`render/mpeg.rs`) is BananaV's
  own video encoder, `bv-mpeg2` (ISO/IEC 13818-2: Main profile, 4:2:0,
  frame pictures, I, P and B — `Settings::b_frames`, two between anchors
  as DVD's IBBP; the encoder holds a GOP's frames until the anchor after
  them is coded and gives the pictures back in the order they decode,
  each with its frame, `Picture::display`; the writers show each picture
  a frame after its place and decode them a frame apart, the sound as
  late; a GOP's last frame is an anchor, so GOPs stay closed —, closed
  GOPs each opened by sequence, sequence extension and
  BT.709 display extension headers, the default matrices, Annex B's codes
  — coefficients Table B-14 lacks escaped —, a quantiser a picture from a
  rate control, a diamond motion search at whole pixels, pictures rebuilt
  as decoders rebuild them; Windows' own MPEG-2 encoder is licensed only
  on some editions), the system's MPEG audio (Layer II) encoder
  (`bv_mf::Codec::Mp2`; linear PCM where there is none) and BananaV's own
  program stream (`bv_mp4::ps`: a pack header and one PES packet,
  padded, in each 2048-byte pack — a DVD's sector —, the system clock
  following the data no more than half a second ahead of decoding); the
  picture's shape is the sequence header's aspect code, DVD's and HDV's
  anamorphic frames are filled, and with the markers saved their frames
  are I-frames. `bv-engine --test mpeg2` decodes a render with the
  system's MPEG-2 decoder and compares it with the project. MPEG-1 for
  Video CDs (`FileFormat::Mpeg1`, the VCD NTSC and PAL templates) is the
  same encoder's `Settings::mpeg1` — no extensions, the pixels' shape in
  the sequence header (`pel_aspect`), escapes of 8 or 16 bits, levels
  within ±255, MPEG-1's mismatch control (each coefficient made odd), the
  colours made BT.601's (`to_bt601`: MPEG-1 says nothing of them and
  decoders take BT.601's) —
  with the system's MPEG audio, which it needs, in an MPEG-1 system
  stream (`PsWriter::mpeg1`: Video CD's 2324-byte packs); `--test mpeg2`
  decodes one too. A DVD-Video
  folder (`RenderOptions::dvd` with an MPEG-2 template; `render/dvd.rs`)
  is `bv-dvd`'s: the title's video objects (`vob.rs`: VOBUs of a GOP each
  — NTSC 15 frames, PAL 12, an interlaced sequence as DVD-Video's must
  be — opened by a navigation pack whose PCI and DSI are filled in once
  every VOBU's place is known, the sound as linear PCM in private stream
  1, a chapter cell at each marker, cut into 1 GiB files) and the
  information files (`ifo.rs`: `VIDEO_TS.IFO`'s first-play chain jumping
  to the title, `VTS_01_0.IFO`'s program chain of the chapters, time
  map, cell and VOBU address tables; their `.BUP` copies). Tools › Burn
  Disc › DVD (`burn_dvd.rs`, VEGAS's Burn DVD dialog: the video format —
  DVD's NTSC or PAL frame, 4:3 or 16:9, `RenderOptions::dvd_wide`, the
  project's picture letterboxed in that shape, `render::Fit::Within` —,
  the audio format, the drive, its speed, Eject when done, kept in
  `UiSettings::burn_dvd`) renders the project into a temporary folder,
  makes it a disc image (`bv_dvd::write_image`, re-exported as
  `bv_engine::write_dvd_image`: UDF 1.02 bridged with ISO 9660, the files
  one after another in DVD-Video's order) and burns it
  (`bv_platform::burn_image`: IMAPI2's data writer reading the image
  through BananaV's own `IStream`, which tells how far it is and fails
  once canceled; the drives from `burners`), the job's window modal as
  the render's. Tools › Burn Disc's audio CDs are `burn_cd.rs` (VEGAS's
  Burn Track-at-Once and Burn Disc-at-Once Audio CD dialogs,
  `UiSettings::burn_cd`): the project's sound — or its loop region, or
  from the CD layout's first track to its last — rendered as CD audio
  (a 44.1 kHz 16-bit stereo Wave file), then burned as a track
  (`bv_platform::burn_audio_track`: IMAPI2's Track-at-Once writer, the
  disc closed or left open) or as the layout's tracks, each from its start
  to the next one's, with their indices, ISRCs, copy and pre-emphasis
  flags, the disc's catalog number and first track number
  (`burn_audio_disc`: IMAPI2's raw image creator, then its raw writer);
  Close disc and Erase RW disc too. The jobs and their window are
  `burn_job.rs` (the DVD's too). IMAPI2 writes no test runs: the dialogs'
  tests are greyed out.
- Menu commands live in `crates/ui-egui/src/commands.rs` (`Cmd`: label,
  default keys, enabled/checked, run). Keyboard shortcuts are dispatched by
  `crates/ui-egui/src/keymap.rs` (VEGAS command ids per context, user maps).
  egui reports the numeric keypad's keys as the digits and signs they type:
  `keymap::keypad` tells them apart with `bv_platform::numpad_down` (Num *
  is read from the keyboard); Shift with a key that moves the cursor
  extends the time selection (`keymap::extending`).
- Icons are `crates/ui-egui/src/icons.rs`: the `Icon` list and `paint(painter,
  rect, icon, color)`, which draws each icon as vector shapes from its layers in
  `icons/glyphs.rs` — SVG path text on a grid of 24 units (`icons/path.rs` reads
  M L H V C S Q A Z; arcs become cubics) — so nothing is baked and every size
  is sharp. A new `Icon` needs its layers there (the match is exhaustive, and
  the tests check that each icon is drawn and stays inside the grid). The set's
  rules: strokes of 1.5 units with round ends and joins; corners of 3 on
  containers; a body is its outline over a gradient tint of the icon's color
  (`body`), supporting details at half strength (`.dim()`), what acts is solid
  (`solid`), fine details only from 20 px (`.fine()`). Only the caller's color
  is used, so hover, selected, disabled and Preferences' icon color strength
  work on every icon; the red of Record and a few marks (`.red()`) is the only
  other. Under 17 px an icon is drawn on the pixel grid (points at pixel
  centers, lines of one pixel) so the toolbar's 14–16 px icons stay sharp.
  BananaV's logo is not one of them: `icons::paint_logo(painter, rect)`
  (`icons/logo.rs`) draws the transparent official logo in its own colors,
  never tinted — the paths of `packaging/icons/bananav-logo-transparent.svg`,
  filled with soft edges that keep the outline exact — in the toolbar's
  corner, the welcome screen and the Explorer's project files. If the logo
  changes, change its paths there too (the tests check their shapes).
- Project Notes are `bv_timeline::notes` (`Project::project_notes`: each
  at a time, a color of `NOTE_COLORS`, a label, its text, when written —
  seconds since 1970 and the clock's offset —, resolved or not; the old
  single text, `Project::notes`, becomes the first note on opening) and
  the window `tabs/notes.rs` (its default color and label are
  `UiSettings::note_color`, `note_label`).
- Project archives and copies of a project's media are `archive.rs`
  (File › Export › Project Archive's dialog; File › Copy Media with
  Project, `UiSettings::save_as_copy_media`, run after Save As through
  its Copy Media dialog, `CopyMediaDialog`): a `CopyPlan` of the files to
  copy into a folder and the media's new paths (the archive's,
  `plan_archive`: with Include nested projects each nested project's file
  too, `CopyPlan::projects`, written pointing at its own media's copies,
  nested ones deeper alike, each once) — with Create trimmed
  copies of source media, the sound files' parts events use plus the
  extra head and tail as `TrimCopy`s, rendered to WAV by
  `bv_engine::trim_sound`, the project then `retarget`ed into them and
  its inactive takes removed —, copied by a `CopyJob` thread with the
  status bar's progress.
  Render's Swap video files on render is `RenderOptions::swap_files`
  (the engine renders beside the file there and renames into its place);
  its path reference, `RenderOptions::path_reference` with
  `RenderSettings::project_file`, is a `bvpr` box or chunk in the files
  BananaV writes (`Engine::project_reference` reads it; Edit Source
  Project opens that project in another BananaV process).
- Edit Details is `tabs/details.rs` (VEGAS 2026's: a table painted by
  hand — `Geometry`, `Hit` —, the views' columns, sort and widths in
  `DetailsView` and the Preset box's `UiSettings::details_presets`; typed
  values go through `apply_text` on the project, the ruler's format read
  by `timefmt::parse_position` and `parse_length`; its commands are
  `DetailsOp`, VEGAS's Details.* keyboard ids, Paste getting the
  clipboard's text from the keyboard map's paste event,
  `BananaApp::pasted_text`, and Copy handing its text out at the frame's
  end, `to_clipboard`). Command markers are `bv_timeline::commands`
  (`Project::commands`) with the Command Properties dialog `command_ui.rs`
  (templates in `UiSettings::command_templates`); closed captions are
  `608CC1`–`608CC4` command markers whose Comment holds VEGAS's caption
  markup and whose Parameter shows the text (`bv_timeline::captions`: a
  CEA-608 decoder of the markup, `Project::captions_at`,
  `caption_spans`, `add_captions`; plain text typed is made a pop-on
  caption, `pop_on`), read and written by `bv_timeline::caption_files`
  (Scenarist's CEA-608 bytes, MacCaption's CEA-708 packets — their
  608 pairs, `read_mcc` —, SubRip, SAMI, RealText, QuickTime text,
  transcripts, DVD subtitles; `write_scc` encodes the markup back into
  608 pairs, the file a render writes beside its own:
  `render/progress.rs`' `captions_file`), the interface's side
  `captions_ui.rs` (also Tools › Scripting › Promote Media Closed
  Captioning: the `.scc`/`.mcc` beside the selected events' media,
  `media_captions`, placed where the events show it)
  (File › Import › Closed Captioning, Tools › Scripting's exports, the
  Video Preview's Overlays CC1–CC4 drawn on a 608 grid) and Edit
  Details' Ctrl+K (`DetailsOp::SetTime`: the picked row to the playback
  position, the next one picked); the audio CD layout is
  `bv_timeline::cd` (`Project::cd_tracks`, `cd_indices`, `cd_rows` for
  the PQ list, `lay_out_cd_from_events`); both have bars under the marker
  bar while the project has them, `timeline/marker_bars.rs`, and follow
  ripple edits as markers do (`remove_marker` removes any of them).
  `MediaInfo::reel` holds a media's tape name and own timecode (Media
  Properties); `MediaInfo::layer` makes a media one layer of a layered
  Photoshop document (`bv_image::read::psd_layer`: raw, RLE or ZIP,
  gray, RGB or CMYK, through its layer mask; opened by
  `bv_media::open_video`, its thumbnails apart); Add Across Tracks gives a
  layer's track its blend mode (`psd_layer_blends`,
  `CompositeMode::from_photoshop`) when the track holds only it. The Marker Tool is
  `timeline/marker_tool.rs`
  (`TimelineView::marker_tool`: the active bar, the picks; its button
  `Layout::marker_tool`; group drags `BarDrag::Picked`, `move_picked`; its
  keys are taken before the keyboard map, `BananaApp::marker_tool_keys`).
- The Trimmer is `tabs/trimmer.rs`, laid out and behaving as VEGAS 23's
  (the user's prints): the history's list and its Sort, Clear, Remove and
  Trimmer on External Monitor buttons (`header`); the picture, or a
  sound's ruler, waveform and scroll bar (`video_body`, `sound_body`); the
  loop bar with the media's markers (`loop_strip`), the time bar
  (`time_bar`) — sized and drawn as VEGAS's, measured on the user's
  prints (`BAND_H`, `STRIP_H`/`STRIP_MARKS_H`, `BAR_H`: the loop region's
  grey down over the time bar, yellow handles with poles, VEGAS's cursor);
  markers' flags are `marker_flag.rs` (VEGAS's numbered pennants, also
  the timeline's marker bar); the transport in the middle (`transport_row`, the buttons
  of `transport_buttons.rs`' `Bar::Trimmer`, drawn by
  `paint_trimmer_icon`) and the time boxes (`time_boxes`, the timeline's
  `TimeBox` and `paint_box_icon`). `TrimmerState` keeps the media (a
  subclip opens alone: `Showing`, its times the part's, `file_time` the
  file's), the history, cursor, the loop region (in and out) and whether
  it is selected (`selected`: a drag, I and O, Select Loop Region select;
  a click elsewhere lets go, the region staying), the streams, the audio
  stream and channel pair shown, Enable Timeline Overwrite, Trimmer on
  External Monitor, the time bar's view and the waveform's share (none at
  first). Its commands are `TrimmerOp`, run by `run` (the keyboard's
  Trimmer context, the buttons, the menus); what it adds goes through
  `add_from_trimmer` (one undo with Fit to Fill's rate, the overwrite, the
  shown stream alone with Use All Streams and Channels off); A without a
  selection adds from the Trimmer's cursor to the media's end, Shift+A
  needs one. Its menus are `trimmer_menu` (`Area`: the picture's, the
  waveform's, the bar's — everything — and the loop region's) and the
  ruler's `format_menu` (`UiSettings::trimmer_format`,
  `trimmer_project_rate`); its switches are `UiSettings::trimmer_no_*`,
  `trimmer_frames` (Show Video Frames, off at first) and
  `trimmer_shown_streams_only`. With the monitor off the Video Preview
  shows the Trimmer's picture while the Trimmer has the focus
  (`BananaApp::trimmer_in_preview`, `trimmer_picture`, `paint_in_preview`;
  the external monitor too with Trimmer on External Monitor); Space,
  Shift+Space and Enter play it (`TrimmerOp::PlayToggle`…,
  `trimmer_play`), Loop Playback over its loop region
  (`TrimmerState::loop_back`, `follow_preview`). The engine keeps the
  Trimmer's one-event project while its media and settings stay
  (`Engine::trimmer_project`) and, while its preview plays, has its
  frames drawn ahead as the timeline's (`request_trimmer`'s `ahead`),
  the interface repainting every frame as during the timeline's
  playback (`BananaApp::previewing`).
  Subclips are
  `bv_timeline::subclips` (`Project::subclips`: items of Project Media
  standing for part of a media, ids from the project's counter;
  `subclip_info` lists them; their events are of the parent media, placed
  by `place_media_streams`) and the Create Subclip dialog `subclip_ui.rs`
  (`BananaApp::open_in_trimmer` opens a subclip alone; Open Parent Media
  in Trimmer and Select Parent Media its parent with its part selected).
  Project Media's shortcut menu is `media_menu.rs` (VEGAS 23's items and
  greying by kind; Swap Video Files, Replace…, Refresh, Rename's box,
  Rotate 90°, Lay Out Tracks by timecode or date, Remove from Project and
  Delete File(s) — `Asked::RemoveAndRecycle` —, and Detect Scenes on a
  media: `Engine::start_media_scene_detection`, `MediaScenes`'s progress
  window, `AfterScenes`). Media previews are
  the engine's `media_preview.rs` (a playback of a project of one event,
  `Engine::start_media_preview`, `set_preview_gain`) and the interface's
  `media_preview.rs` (`BananaApp::preview_media`, `preview_file`,
  `stop_preview`, `follow_preview`; the Preview strip is `tabs/master.rs`'
  `preview_strip`, `UiSettings::preview_fader`, `preview_gain_db`).
- The Video Preview's and the Trimmer's transport bars show the buttons
  their More › Edit Visible Button Set ticks (`transport_buttons.rs`:
  `TransportButton`, `Bar`, `UiSettings::preview_transport` and
  `trimmer_transport`, `None` VEGAS's), the others in their More menus.
- More › Edit Visible Button Set is `header_buttons.rs`, VEGAS 23's
  dialog (the user's prints): the buttons each kind of header shows —
  Mute and Solo among them, the only ones at first — are
  `UiSettings::header_buttons` (`settings::HeaderButton`, in the dialog's
  order, Min/Max one; settings saved before Mute and Solo were buttons
  keep them, `StoredHeaderButtons`). `timeline/headers.rs` lays them out
  as VEGAS's (`ButtonsFor::lay_out`, measured on the prints: the top row
  from the right — Solo, Mute, the track's own (the bus button, Restore
  Track Height, an armed track's Arm), Automation Settings with its menu's
  arrow, Track FX (an adjustment track's always), Track Motion, Color
  Grading, Bypass Motion Blur, Minimize, Maximize, a button every 24
  pixels — each where it fits, so they go from the left as the list
  narrows; a video track's Compositing Mode, Make Compositing Parent and
  Child at the end of its second row, the level slider before them, its
  label only while the slider keeps room) and draws them (`header_button`,
  `automation_button` — the Mixing Console's mode icon —, the icons
  BananaV's own in `header_icons.rs`); More (⋯, `more_menu`) has the
  commands of the buttons a header does not show, in VEGAS's order, its
  switches checked as VEGAS's (`menus::mark_item`), then Edit Visible Button
  Set. The track list's width is
  `UiSettings::track_list_width` (`TimelineView::header_w`; the dark
  splitter between it and the lanes, `SPLIT_W` wide from the timeline's top
  to its bottom as VEGAS's, is `list_border`), the headers giving way as it
  narrows, as VEGAS's
  (`headers::Tier`): the color strips alone under 64 pixels, then Mute and
  Solo over the sliders alone (an audio track's pan from 120), their
  values from 200 (an audio track's from 230: VEGAS 23 shows none at 215;
  the meters from 190), their names — Vol: and Pan:,
  the two sharing the row — from 400; the time display smaller, then
  gone, the Rate's slider, then the Rate; the list as wide as all but 120
  pixels of the timeline. The timeline's bottom is VEGAS 23's: the track
  list goes down to it, the Rate there (`Layout::rate`, as tall as the
  scroll bar and the toolbar together), the toolbar beside it from the
  lanes' left edge (`toolbar.rs`' `ui`, its buttons cut off where the time
  boxes begin) and at its right VEGAS's time boxes (`time_boxes`, as its
  Help's "Creating selections and positioning the cursor" shows them: with
  the cursor in the time selection its start, end and length; otherwise
  where the cursor is, the end's icon alone and the length dimmed; a
  double-click types into a lit one — a time in the ruler's format, or +
  or − and a length from what it shows, `type_into`).
- A track header's Set Default Track Properties is `track_defaults.rs`:
  the defaults are `UiSettings::track_defaults` (one `KindDefaults` for
  audio, one for video); the tracks an edit makes get them each frame
  (`apply_track_defaults`, through `History::amend`, as part of that
  edit's undo): tracks whose id is past the counter seen before the edit
  (`Project::id_counter`), after a new edit (`History::edits`), never
  after undo, redo or a reset (`History::moves`).
- Track groups (VEGAS's "Grouping tracks") are `bv_timeline::track_groups`
  (`Track::group`, `Project::track_groups`; a group's tracks always next to
  one another: `group_tracks`, `ungroup_tracks`, `move_track_group_to`, and
  `normalize_groups` after every change of the track list and on opening
  a file; `track_muted`/`track_soloed` give what plays, the group's Mute
  and Solo overriding its tracks'); the timeline's side is
  `timeline/groups.rs` (`Layout::groups`' header rows, `Row::group` — its
  header after the grouping lane —, collapsed tracks left out of the
  rows) and several tracks are picked with Shift and Ctrl on their headers
  (`BananaApp::track_picks`, `track_selection`, `pick_track`); a
  right-click on one of them keeps them picked (`track_focus`) and Delete
  Track, or Delete with the track list focused, deletes them all in one
  undo (`tracks_with`, `request_delete_track`, `delete_tracks`). The group
  header's Mute and Solo are drawn as VEGAS's Help shows them
  (`group_button`: the tracks as bars, a "no" sign or an exclamation
  mark).
- The timeline's shortcut menus are `timeline/shortcut_menus.rs` (below
  the tracks, in the timeline and the track list: Open, the tracks to
  insert, Preferences; an event's, VEGAS 23's items in its order and no
  others — every label in the icons' column, `menus::icon_item`; greyed
  where VEGAS greys them: Open Parent Media in Trimmer but for an event
  of a subclip's part, Event Heal and Close Gaps unless they would do
  something, `Project::can_rejoin`, `has_gaps`, Create Sync Link with
  Selected Events without selected events of other groups; Synchronize
  and Sync Link always there, their items greyed —; a track header's,
  VEGAS's for its kind, are `headers.rs`' `header_menu`); an event dragged below the tracks shows
  VEGAS's ghost there and lands on a new track of its kind at the bottom
  when let go (`TimelineView::new_track`), and an event's buttons go one
  by one from the left as it narrows; its mouse
  (VEGAS's modifiers, the wheel, auto-scroll, the right-drag's Move/Copy
  Here menu) is `timeline/lanes.rs` (`start_drag`, `click`, `wheel`: it
  zooms toward the needle, as ↑/↓ and the zoom buttons do,
  `TimelineView::zoom_around` — in view the needle keeps its place, out
  of view it comes to the middle; playing, the playback position's), on
  the model's `slip_trim`, `trim_adjacent` and `copy_here`. A drag near
  or past the tracks' left or right edge runs the view on that way while
  it lasts, faster the further past, the drag going on at the edge
  (`TimelineView::edge_scroll_x`, `edge_scroll_y`; lanes.rs' `edge_held`):
  events moved and trimmed, fades, selections, envelopes, the bars'
  markers and loop region, the ruler's scrub, media dropped; up and down
  too for events, the selection rectangle (its corner kept on the
  timeline), envelope points and tracks reordered, as VEGAS's. While the
  timeline draws, `app.timeline` is a stand-in: its code uses the view it
  is given. Selection follows VEGAS: the events the user picked (click,
  Ctrl, Shift, the rectangle) are outlined in yellow, the group mates
  selected with them in blue (`app.picked`, valid while the selection is
  the one it was noted with: `picked()`, `set_picked`,
  `selected_by_group`). Dragging on an empty spot draws the selection
  rectangle (a right-click while dragging cycles `BoxShape`: time, tracks,
  free), which picks the events it takes as it goes, after those picked
  when it began with Ctrl (`box_pick`, `TimelineView::box_base`); a right-drag there and the loop region bar make time selections;
  double and triple clicks on empty spots are `empty_clicks`; an event
  double-clicked and dragged extends the time selection to the next fade
  or event edge (`extended_selection`, `Drag::Extend`). The fades
  of an event's edge under another event give way to the crossfade
  (`Track::crossfades`, `effective_fades`), drawn and grabbed only where
  there is none. Fades are drawn as VEGAS's: up to the event's level line
  (`fade_points`: the gain times the level, as the event plays it — video's
  Smooth an S, `scene::video_curve`, sound's the sine of constant power),
  a dark line across the event where each ends (`paint_fade_edge`; a
  crossfade's where it begins and ends too), the level line only between
  them (`level_span`, its grab zone too), their handles VEGAS's blue
  corners on it (`paint_fade_tab`, `fade_handle_y`; the pointer over one
  an arrow with a fade's curve, drawn, `fade_cursor`), the level's VEGAS's
  blue tab in the middle of the line between them (`paint_level_handle`),
  both while the event is picked or under the pointer — edged dark and
  drawn after the track's events and outlines, whole —, nothing shaded; an
  audio event's waveform as loud as the event plays it, live
  (`paint_waveform`, `fade_gain`: its level and its fades', crossfades'
  too), a crossfade's two sounds merged over it (the one going out drawn
  again there), and a picked event's outline over what overlaps it, as
  VEGAS's (the user's prints); a right-click on one
  opens VEGAS's popup of curves (`fade_menu`, `ContextTarget::Fade`,
  `Project::set_fade_curve`), on a crossfade its 5 × 5 pairs
  (`crossfade_menu`: a column per curve coming in, a row per curve going
  out, VEGAS's lists' order); an audio crossfade's overlap is darker
  (`OVERLAP_SHADE`), and a video crossfade with a transition shows VEGAS's
  label instead of its X — the transition's name in a blue band, a
  plug-in's Transition Properties icon at its right opening the
  transition in the FX window as VEGAS's Video Event FX
  (`FxTarget::Transition`, `transition_window.rs`: "Transition: name" over
  a chain of the transition alone — its checkbox `Clip::transition_bypass`,
  a plain crossfade while off; removed, the crossfade stays —, its
  presets, settings and columns; a double-click on the label too); View ›
  Event Fade Lengths is off at first, as VEGAS's, but a crossfade shows its
  length — in a small white box in its middle, seconds and frames, just
  under its transition's label when it has one (`label_band`), however far
  out the zoom is — while one of its events is selected, its curves
  VEGAS's colors (the one going out blue, the one coming in white). Fade-outs are named as VEGAS names them, by
  how they begin (`FadeCurve::out_shape`, `eval_out`: a Fast one drops at
  once); projects of format 1 swap their fade-outs' Fast and Slow when
  opened, and new events' fade-outs take the preference's shape turned
  around (a Fast fade-in, a Slow fade-out). The grid (Options ›
  Grid Spacing, `BananaApp::grid_ticks` at the zoom of the view being
  drawn) is VEGAS's dotted lines on the timeline's background, always
  (`paint_grid`, `theme::grid`). The mouse wheel over
  an audio track's output meter changes its range
  (`UiSettings.track_meter_range`, `MeterRange`); its Use Vertical
  Meters off lays the meters across the header under the name
  (`UiSettings::track_meters_across`, `paint_meter_across`, the rows
  below moved down). Minimize and Maximize remember the height a track
  had (`TimelineView::track_restore`), which the same button or a
  minimized track's Restore Track Height brings back; ` minimizes every
  track and again brings them back (those minimized alone stay so),
  Ctrl+` sets every track at the default height; a header's bottom edge
  dragged sizes every track selected with it alike (`resize_tracks`). The
  Video Preview's navigation (wheel zoom toward the pointer, pan with the
  wheel held, double-click to 100 %) and shortcut menu are in
  `tabs/preview.rs`; its bars, scaling and optimal playback are
  `UiSettings.preview`.
- The welcome screen is `welcome.rs` (`MenuDialogs.welcome`; shown on
  the first start: `UiSettings::load` defaults `show_welcome` to on when
  there is no settings file). Getting Started's places are BananaV's own —
  its website, GitHub, Discord and donations — Ko-fi, from any country,
  and PIX, from Brazil, side by side (`Channel::places`) —, each card
  showing its address and opening it in the browser (`place_card`).
- `unsafe` is denied workspace-wide; FFI modules opt in locally with
  `#[allow(unsafe_code)]`.

## The interface's windows

- The main window's title bar is BananaV's (`title_bar.rs`; the system's is
  off, `with_decorations(false)` in `run`), laid out as VEGAS 23's: the
  program's icon, the menus beside it (`menus::menu_bar`, spaced as
  VEGAS's), the window's title in the middle and the window's buttons at
  the right; dragged it moves the window (`ViewportCommand::StartDrag`, so
  the system's snapping works), double-clicked it maximizes or restores
  it, right-clicked it has the window's menu, and the window's edges size
  it while it is neither maximized nor full screen (`resize_edges`,
  `BeginResize`). The status bar under the windows is VEGAS's too: 20
  pixels, apart from them by the docking area's dark.
- Dialogs and tool windows (Preferences, Customize Keyboard/Toolbar, FX and
  Pan/Crop, Render As, Project Properties…) are real OS windows, like VEGAS's:
  build them with `crate::floating::Window`, which has the `egui::Window`
  builder they need. It opens an egui viewport owned by the main window
  (stays in front of it, minimizes with it, no taskbar button; made owned
  it is brought in front, `bv_platform::set_window_owner`: the main window
  may have come up over it meanwhile — at the program's start, as the
  welcome screen, it shows after its first frame) and falls back
  to an `egui::Window` where the backend embeds viewports (the headless
  tests). Do not use `egui::Window` directly for dialogs.
- Never draw into a hidden OS window (wgpu stalls on Windows): dialogs open
  visible and take their measured size on the next frame.
- A modal dialog (the render's progress, as VEGAS's) blocks the rest of
  the program while it is up: `floating::set_modal` names the dialogs that
  still take input (the context keeps them; `floating::shield` covers
  every other window's contents, and the main window's), the OS refuses
  the other windows input (`bv_platform::block_input`, Windows), and the
  app skips the keyboard's shortcuts, the joystick and the control
  surfaces (`BananaApp::follow_modal`). Unblock before the modal window
  closes, or the OS gives the focus to another program.
- Dialogs are immediate viewports: each of their frames is a frame of the
  whole application. A window moved does not repaint (BananaV's change to
  egui-winit, `vendor/egui-winit/BANANAV.md`), so dialogs follow the
  pointer when dragged; at most one frame waits to be shown
  (`SurfaceConfig::LOW_LATENCY`). `BV_FRAME_TRACE=1` prints each frame of
  the main window (when it started, how long it took, the dialogs
  included) to find out what slows the interface down.

## Platforms

- On Windows the interface draws through DirectX 12 (OpenGL where there
  is none; `ui-egui`'s `interface_graphics`): with monitors on two
  graphics cards, Windows copies the window between them itself, so it
  moves smoothly from one monitor to the other. `WGPU_BACKEND` picks
  another.
- Starts that end early (`ui-egui`'s `safe_start`): some drivers fail on
  DirectX 12 and BananaV closed as its window appeared. Each start is
  noted in `Start.json` (the settings folder) before the window opens and
  crossed out once the window drew and the card compositing video opened
  and compiled its kernels (`Engine::gpu_starting`), or when BananaV
  closes. A start left noted makes the next one draw through the next
  interface — Windows: Vulkan for the interface and the video card
  (`bv_gpu::set_interfaces`), then OpenGL with the processor compositing;
  Linux: OpenGL — and the one that works is kept (deleting `Start.json`
  goes back). `Crash.log` in the logs folder gets the panics, the window's
  failures and, on Windows, crashes no code handled (`bv_platform::
  on_crash`: one line, the exception and its module, written without
  taking memory); a crash while starting there starts BananaV again at
  once on the next interface.
- Updates (`ui-egui`'s `updates`): the public repository's latest
  release asked of GitHub's API through the system's `curl` (no network
  library in BananaV), as BananaV starts and from the Help menu; on
  Windows, installed by its installer, the setup is downloaded to the
  temporary folder, checked against the SHA-256 GitHub gives and run as
  BananaV closes (`install_on_exit`).
- Letters: egui's fonts have Latin, Greek and Cyrillic; behind them the
  interface adds the computer's Chinese, Japanese and Korean faces and
  one with many other scripts (`interface_fonts`, found by
  `bv_effects::interface_fallbacks`), read on a thread of their own (at
  once for an Asian interface language). Titles & Text draws a letter its
  font lacks from a face that has it (`fonts::fallback`: the usual
  families of the text's language — kana makes it Japanese, hangul Korean
  —, then wide ones, then any face whose `OS/2` table says it covers the
  block), each such face a style of its own.
- Windows x64 is the development computer; Windows ARM64 and Linux (x64
  and ARM64) are checked on GitHub's machines (`.github/workflows/ci.yml`),
  macOS (Apple's chips and Intel) there too when asked (`-f macos=true`).
  Started by hand, the CI can check one crate (`gh workflow run ci.yml
  --ref <branch> -f crate=bv-mf`; `-f ignored=true` adds its oracle
  tests, with the ffmpeg tool installed).
- Linux: media through FFmpeg 5.1 or newer (`crates/mf/src/ff`, the API
  and rules of the Windows modules: libavformat, libavcodec, libswscale;
  decoding on the cards through VA-API or NVIDIA's decoder, NVENC and
  VA-API encoders, `hw.rs` lists the cards from `/dev/dri`); sound and MIDI
  through ALSA; the OS services of `bv_platform` from the kernel and from
  X11 (`crates/platform/src/x11.rs`, libX11 and libXrandr loaded at run
  time). BananaV's windows are X11 ones, through XWayland under Wayland
  (`ui-egui`'s `run`): `BANANAV_WAYLAND=1` gives Wayland's own, which
  cannot place dialogs nor see the mouse, keypad or screen outside them.
  `BV_FFMPEG_LOG=warning` (`info`, `debug`) shows FFmpeg's messages.
- macOS (11 and later; one app for Apple's chips and Intel): media
  through FFmpeg as on Linux (`crates/mf/src/ff`), the Mac's media engine
  through VideoToolbox (`hw.rs`: one "card", named after the chip,
  decoding H.264, HEVC and what else it takes; `encode.rs`: the H.264 and
  HEVC encoders, `Vendor::Apple`, "Apple VideoToolbox", constant quality
  on Apple's chips); sound through Core Audio (cpal); the interface drawn
  by Metal (`every_kernel_translates_to_metal` checks every kernel in
  Metal's language from any system). `bv_platform`'s services are
  `mac.rs` — AppKit through objc2, on the main thread: the program's
  windows (`NSWindow`; dialogs as child windows of the main one), the
  mouse, the displays' names, the Trash, the system's color sampler for
  the eyedropper (`start_color_sampler`), Quit and the files Finder
  opens (the window library's application delegate gets
  `applicationShouldTerminate:` and `application:openURLs:` as it
  finishes launching, `before_run`; the interface reads
  `take_quit_request` and `take_opened_files`, woken by `set_wake`); Core
  Graphics for the displays and the Screen Capture window (`screen.rs`);
  the keyboard layouts (Text Input Sources) for the keys' labels and a
  watch on the program's key events for the keypad; LaunchServices for
  project files; Disk Arbitration for removable drives; GameController;
  Core MIDI (`midi.rs`); IOKit's disk and graphics statistics and the
  Dock's badge for a render's progress (`monitor.rs`). The main window
  keeps the system's buttons at the left of BananaV's bar (`lib.rs`' `run`:
  a clear title bar over the content; `title_bar.rs` leaves them room and
  has no buttons or edges of its own); BananaV's Ctrl is the Command key
  (the interface reads `Modifiers::command`), a Control-click is a right
  click (`keymap::mac_control_click`) and the texts name the keys Cmd and
  Option (`i18n::system_keys`). Plug-ins are bundles: `bv-apple` loads
  them through `CFBundle` (VST 3's `bundleEntry`, VST 2's `VSTPluginMain`
  or `main_macho`, OpenFX's `MacOS` folder), reads Mach-O files
  (`macho.rs`; a plug-in made for the other processor is refused with the
  reason — BananaV opened through Rosetta runs Intel-only ones) and guards
  every call into them (`guard.c`: signal handlers put in before a
  plug-in's code loads, a crash coming back as the signal and its place,
  `cfg(bv_vst3_guard)` and its kind as on Windows); their editors are
  NSViews inside the FX window (`EmbeddedWindow`, sized in points,
  `EDITOR_POINTS`) or windows of their own (`PluginWindow`). The scans'
  child processes call plug-ins on their main thread, where Mac plug-ins
  expect their first calls. There is no 32-bit bridge, CUDA or ASIO on a
  Mac; burning discs and capturing from cameras are not done there yet.
  From another system `sh tools/mac_check.sh` runs clippy for macOS
  (`MAC_TARGET=x86_64-apple-darwin` for Intel Macs).
- macOS's package (`packaging/macos`): `build_ffmpeg.sh` builds FFmpeg
  under the LGPL for one processor (VideoToolbox, AudioToolbox, dav1d,
  LAME, SVT-AV1; macOS 11), `make_app.py` puts each processor's program
  and libraries together in `BananaV.app` (`lipo`; the libraries in
  Contents/Frameworks, found through `@rpath`), with `Info.plist` (bundle
  identifier `io.github.bananav-editor.BananaV`, the `.bvproj` files'
  type, the permissions' texts in English and Brazilian Portuguese),
  `BananaV.icns` and `BananaV.entitlements`, signs it (ad hoc, or with a
  Developer ID and notarized when the secrets are there) and makes the
  disk image.
- Building on Linux needs (Ubuntu's names) `libasound2-dev pkg-config
  libclang-dev libavcodec-dev libavformat-dev libavutil-dev
  libswscale-dev libswresample-dev`.
- From Windows, `sh tools/linux_check.sh` runs clippy for Linux (all of
  it, or `-p crate …`): FFmpeg's declarations come from the CI's
  `ffmpeg-bindings` artifact (no FFmpeg headers here); without them it
  checks with `--cfg bv_no_ffmpeg`. `LINUX_TARGET=aarch64-unknown-linux-gnu`
  checks ARM64.
- Packages: `gh workflow run package.yml` makes, as the run's artifacts
  (nothing is published), zips and installers for Windows x64 and ARM64 (the
  C runtime inside; `packaging/windows/bananav.iss`, Inno Setup, the installer
  tried out on the runner), macOS's universal disk image (each processor
  built on its own Mac, put together, started on the runner and its screen
  kept as a picture) and AppImages for Linux x64 and ARM64, built on Ubuntu 22.04
  with an FFmpeg built under the LGPL (`packaging/linux/build_ffmpeg.sh`:
  no x264 or x265 while BananaV's license is open). The icon is BananaV's
  official logo (`packaging/icons`: `bananav-icon.svg`, the rounded icon;
  `bananav-logo.svg`, the logo on its dark square; `bananav-logo-transparent.svg`;
  and `bananav.png`, the rounded icon at 256 px, which the AppImage and the
  main window use: `window_icon` in `crates/ui-egui/src/lib.rs`).

## User settings

- User settings live in `%APPDATA%\BananaV\`: `interface.json` (recent
  projects, layouts, language, toolbars, saved video templates, settings for
  new projects), `teclado.json` (keyboard maps), `predefinicoes.json`
  (effect presets), `LUTs\` (BananaV's looks, LUT Filter's presets'
  tables), `Layouts\` (window layouts, `.bvlayout`), `FX Packages\`
  (filter packages, `.bvfx`).
