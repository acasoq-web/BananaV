# Industry standards

The open standards of the video industry that VEGAS builds on and that suit
BananaV, the ones that come with licenses, fees or restrictions, and the free
alternatives to those. Checked in October 2026 (sources at the end).

Licensing changes over time: check a line again before relying on it, and
ask a lawyer before BananaV is sold or published. This is a developer's map,
not legal advice.

BananaV today: **Done** · **Partly** · **Greyed** (shown as "Coming soon") ·
**Not yet**.

## 1. Free standards VEGAS uses

### Plug-ins

| Standard | Terms | BananaV today |
|---|---|---|
| **OpenFX (OFX) 1.5** — video effects, transitions and generators from other makers (Boris FX, Neat Video, NewBlue, RE:Vision…). Also hosted by DaVinci Resolve/Fusion, Nuke, Natron and Baselight; not by Premiere/After Effects (Adobe's own SDK), Final Cut (FxPlug) or Avid (AVX). VEGAS 23 adds timeline envelopes for OFX effects (beta). | BSD-3-Clause, hosted by the Academy Software Foundation. No fees. | **Done** on the processor: effects (event, media, track, output), transitions and generators; parameters with keyframes and presets; overlays (draw suite); temporal clip access. Tested with Natron's free plug-ins (openfx-misc, openfx-io, CImg, G'MIC, Arena, Shadertoy). BananaV's own effects are OFX plug-ins too (Crop, Color Curves, Color Corrector, Cookie Cutter, Layer Dimensionality, Pixelate), also built as a `BananaV.ofx` bundle for other hosts; BananaV draws them on the graphics card itself. OFX 1.5's CUDA render on Windows with NVIDIA cards: plug-ins that render with CUDA (Sapphire) draw on the card's own pictures, Direct3D 12 buffers shared with CUDA. Overlays that draw with OpenGL themselves are shown (Windows). Not yet: OpenGL, OpenCL and Metal render, CUDA on Linux, parameters' own interacts, Nuke's and Natron's multi-plane and multi-view extensions, "Show OFX Effect Envelopes" (greyed). |
| **VST3 3.8** — audio effects and instruments. | MIT since October 2025 (it was proprietary or GPLv3 before). No fees. | **Partly** (awaiting the user's review): audio effects hosted (`bv-vst3`, written from the MIT headers; notice shipped): run in the mixer's chains, their parameters and states kept in projects, BananaV's generic editor and the plug-in's own window on Windows. Still to come: the plug-in's window on Linux (X11 embedding), instruments, presets through BananaV's preset bar, parameter automation. |

What hosting OFX takes (and how BananaV does it):

- Plug-ins are native libraries (`.ofx` bundles) that run inside BananaV's
  process: one that crashes takes BananaV down (as in VEGAS and Resolve).
  BananaV describes each binary in a process of its own when it starts, so
  one that crashes there is only listed as failed. The FFI modules opt
  into `unsafe` locally.
- They take RGBA images of 8, 16 or 32 bits per channel; BananaV composites
  in linear, premultiplied 32-bit float and converts at the plug-in's edges
  (OFX 1.5 has a color management suite for this).
- Their graphics card paths are CUDA, OpenCL, Metal or OpenGL, none of which
  meets wgpu directly. CUDA does through Direct3D 12's shared resources:
  on Windows, where NVIDIA's driver is, BananaV says it renders with CUDA
  and hands plug-ins that do (Sapphire) its own buffers, imported into
  CUDA. `bv-cuda` calls the installed driver's API (`nvcuda.dll`),
  declared from NVIDIA's documentation: no license is needed to call it,
  and nothing of NVIDIA's (the CUDA Toolkit's headers or libraries) is
  used or shipped. Other plug-ins run on the processor (pictures copied to
  and from the card when it composites there). On-screen controls
  (overlays) are drawn with OFX 1.5's draw suite; overlays that draw with
  OpenGL themselves are drawn in an offscreen OpenGL context of the card's
  (Windows) and laid over the preview.
- Almost no commercial plug-in ships ARM64 builds: Windows ARM64 and Linux
  ARM64 users would have few or none.
- Some commercial plug-ins check which host loads them and are supported
  only in the hosts their makers list. VEGAS's own OFX plug-ins are of
  these: besides checking the host, they ask for VEGAS's private suites
  (`OfxVegasEffectSuite`, `OfxVegasKeyframeSuite`, `OfxHWndInteractSuite`…),
  which have no public documentation; BananaV does not imitate VEGAS's
  identity or reverse engineer them, so most of them refuse to run in it
  (`crates/ofx/tests/vegas_poc.rs` measures this on the user's copy).
  VEGAS 23's 59, tried in October 2026: 51 refuse to describe themselves,
  Titles & Text to make an instance, LAB Adjust and Color Grading to
  render; the 360° ones (Scene Rotation, Dual-Fish-Eye Stitching, 360°
  Video Stabilization) say the host is not supported and leave the
  picture as it was, and Smart Zoom and Smart Upscale, which render
  through VEGAS's private OpenCL suite, leave it black.
- Free plug-ins to test with: the OFX SDK's examples and Natron's
  (openfx-misc, openfx-io).
- BananaV's own effects are written as OFX plug-ins (`crates/effects`),
  as VEGAS's are: BananaV hosts them built in, and `apps/ofx-bundle`
  builds them as `BananaV.ofx`, which other OFX hosts load. On the
  graphics card BananaV draws them itself (the same math as kernels);
  others' plug-ins run on the processor until BananaV hosts a graphics
  card suite.

### Color and HDR

| Standard | Terms | BananaV today |
|---|---|---|
| **ACES** (VEGAS 23: ACES 1.3; its feature page still says 1.2) — the Academy Color Encoding System. | The Academy's license: royalty-free right to copy, modify and use in source and binary form, keeping the notice. | **Done (1.3, SDR)**: File › Properties › Video's ACES version, color space (ACES2065-1 or ACEScg), view transform (VEGAS's lists: with 1.2, sRGB, Rec.709, Rec.2020 and P3D65 drawn; with 1.3, the SDR Video of sRGB, Display P3, Rec.1886 Rec.709 and Rec.2020, P3-D65's SDR Cinema, and the un-tone-mapped and raw views) and Look (with 1.3 the Reference Gamut Compression, with 1.2 the Blue Light Artifact Fix), with 32-bit floating point (full range) in linear light or with the 2.222 gamma (the pictures' values as ACES's light), as VEGAS's; the RRT, the SDR ODTs and the Looks ported from the Academy's CTL (aces-dev 1.3), their license shipped (`ACES-license.md`). Worked out exactly on the processor and the graphics card (a 65³ lattice misses near AP1's edge). Each media's color space (Media Properties) and a render template's (its Project tab): VEGAS's list, ACES's and the cameras' spaces (ARRI, Sony, Panasonic, Canon, RED, Blackmagic, DaVinci; linear and log). Still to come: HDR outputs (ACES 1.1's single-stage tone scale) and VEGAS's other views (gamut-limited, D60 simulation, DCDM, P3-D60, Log), the ACEScc, ACEScct and ACESproxy working spaces, the ADX10/16 encodings. |
| **OpenColorIO 2** — color management library and configurations (the ACES ones among them). | BSD-3-Clause (Academy Software Foundation). | **Partly, BananaV's own implementation** (`bv-ocio`, no OpenColorIO code): it reads OpenColorIO configurations (profile versions 1 and 2: color spaces, roles, aliases, displays and views, shared views, view transforms, looks, file rules) and works out their transforms — matrices, exponents, logs and camera logs, CDLs, ranges, LUT files (`.cube`, `.spi1d`, `.spi3d`, `.spimtx`) and the builtins of ACES 1.3, the cameras (ARRI LogC3 and LogC4, S-Log3, V-Log, Log3G10, Apple Log) and the displays. BananaV's own ACES 1.3 configuration (`crates/ocio/configs`) drives the project's color management; loading another configuration comes later. |
| **Camera log encodings and gamuts** — ARRI LogC3/LogC4 and Wide Gamut 3/4, S-Log3 and S-Gamut3(.Cine), V-Log/V-Gamut, Log3G10/REDWideGamutRGB, Apple Log, Blackmagic Film Gen 5, DaVinci Intermediate. | Published by their makers for interchange (white papers, ACES IDTs); the names are their trademarks, used only to say which format a file is, as OpenColorIO's ACES configurations do. | **In the ACES configuration** (`bv-ocio`: curves and matrices checked against the makers' published IDTs); offered in the interface with Media Properties' color space. |
| **LUTs (`.cube`)** — 1D/3D look-up tables; VEGAS imports, exports and keeps presets of them. | Free, documented format. | **Partly**: `bv-lut` reads them (Adobe's 1D and 3D, Resolve's shaper before a lattice; trilinear or tetrahedral), and the LUT Filter effect applies them, on the processor and the graphics card. Still to come: exporting them. |
| **PQ (SMPTE ST 2084) and HDR10** (static metadata, SMPTE ST 2086). | Free to implement. | **Partly**: the curves are in `bv-color`; HDR project settings greyed. |
| **HLG** (ARIB STD-B67, ITU-R BT.2100), broadcast HDR by the BBC and NHK. | Royalty-free. | **Partly** (as PQ). |

### Interchange

| Standard | Terms | BananaV today |
|---|---|---|
| **EDL** (CMX 3600 cut lists). | De facto standard, free. | **Done** (import and export). |
| **Final Cut Pro 7 XML**. | Documented by Apple, free to read and write. | **Done** (import and export: Final Cut Pro 7's, DaVinci Resolve's, Premiere's and VEGAS's files — cuts, crossfades and fades, stereo pairs and a file's streams, links, opacity and gain, speeds, titles, colors, nested sequences, markers; Pan/Crop still to come; FCPXML too). |
| **OpenTimelineIO** (OTIO, Academy Software Foundation): `.otio`, and the bundles `.otioz` and `.otiod`. | Apache-2.0; the file format is a published specification. | **Done** (import and export, written from the specification, no OTIO code): timelines to and from DaVinci Resolve (18.5+), Premiere Pro (25.6+) and Kdenlive — cuts, timing, crossfades and fades, speeds, markers; BananaV's own projects whole through its metadata. |

AAF, which VEGAS also imports and exports, has a license of its own: see
section 2.

### Audio

| Standard | Terms | BananaV today |
|---|---|---|
| **EBU R 128 / ITU-R BS.1770** loudness. | Free. | **Done**: the loudness scan and log, and the Loudness Meters (true peaks four times oversampled, BS.1770-4's Annex 2). |
| **ATSC A/85** loudness for television (−24 LUFS, true peaks to −2 dBTP). | Free (ATSC recommended practice). | **Done**: the Loudness Meters' ATSC A/85 mode. |
| **ITU-R BS.775** downmix (5.1 to stereo: center and surrounds 3 dB down, LFE out). | Free (ITU recommendation). | **Done**: the Master Bus's Downmix Output (playback only). |
| **IEC 60268-17** VU and **IEC 60268-10** PPM ballistics (VU: 99 % in 300 ms; PPM Type I: 5 ms, 20 dB in 1.7 s; Type II: 10 ms, 24 dB in 2.8 s); **EBU R68** alignment (0 dBu at −18 dBFS). | The IEC sells the documents; implementing them is free. EBU R68 is free. | **Partly** (awaiting the user's review): the Mixing Console's VU Meters — Traditional, Extended and Logarithmic VU, UK, EBU, DIN and Nordic PPM — computed in the mixer (`bv_mixer::meters`); the alignment is BananaV's choice until the user confirms VEGAS's. |
| **Audio CD (Red Book, IEC 60908)** — 75 frames a second, tracks and indexes. | The format is public knowledge; the standard document is sold by the IEC. | **Partly**: the Audio CD Time ruler format; CD layout and burning greyed. |
| **Broadcast Wave (BWF), RF64** (EBU). | Free. | **Done**. |
| **Wave64**, **AIFF**. | Public specifications. | **Done**. |
| **FLAC**. | Free (Xiph.Org). | **Done** (BananaV's own encoder). |
| **MP3**. | Patents expired in 2017; the licensing program ended. | **Done** (the system's codecs). |
| **Ogg Vorbis**. | Royalty-free (Xiph.Org). | **Partly**: read where the system has the codec (FFmpeg on Linux); rendering greyed. |
| **MIDI 1.0, MIDI Time Code, MIDI clock**. | Free specifications (MIDI Association). | **Done**. |
| **SMPTE timecode** (ST 12-1: hours, minutes, seconds and frames; drop-frame counting at 29.97 and 59.94) and media timecode — QuickTime's timecode track (`tmcd`) in MP4/MOV, the Broadcast Wave time reference. | The format is free to implement; the SMPTE document is sold. | **Partly**: the ruler's SMPTE formats; media timecode read from MP4/MOV and BWF files to line multicamera events up (Tools › Multicamera); MXF's, AVCHD's and DV's, and Media Properties' timecode, not yet. |
| **Mackie Control** (control surfaces). | De facto protocol. | **Done**. |

### Video, containers and pictures

| Standard | Terms | BananaV today |
|---|---|---|
| **MPEG-2** video (DVD, Blu-ray, XDCAM). | The last US patent expired in February 2018 (a few countries later). | **Partly**: read; rendering greyed. |
| **AV1** — royalty-free video (Alliance for Open Media). See the note in section 2. | Royalty-free license from AOMedia's members. | **Done** (rendering on cards with an AV1 encoder; reading through the system's codecs). |
| **MP4 / QuickTime MOV** containers; **MXF**; **Matroska / WebM**. | Free to implement (the codec inside may not be). | MP4/MOV: **Done** (BananaV's own writer). MKV/WebM: read. MXF: **Not yet** on Windows. |
| **PNG, JPEG, TIFF, BMP, DPX, OpenEXR, DDS, JPEG XR**. | Free (OpenEXR: BSD-3-Clause; JPEG XR: BSD library and Microsoft's patent promise). | **Done**. |
| **OpenType / TrueType fonts** (ISO/IEC 14496-22, Open Font Format) — the computer's fonts for Titles & Text. | Free standard; each font has its own license (BananaV only draws the user's installed fonts, ships none). | **Done**: TrueType and CFF outlines through `skrifa` (MIT or Apache-2.0); no kerning or complex shaping yet. |
| **CSV (RFC 4180)** and tab-separated text — subtitles to import. | Free. | **Done**: Insert Subtitles from File. |
| **360° video** (spherical metadata, Google's Spatial Media). | Apache-2.0. | **Partly** (awaiting the user's review): equirectangular mono 360° projects; MP4 and MOV renders carry Spherical Video V2's `st3d` and `sv3d` boxes and V1's XML `uuid` box, written from the RFCs; importing reads either (and a photo's GPano XMP) to find 360° media; the Video Preview's 360° View. Still to come: stereoscopic 360°, cube maps and meshes, spatial audio (`SA3D`). |
| **Closed captions** (CEA-608/708). | Free to implement. | **Partly** (awaiting the user's review): CEA-608 caption markup on command markers, read from Scenarist (`.scc`), SubRip, SAMI, RealText, QuickTime text, transcripts and DVD subtitles, written as SubRip, SAMI, QuickTime text and DVD subtitles, drawn over the Video Preview. Still to come: captions inside rendered video (608/708 in the stream), MCC files. |

### Graphics cards and hardware encoders

| Standard | Terms | BananaV today |
|---|---|---|
| **Direct3D 12, Vulkan, Metal** (BananaV draws through wgpu; VEGAS 23 uses DirectX 11/12). | Part of the systems and drivers. | **Done**. |
| **NVENC/NVDEC, AMD AMF, Intel Quick Sync** (oneVPL). | Free SDKs from the card makers. | **Done** (through Media Foundation on Windows, FFmpeg on Linux). |

## 2. Standards with licenses, fees or restrictions

| Standard | What it takes | BananaV today | Free alternative / way around |
|---|---|---|---|
| **H.264 / AVC** | Patent pool of Via Licensing Alliance (MPEG LA joined it in 2023): the first 100,000 units a year free, then $0.20, then $0.10 a unit, with a yearly cap. | Encoded and decoded by Windows (Microsoft's licensed codecs) and the graphics cards; the Linux AppImage carries FFmpeg's decoder (no x264). | AV1. **OpenH264** (BSD): Cisco pays the H.264 royalties for its binaries when the program downloads them from Cisco (they cannot be bundled). Keep using the system's and the cards' codecs. |
| **HEVC / H.265** (and HEIC pictures) | Several pools: Access Advance's HEVC Advance (29,000+ patents; a royalty per software copy) — since December 2025 it also runs Via LA's former HEVC/VVC pool (now "VCL Advance") — and companies outside the pools. | Through Windows (its HEVC extension or the card) and the cards; FFmpeg's decoder on Linux. | AV1. |
| **AAC** | Via LA's AAC pool still licenses AAC-LC, HE-AAC and xHE-AAC (some say the US AAC-LC patents have expired: unconfirmed). | Through Windows; FFmpeg on Linux. | **Opus** (royalty-free); PCM or FLAC for masters. |
| **AV1, VP9** | Royalty-free from AOMedia and Google, but Sisvel runs AV1/VP9 pools, Access Advance is exploring an AV1/AV2 pool, and there are lawsuits (Dolby v. Snap, InterDigital v. Amazon). | AV1 rendering on the cards. | Still the best free bet for video; watch the lawsuits. |
| **Apple ProRes** | Apple licenses ProRes through its ProRes Program Office and lists authorized products; it warns that unauthorized implementations (FFmpeg's, for example) may decode wrongly. "Apple ProRes" is Apple's trademark. | **BananaV's own encoder and decoder** (from SMPTE RDD 36), and the Render window says "Apple ProRes". | Apply to Apple's program; or offer **CineForm** (free, see section 3) as the editing codec and keep ProRes reading. Decide before 0.1.0. |
| **Avid DNxHD / DNxHR** (SMPTE VC-3) | Commercial use needs Avid's approval; the decoder's patents on RAND terms; selling a product as "DNxHD" means Avid's testing (reported fee). Commercial SDK: MainConcept. | Recognized; not decoded on Windows (FFmpeg decodes it on Linux). | CineForm; FFV1 for lossless. |
| **AC-3 (Dolby Digital), E-AC-3** | AC-3's patents expired in March 2017; E-AC-3's last US patent reportedly expired in January 2026 (unconfirmed). The names and logos "Dolby" and "Dolby Digital" stay Dolby's trademarks (a license to use them). | **BananaV's own AC-3 decoder**; encoding through Windows' encoder; the interface says "Dolby Digital AC-3". | Name it **"AC-3"** in the interface; Opus or AAC for surround in other containers. |
| **Windows Media** (WMV/VC-1, WMA, ASF) | Microsoft's formats (VC-1 had its own patent pool). | Windows' own encoders and decoders. | Keep it Windows-only. |
| **HDR10+** | Royalty-free, but through a license agreement with HDR10+ Technologies (yearly administration fee for device makers and services; terms for editing software to confirm). | Not yet. | HDR10, HLG. |
| **Dolby Vision, Dolby Atmos** | Dolby's licensed formats. | Not planned. | HDR10/HLG; 5.1/7.1 and Ambisonics (free). |
| **ASIO** (low-latency audio drivers, Windows) | Steinberg: its proprietary license (signed agreement) or, since October 2025, GPLv3 — which would make BananaV's own code GPLv3. | Drivers listed from the registry; **Greyed**. | **WASAPI** exclusive mode (part of Windows, low latency); ALSA/PipeWire on Linux. Or ASIO under Steinberg's proprietary license. |
| **VST2** | Steinberg stopped signing new VST2 licenses in 2018; hosts that had one keep it. | Not planned. | **VST3** (MIT), **CLAP** (MIT). |
| **AAF** (VEGAS imports and exports it) | The AAF SDK has its own "Public Source License" (not MIT/BSD; using the "AAF" name has an adopters' agreement). | **Greyed** (AAF export settings). | Write AAF from the published object specification (Microsoft's compound file format is an open specification); or **OpenTimelineIO**. |
| **Blu-ray and DVD discs** | Blu-ray: the One-Blue patent pool and the Blu-ray Disc Association's format and logo licenses (authoring is costly). DVD: format and logo licensing. | Renders MPEG-2 and AC-3 "for DVD and Blu-ray authoring"; disc burning **Greyed**. | Render disc-ready files and leave authoring to other programs; data discs. |
| **GPL libraries** (x264, x265, FFmpeg's GPL parts) | Copyleft: linking them makes BananaV GPL; x264/x265 also leave the H.264/HEVC patents to BananaV. | Left out: Linux packages carry an FFmpeg built under the LGPL. | Hardware encoders; OpenH264; AV1 encoders **SVT-AV1** and **rav1e** (BSD). |

## 3. Free extras worth considering

| Standard | Terms | Why |
|---|---|---|
| **CLAP** (audio plug-ins, by Bitwig and u-he). | MIT; no fees or membership. | Modern and simple; Rust crates exist; next to VST3. |
| **GoPro CineForm** (SMPTE ST 2073, VC-5). | MIT or Apache-2.0 (open since 2017). | A free editing codec with alpha in place of ProRes and DNxHR. |
| **GL Transitions** — an open collection of transitions written in GLSL (VEGAS 2026 ships them as its GL Transition). | MIT (each transition's author in its file; one, InvertedPageCurl, BSD-3-Clause). | **Done**: 62 of them as BananaV's own OFX transition's presets, their GLSL — and GLSL typed in by the user — run by BananaV's own GLSL interpreter on the processor and translated to WGSL for the graphics card (their authors in `crates/effects/licenses/gl-transitions.md`, shipped with the packages). |
| **lensfun's lens database** — lenses' distortion, vignetting and chromatic aberration calibrations (VEGAS's Lens Correction lists lensfun's lenses). | CC BY-SA 3.0 (data; adaptations may be CC BY-SA 4.0, compatible with GPLv3). | **Done** for distortion: Lens Correction's 636 lenses, the PanoTools coefficients of each one's first calibration in version 0.3.95, adapted as CC BY-SA 4.0 (`crates/effects/licenses/lensfun.md`, shipped with the packages). |
| **FFV1** (IETF RFC 9043). | Free. | Lossless video for masters and archives. |
| **Opus**. | BSD, royalty-free. | Audio in MP4 and WebM without AAC's pool. |
| **dav1d / rav1e** (AV1 decoder / encoder). | BSD-2-Clause. | Reading and writing AV1 on every system without the system's codecs (rav1e is written in Rust). |

## 4. Decisions before 0.1.0

1. **BananaV's own license** (permissive or GPL): it decides whether
   GPL pieces (x264/x265, ASIO under GPLv3) can come in.
2. **ProRes**: Apple's program, or CineForm in its place (and the "Apple
   ProRes" texts in the Render window).
3. **"Dolby Digital"** in the interface: "AC-3" unless Dolby licenses the
   name.
4. **The Linux AppImage** carries FFmpeg's H.264, HEVC and AAC decoders:
   distributing it (above all for sale) may owe those pools; options are
   using the system's FFmpeg, OpenH264 for H.264, or the cards' decoders.
5. **Codecs in pure Rust** (roadmap): writing H.264, HEVC or AAC ourselves
   makes BananaV the one the pools license; prefer AV1, VP9, Opus and FLAC,
   and leave the patented ones to the systems and the cards.
6. A lawyer's review before selling.

## 5. A suggested order

1. OFX on the processor, with keyframes, presets and overlays (**done**);
   commercial plug-ins' trial versions (Sapphire's **done**); OFX 1.5's
   CUDA render on Windows (**done**); then OpenGL, OpenCL and Metal render
   and CUDA on Linux.
2. VST3 hosting for audio (**effects done**, awaiting review; next its window on Linux,
   instruments and automation), then CLAP.
3. OpenColorIO + ACES + `.cube` LUTs (**done**: ACES 1.3 for SDR; next, HDR and each media's color space).
4. OpenTimelineIO (**done**), then AAF.
5. HDR output (PQ/HLG with HDR10 metadata), 360° metadata (**done**, awaiting
   review), closed captions (**files done**, awaiting review; next, inside
   rendered video).

## Sources

- VEGAS Pro 23: [What's new](https://www.vegascreativesoftware.com/int/new-features),
  [features](https://www.vegascreativesoftware.com/za/features/),
  [Digital Production](https://digitalproduction.com/2025/09/10/vegas-pro-23-boosts-engine-streamlines-colour-grading-modernises-interface/)
- OpenFX: [Wikipedia](https://en.wikipedia.org/wiki/OpenFX_(API)),
  [license (SideFX)](https://www.sidefx.com/docs/houdini/licenses/openfx.html),
  [ASWF](https://www.aswf.io/tag/openfx/)
- VST3 and ASIO: [Steinberg press release](https://ocl-steinberg-live.steinberg.net/_storage/asset/819253/storage/master/Press%20Release%20-%202025-10-29%20-%20VST%203.8%20-%20EN.pdf),
  [Sonicstate](https://sonicstate.com/news/2025/10/30/vst-3-now-available-under-mit-license),
  [heise](https://heise.de/-10963451)
- CLAP: [Wikipedia](https://en.wikipedia.org/wiki/CLever_Audio_Plug-in),
  [Bitwig](https://bitwig.com/stories/clap-the-new-audio-plugin-standard-201/)
- OpenColorIO: [license](https://opencolorio.readthedocs.io/en/main/aswf/license.html),
  [ACES studio configuration](https://opencolorio.readthedocs.io/en/main/configurations/aces_studio.html);
  ACES: [aces-dev 1.3](https://github.com/ampas/aces-dev/tree/v1.3), [its license](https://raw.githubusercontent.com/ampas/aces-dev/v1.3/LICENSE.md)
- HLG: [ARIB STD-B67](https://www.arib.or.jp/english/std_tr/broadcasting/desc/std-b67.html),
  [Wikipedia](https://en.wikipedia.org/wiki/Hybrid_log%E2%80%93gamma);
  HDR10+: [Wikipedia](https://en.wikipedia.org/wiki/HDR10%2B),
  [Display Daily](https://displaydaily.com/hdr10-continues-to-gain-momentum/)
- H.264: [Via LA](https://www.via-la.com/licensing-2/avc-h%20264/),
  [MPEG LA](https://en.wikipedia.org/wiki/MPEG_LA);
  HEVC: [Access Advance](https://accessadvance.com/topic-what-do-we-license/),
  [rate extension](https://www.businesswire.com/news/home/20260126799824/en/Access-Advance-Extends-HEVC-Advance-Rate-Increase-Deadline),
  [AV1/AV2 pool](https://secure.businesswire.com/news/home/20260825292774/en/Access-Advance-Launches-Exploration-Phase-for-an-AV1AV2-Device-and-Software-Patent-Pool-Invites-Participation-of-Potential-Licensors)
- AAC: [Via LA](https://via-la.com/via-licensing-alliance-welcomes-honor-as-advanced-audio-coding-patent-pool-licensee/),
  [Fraunhofer FDK AAC](https://en.wikipedia.org/wiki/Fraunhofer_FDK_AAC)
- AV1: [Sisvel pool](https://www.streamingmediaglobal.com/Articles/News/Featured-News/Sisvel-Announces-AV1-Patent-Pool-139634.aspx),
  [Dolby v. Snap](https://www.techspot.com/news/111865-dolby-sues-snap-over-video-compression-patent-claims.html)
- ProRes: [Apple's authorized products](https://support.apple.com/118584),
  [white paper](https://www.apple.com/euro/final-cut-pro/f/generic/docs/Apple_ProRes_White_Paper.pdf)
- DNxHD/DNxHR: [Wikipedia](https://en.wikipedia.org/wiki/Avid_DNxHD),
  [FFmpeg list](https://ffmpeg.org/pipermail/libav-user/2016-January/008749.html)
- CineForm: [Wikipedia](https://en.wikipedia.org/wiki/CineForm)
- AC-3 / E-AC-3: [Gigazine](https://gigazine.net/gsc_news/en/20170321-ac-3-patent-expired),
  [Phoronix](https://www.phoronix.com/news/Dolby-Digital-Plus-E-AC3-2026)
- MPEG-2 and MP3: [TechRepublic](https://www.techrepublic.com/article/mpeg-2-patent-expiration-opens-door-for-royalty-free-use/),
  [LWN](https://lwn.net/Articles/737834)
- AAF: [AAF SDK Public Source License](https://aaf.sourceforge.net/docs/AAFSDKPSL.html)
- OpenTimelineIO: [Arch package (license)](https://archlinux.org/packages/extra-staging/x86_64/opentimelineio),
  [Premiere beta](https://community.adobe.com/t5/premiere-pro-beta-discussions/now-in-beta-otio-import-and-export/m-p/14985932),
  [Remotion](https://www.remotion.dev/docs/export-opentimeline)
- Blu-ray: [One-Blue licensees](https://www.storagenewsletter.com/?p=144466)
