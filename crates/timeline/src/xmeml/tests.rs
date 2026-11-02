use bv_core::{FrameRate, SampleRate, Tick};

use super::{export, read};
use crate::fcp_import::TITLES_PLUGIN;
use crate::fx::{FadeCurve, PluginFx, PluginParam, PluginValue, Transition};
use crate::model::{
    AudioStreamInfo, ChannelMap, Clip, Generator, Marker, MarkerId, MediaId, MediaInfo,
    MediaSource, Project, ProjectSettings, Region, TrackKind, VideoStreamInfo,
};
use crate::otio::{self, DocChannels, DocEvent, Document, MediaRefSource};

fn s(x: i64) -> Tick {
    Tick::from_seconds(x)
}

/// A file with video and `streams` stereo audio streams, 20 s long.
fn file(p: &mut Project, path: &str, streams: usize) -> MediaId {
    let stereo = AudioStreamInfo {
        sample_rate: SampleRate::HZ_48000,
        channels: 2,
    };
    p.add_media(MediaInfo {
        id: MediaId(0),
        source: MediaSource::File { path: path.into() },
        name: path.rsplit('\\').next().unwrap().into(),
        duration: s(20),
        video: Some(VideoStreamInfo {
            width: 1920,
            height: 1080,
            frame_rate: p.settings.frame_rate,
            pixel_aspect: Default::default(),
            field_order: Default::default(),
            spherical: false,
            full_range: false,
            reading: Default::default(),
        }),
        audio: (streams > 0).then_some(stereo),
        extra_audio: vec![stereo; streams.saturating_sub(1)],
        sequence: None,
        still: false,
        effects: Vec::new(),
        markers: Vec::new(),
        color_space: None,
        reel: Default::default(),
        layer: None,
    })
}

/// A document's tracks of `kind`, each its events' start, length and
/// media time in whole frames of `rate`.
fn placed(doc: &Document, kind: TrackKind, rate: FrameRate) -> Vec<Vec<(i64, i64, i64)>> {
    doc.tracks
        .iter()
        .filter(|t| t.kind == kind)
        .map(|t| {
            t.events
                .iter()
                .map(|e| {
                    (
                        rate.frame_at(rate.snap_nearest(e.start)),
                        rate.frame_at(rate.snap_nearest(e.duration)),
                        rate.frame_at(rate.snap_nearest(e.source)),
                    )
                })
                .collect()
        })
        .collect()
}

fn audio(doc: &Document) -> Vec<&Vec<DocEvent>> {
    doc.tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Audio)
        .map(|t| &t.events)
        .collect()
}

/// Rate 60's `<rate>`.
const R60: &str = "<rate><ntsc>FALSE</ntsc><timebase>60</timebase></rate>";

/// A VEGAS clip item: its id, master clip, timing, file, filters, links.
fn vegas_item(id: &str, master: &str, timing: (i64, i64, i64, i64), more: &str) -> String {
    let (start, end, i, o) = timing;
    format!(
        "<clipitem id=\"{master}{id}\"><name>{master}</name><duration>{o}</duration>{R60}\
         <in>{i}</in><out>{o}</out><start>{start}</start><end>{end}</end>\
         <masterclipid>{master}</masterclipid>{more}</clipitem>"
    )
}

/// A sound clip item as VEGAS writes it: its channel, pan and links.
fn vegas_sound(
    id: &str,
    master: &str,
    timing: (i64, i64, i64, i64),
    ch: u16,
    links: &[&str],
) -> String {
    let file = master.trim_end_matches(".mp4");
    let pan = if ch == 1 { -1 } else { 1 };
    let links: String = links
        .iter()
        .map(|l| format!("<link><linkclipref>{l}</linkclipref></link>"))
        .collect();
    vegas_item(
        id,
        master,
        timing,
        &format!(
            "<file id=\"{file}\" /><filter><effect><name>Audio Levels</name><effectid>audiolevels</effectid>\
             <parameter><parameterid>level</parameterid><value>1</value></parameter></effect></filter>\
             <filter><effect><name>Audio Pan</name><effectid>audiopan</effectid>\
             <parameter><parameterid>pan</parameterid><value>{pan}</value></parameter></effect></filter>\
             <sourcetrack><mediatype>audio</mediatype><trackindex>{ch}</trackindex></sourcetrack>{links}"
        ),
    )
}

fn vegas_crossfade(start: i64, end: i64) -> String {
    format!(
        "<transitionitem>{R60}<start>{start}</start><end>{end}</end><alignment>center</alignment>\
         <effect><name>Cross Fade ( 0dB)</name><effectid>KGAudioTransCrossFade0dB</effectid>\
         <effecttype>transition</effecttype><mediatype>audio</mediatype></effect></transitionitem>"
    )
}

/// A project as VEGAS 23 writes it (File › Export's Final Cut Pro 7 XML):
/// its files described in the bins' master clips, the sequence naming
/// them; a recording with two sound streams crossfading into another, on
/// two sound tracks listed from the bottom up, each a stereo pair whose
/// channels say the first stream; a stray transition after a right
/// channel's last clip.
fn vegas_document() -> String {
    let master = |name: &str, file: &str, path: &str| {
        format!(
            "<clip id=\"{name}\"><name>{name}</name><duration>2000</duration>{R60}<ismasterclip>TRUE</ismasterclip>\
             <media><video><track><clipitem id=\"{name}1\"><name>{name}</name>{R60}<masterclipid>{name}</masterclipid>\
             <file id=\"{file}\"><name>{name}</name><pathurl>file://localhost/{path}</pathurl>{R60}<duration>2000</duration>\
             <media><video><samplecharacteristics><width>1920</width><height>1080</height></samplecharacteristics></video>\
             <audio><channelcount>1</channelcount></audio><audio><channelcount>1</channelcount></audio></media></file>\
             </clipitem></track></video></media></clip>"
        )
    };
    let a_links = [
        "a.mp4805306375",
        "a.mp4536870920",
        "a.mp45368709201",
        "a.mp4536870921",
        "a.mp45368709211",
    ];
    let video = [
        vegas_item(
            "805306375",
            "a.mp4",
            (0, -1, 0, 600),
            &format!(
                "<file id=\"a\" /><filter><effect><name>Opacity</name><effectid>opacity</effectid>\
                 <parameter><parameterid>opacity</parameterid><value>100</value></parameter></effect></filter>{}",
                a_links
                    .iter()
                    .map(|l| format!("<link><linkclipref>{l}</linkclipref></link>"))
                    .collect::<String>()
            ),
        ),
        format!(
            "<transitionitem>{R60}<start>500</start><end>600</end><alignment>center</alignment>\
             <effect><name>Cross Dissolve</name><effectid>Cross Dissolve</effectid><mediatype>video</mediatype></effect></transitionitem>"
        ),
        vegas_item(
            "805306378",
            "b.mp4",
            (-1, 1400, 0, 900),
            "<file id=\"b\" /><link><linkclipref>b.mp4805306378</linkclipref></link>",
        ),
    ]
    .concat();
    // The lower track (VEGAS's) first: its b from 520, the upper's from 560.
    let tracks = [
        [
            vegas_sound("536870921", "a.mp4", (0, -1, 0, 600), 1, &a_links),
            vegas_crossfade(520, 600),
            vegas_sound(
                "536870924",
                "b.mp4",
                (-1, 1420, 0, 900),
                1,
                &["b.mp4536870924", "b.mp45368709241"],
            ),
        ]
        .concat(),
        [
            vegas_sound("5368709211", "a.mp4", (0, -1, 0, 600), 2, &a_links),
            vegas_crossfade(520, 600),
            vegas_sound(
                "5368709241",
                "b.mp4",
                (-1, 1420, 0, 900),
                2,
                &["b.mp4536870924", "b.mp45368709241"],
            ),
            vegas_crossfade(521, 600),
        ]
        .concat(),
        [
            vegas_sound("536870920", "a.mp4", (0, -1, 0, 600), 1, &a_links),
            vegas_crossfade(560, 600),
            vegas_sound(
                "536870923",
                "b.mp4",
                (-1, 1460, 0, 900),
                1,
                &["b.mp4536870923", "b.mp45368709231"],
            ),
        ]
        .concat(),
        [
            vegas_sound("5368709201", "a.mp4", (0, -1, 0, 600), 2, &a_links),
            vegas_crossfade(560, 600),
            vegas_sound(
                "5368709231",
                "b.mp4",
                (-1, 1460, 0, 900),
                2,
                &["b.mp4536870923", "b.mp45368709231"],
            ),
            vegas_crossfade(561, 600),
        ]
        .concat(),
    ];
    let audio: String = tracks
        .iter()
        .map(|t| format!("<track>{t}<enabled>TRUE</enabled><locked>FALSE</locked></track>"))
        .collect();
    format!(
        "\u{feff}<?xml version=\"1.0\"?>\r\n<xmeml version=\"5\"><project><name>P</name><children>\
         <bin><name>Custom Bins</name><children /></bin>{}{}\
         <sequence id=\"Sequence 1\"><name>Sequence 1</name><duration>1460</duration>{R60}\
         <timecode>{R60}<frame>216000</frame></timecode><in>-1</in><out>-1</out><media><video>\
         <format><samplecharacteristics><width>1920</width><height>1080</height></samplecharacteristics></format>\
         <track>{video}<enabled>TRUE</enabled></track></video><audio>{audio}</audio></media></sequence>\
         </children></project></xmeml>",
        master("a.mp4", "a", "C:/Gravações/a b.mp4"),
        master("b.mp4", "b", "C:/Gravações/b.mp4"),
    )
}

/// VEGAS's export: its files found in the bins, the crossfades as overlaps,
/// each stereo pair one event, its audio tracks from the top down, the
/// links grouping the first recording's picture and sound.
#[test]
fn a_vegas_project_opens_as_it_was() {
    let doc = read(&vegas_document()).unwrap();
    let r = FrameRate::new(60, 1);
    assert_eq!(doc.rate, Some(r));
    assert_eq!(doc.name, "Sequence 1");
    assert_eq!(doc.frame, Some((1920, 1080)));
    assert_eq!(doc.media.len(), 2);
    assert_eq!(
        doc.media[0].source,
        MediaRefSource::File("C:/Gravações/a b.mp4".into())
    );
    assert_eq!(doc.media[0].available, Some(r.frame_start(2000)));
    assert_eq!(
        placed(&doc, TrackKind::Video, r),
        vec![vec![(0, 600, 0), (500, 900, 0)]]
    );
    let video = &doc.tracks[0].events;
    assert_eq!(video[1].transition, Some(Transition::Dissolve));
    // The upper track (its b from 560) first.
    assert_eq!(
        placed(&doc, TrackKind::Audio, r),
        vec![
            vec![(0, 600, 0), (560, 900, 0)],
            vec![(0, 600, 0), (520, 900, 0)],
        ]
    );
    let sound = audio(&doc);
    let pair = Some(DocChannels {
        left: 1,
        right: 2,
        centered: false,
    });
    for e in sound.iter().flat_map(|t| t.iter()) {
        assert_eq!(e.channels, pair);
        assert_eq!(e.level, Some(1.0));
    }
    assert_eq!(sound[0][1].curves[0], Some(FadeCurve::Linear));
    assert_eq!(sound[0][0].curves[1], Some(FadeCurve::Linear));
    // The first recording's picture and its two sounds go together; the
    // second's sounds are linked to nothing else.
    let link = video[0].link;
    assert!(link.is_some());
    assert_eq!((sound[0][0].link, sound[1][0].link), (link, link));
    assert_eq!(
        (video[1].link, sound[0][1].link, sound[1][1].link),
        (None, None, None)
    );
    assert_eq!(doc.skipped, 0);
}

/// Built with its files found (two streams each): the second track's
/// events play each file's second stream, as VEGAS's did.
#[test]
fn a_vegas_project_s_second_sound_track_plays_the_second_stream() {
    let doc = read(&vegas_document()).unwrap();
    let mut p = Project::new(ProjectSettings::default());
    let found: Vec<Option<MediaInfo>> = (0..doc.media.len())
        .map(|i| {
            let id = file(&mut p, &format!(r"C:\m{i}.mp4"), 2);
            p.media(id).cloned()
        })
        .collect();
    let built = otio::build(&doc, &found, &ProjectSettings::default()).unwrap();
    let p = built.project;
    assert_eq!(p.settings.frame_rate, FrameRate::new(60, 1));
    let tracks: Vec<Vec<&Clip>> = p
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Audio)
        .map(|t| t.clips.iter().collect())
        .collect();
    assert_eq!(tracks.len(), 2);
    for c in &tracks[0] {
        assert_eq!((c.audio_stream, c.channels), (0, ChannelMap::Both));
    }
    for c in &tracks[1] {
        assert_eq!((c.audio_stream, c.channels), (1, ChannelMap::Both));
    }
    // The first recording's picture and sounds in one group.
    let picture = &p
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Video)
        .unwrap()
        .clips[0];
    assert!(picture.group.is_some());
    assert_eq!(tracks[0][0].group, picture.group);
    assert_eq!(tracks[1][0].group, picture.group);
    assert_eq!(tracks[1][1].fade_in_curve, FadeCurve::Linear);
}

/// Premiere's way: tracks of a stereo pair marked as such, a clip at
/// another rate than the sequence's, a fade to black, markers, the frame
/// and sample rate.
#[test]
fn premiere_pairs_fades_markers_and_rates() {
    let text = r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE xmeml><xmeml version="4">
        <sequence id="sequence-1"><name>S</name><rate><timebase>30</timebase><ntsc>FALSE</ntsc></rate>
        <media><video><format><samplecharacteristics><width>1280</width><height>720</height></samplecharacteristics></format>
          <track>
            <clipitem id="clipitem-1"><name>c.mov</name><rate><timebase>15</timebase><ntsc>FALSE</ntsc></rate>
              <start>30</start><end>90</end><in>0</in><out>30</out>
              <file id="file-1"><name>c.mov</name><pathurl>file://localhost/D%3a/media/c.mov</pathurl>
                <rate><timebase>30</timebase></rate><duration>300</duration></file>
              <marker><name>Hit</name><in>10</in><out>-1</out></marker></clipitem>
            <clipitem id="clipitem-2"><name>d.mov</name><rate><timebase>30</timebase></rate>
              <start>90</start><end>-1</end><in>0</in><out>60</out>
              <file id="file-2"><pathurl>file://localhost/D%3a/media/d.mov</pathurl></file></clipitem>
            <transitionitem><start>130</start><end>150</end><alignment>end-black</alignment>
              <effect><name>Cross Dissolve</name><effectid>Cross Dissolve</effectid></effect></transitionitem>
          </track></video>
          <audio><format><samplecharacteristics><samplerate>44100</samplerate></samplecharacteristics></format>
            <track currentExplodedTrackIndex="0" totalExplodedTrackCount="2" premiereTrackType="Stereo" MZ.TrackName="Voz">
              <clipitem id="clipitem-3" premiereChannelType="stereo"><rate><timebase>30</timebase></rate>
                <start>30</start><end>90</end><in>0</in><out>60</out><file id="file-1"/>
                <sourcetrack><mediatype>audio</mediatype><trackindex>1</trackindex></sourcetrack>
                <link><linkclipref>clipitem-1</linkclipref></link><link><linkclipref>clipitem-3</linkclipref></link>
                <link><linkclipref>clipitem-4</linkclipref></link></clipitem>
            </track>
            <track currentExplodedTrackIndex="1" totalExplodedTrackCount="2" premiereTrackType="Stereo">
              <clipitem id="clipitem-4" premiereChannelType="stereo"><rate><timebase>30</timebase></rate>
                <start>30</start><end>90</end><in>0</in><out>60</out><file id="file-1"/>
                <sourcetrack><mediatype>audio</mediatype><trackindex>2</trackindex></sourcetrack>
                <link><linkclipref>clipitem-1</linkclipref></link><link><linkclipref>clipitem-3</linkclipref></link>
                <link><linkclipref>clipitem-4</linkclipref></link></clipitem>
            </track>
          </audio></media>
        <marker><name>Start</name><in>0</in><out>-1</out></marker>
        <marker><name>Part</name><in>30</in><out>60</out></marker>
        </sequence></xmeml>"#;
    let doc = read(text).unwrap();
    let r = FrameRate::new(30, 1);
    assert_eq!(
        (doc.frame, doc.sample_rate),
        (Some((1280, 720)), Some(44100))
    );
    assert_eq!(
        doc.media[0].source,
        MediaRefSource::File("D:/media/c.mov".into())
    );
    // The clip's 30 frames at 15 a second are two seconds.
    assert_eq!(
        placed(&doc, TrackKind::Video, r),
        vec![vec![(30, 60, 0), (90, 60, 0)]]
    );
    let video = &doc.tracks[0].events;
    assert_eq!(video[1].fade_out, r.frame_start(20));
    assert_eq!(
        video[0].markers,
        vec![(
            FrameRate::new(15, 1).frame_start(10),
            Tick::ZERO,
            "Hit".into()
        )]
    );
    let sound = audio(&doc);
    assert_eq!(sound.len(), 1, "the pair is one track");
    assert_eq!(doc.tracks[1].name, "Voz");
    assert_eq!(
        sound[0][0].channels,
        Some(DocChannels {
            left: 1,
            right: 2,
            centered: false
        })
    );
    assert!(video[0].link.is_some() && video[0].link == sound[0][0].link);
    let marks: Vec<(Tick, Tick, &str)> = doc
        .markers
        .iter()
        .map(|m| (m.start, m.duration, m.name.as_str()))
        .collect();
    assert_eq!(
        marks,
        vec![(Tick::ZERO, Tick::ZERO, "Start"), (s(1), s(1), "Part")]
    );
    assert_eq!(doc.skipped, 0);
}

/// A stereo pair whose channels are linked to their picture only, not to
/// each other: still one event, with the picture.
#[test]
fn channels_linked_through_their_picture_are_a_pair() {
    let text = r#"<xmeml version="5"><sequence><rate><timebase>25</timebase></rate><media>
        <video><track><clipitem id="v"><start>0</start><end>50</end><in>0</in><out>50</out>
          <file id="f"><pathurl>file:///C:/a.mov</pathurl></file>
          <link><linkclipref>v</linkclipref></link></clipitem></track></video>
        <audio>
          <track><clipitem id="l"><start>0</start><end>50</end><in>0</in><out>50</out><file id="f"/>
            <sourcetrack><mediatype>audio</mediatype><trackindex>1</trackindex></sourcetrack>
            <link><linkclipref>v</linkclipref></link></clipitem></track>
          <track><clipitem id="r"><start>0</start><end>50</end><in>0</in><out>50</out><file id="f"/>
            <sourcetrack><mediatype>audio</mediatype><trackindex>2</trackindex></sourcetrack>
            <link><linkclipref>v</linkclipref></link></clipitem></track>
        </audio></media></sequence></xmeml>"#;
    let doc = read(text).unwrap();
    let sound = audio(&doc);
    assert_eq!(sound.len(), 1);
    assert_eq!(
        sound[0][0].channels,
        Some(DocChannels {
            left: 1,
            right: 2,
            centered: false
        })
    );
    assert!(sound[0][0].link.is_some() && sound[0][0].link == doc.tracks[0].events[0].link);
}

/// A transition between clips written at their cut: each reaches into
/// its handle, over the whole transition.
#[test]
fn clips_written_at_the_cut_reach_over_their_transition() {
    let text = r#"<xmeml version="5"><sequence><rate><timebase>25</timebase></rate><media><video><track>
        <clipitem id="a"><start>0</start><end>50</end><in>0</in><out>50</out>
          <file id="f"><pathurl>file:///C:/a.mov</pathurl></file></clipitem>
        <transitionitem><start>40</start><end>60</end><alignment>center</alignment></transitionitem>
        <clipitem id="b"><start>50</start><end>100</end><in>30</in><out>80</out>
          <file id="g"><pathurl>file:///C:/b.mov</pathurl></file></clipitem>
        </track></video></media></sequence></xmeml>"#;
    let doc = read(text).unwrap();
    let r = FrameRate::new(25, 1);
    assert_eq!(
        placed(&doc, TrackKind::Video, r),
        vec![vec![(0, 60, 0), (40, 60, 20)]]
    );
    assert_eq!(
        doc.tracks[0].events[1].transition,
        Some(Transition::Dissolve)
    );
}

/// A sequence nested in a clip: its tracks' events in the clip's place,
/// cut to the part it shows; its second track above.
#[test]
fn nested_sequences_are_laid_out_in_place() {
    let text = r#"<xmeml version="5"><sequence id="outer"><rate><timebase>25</timebase></rate>
        <media><video><track>
          <clipitem id="n"><start>100</start><end>150</end><in>10</in><out>60</out>
            <sequence id="inner"><rate><timebase>25</timebase></rate><media><video>
              <track>
                <clipitem id="x"><start>0</start><end>40</end><in>0</in><out>40</out>
                  <file id="f"><pathurl>file:///C:/x.mov</pathurl></file></clipitem>
                <clipitem id="y"><start>40</start><end>100</end><in>5</in><out>65</out>
                  <file id="g"><pathurl>file:///C:/y.mov</pathurl></file></clipitem>
              </track>
              <track><clipitem id="z"><start>20</start><end>30</end><in>0</in><out>10</out><file id="f"/></clipitem></track>
            </video></media></sequence>
          </clipitem>
        </track></video></media></sequence></xmeml>"#;
    let doc = read(text).unwrap();
    let r = FrameRate::new(25, 1);
    assert_eq!(
        placed(&doc, TrackKind::Video, r),
        vec![vec![(100, 30, 10), (130, 20, 5)], vec![(110, 10, 0)]]
    );
    assert_eq!(doc.media.len(), 2);
}

/// Opacity keyframes rising from nothing and falling back: fades, the
/// level between them.
#[test]
fn keyframed_levels_become_fades() {
    let text = r#"<xmeml version="5"><sequence><rate><timebase>25</timebase></rate><media><video><track>
        <clipitem id="a"><start>0</start><end>100</end><in>0</in><out>100</out>
          <file id="f"><pathurl>file:///C:/a.mov</pathurl></file>
          <filter><effect><effectid>opacity</effectid><parameter><parameterid>opacity</parameterid>
            <keyframe><when>0</when><value>0</value></keyframe><keyframe><when>25</when><value>80</value></keyframe>
            <keyframe><when>75</when><value>80</value></keyframe><keyframe><when>100</when><value>0</value></keyframe>
          </parameter></effect></filter></clipitem>
        </track></video></media></sequence></xmeml>"#;
    let doc = read(text).unwrap();
    let e = &doc.tracks[0].events[0];
    let r = FrameRate::new(25, 1);
    assert_eq!(e.level, Some(0.8));
    assert_eq!(
        (e.fade_in, e.fade_out),
        (r.frame_start(25), r.frame_start(25))
    );
}

/// Final Cut Pro 7's speeds (Time Remap's constant speed: as long as the
/// media part at it), titles and colors.
#[test]
fn speeds_titles_and_colors() {
    let text = r#"<?xml version="1.0"?><!DOCTYPE xmeml><xmeml version="5"><sequence><name>S</name>
        <rate><timebase>25</timebase></rate><media><video><track>
          <clipitem id="a"><start>0</start><end>50</end><in>0</in><out>100</out>
            <file id="f1"><name>a.mov</name><pathurl>file://localhost/D:/a.mov</pathurl></file>
            <filter><effect><effectid>timeremap</effectid>
              <parameter><parameterid>speed</parameterid><value>200</value></parameter>
              <parameter><parameterid>reverse</parameterid><value>FALSE</value></parameter>
            </effect></filter></clipitem>
          <generatoritem><name>Hello</name><start>50</start><end>75</end>
            <effect><name>Text</name><effectid>Text</effectid>
              <parameter><parameterid>str</parameterid><value>Hello there</value></parameter></effect></generatoritem>
          <generatoritem><name>Red</name><start>75</start><end>100</end>
            <effect><name>Color</name><effectid>Color</effectid><parameter><parameterid>color</parameterid>
              <value><alpha>255</alpha><red>255</red><green>0</green><blue>0</blue></value></parameter></effect></generatoritem>
        </track></video></media></sequence></xmeml>"#;
    let doc = read(text).unwrap();
    let ev = &doc.tracks[0].events;
    let fps = FrameRate::new(25, 1);
    assert_eq!((ev[0].duration, ev[0].speed), (fps.frame_start(50), 2.0));
    let MediaRefSource::Generated(Generator::Plugin(fx)) = &doc.media[ev[1].media].source else {
        panic!("a title")
    };
    assert_eq!(
        fx.params["text"].value,
        PluginValue::Text("Hello there".into())
    );
    assert_eq!(
        doc.media[ev[2].media].source,
        MediaRefSource::Generated(Generator::SolidColor {
            rgba: [1.0, 0.0, 0.0, 1.0]
        })
    );
    assert_eq!(ev[2].start, fps.frame_start(75));
    assert_eq!(doc.skipped, 0);
}

#[test]
fn a_transition_s_neighbours_overlap_into_a_crossfade() {
    let text = r#"<?xml version="1.0"?><!DOCTYPE xmeml><xmeml version="4"><project><children>
        <sequence><name>T</name><rate><timebase>25</timebase><ntsc>FALSE</ntsc></rate>
        <media><video><track>
          <clipitem id="a"><start>0</start><end>-1</end><in>0</in><out>60</out>
            <file id="f1"><name>a.mov</name><pathurl>file://localhost/D:/a.mov</pathurl></file></clipitem>
          <transitionitem><start>40</start><end>60</end></transitionitem>
          <clipitem id="b"><start>-1</start><end>100</end><in>10</in><out>70</out>
            <file id="f1"/></clipitem>
          <generatoritem><start>100</start><end>125</end></generatoritem>
        </track></video></media></sequence></children></project></xmeml>"#;
    let doc = read(text).unwrap();
    let ev = &doc.tracks[0].events;
    let fps = FrameRate::new(25, 1);
    assert_eq!(
        (ev[0].start, ev[0].duration),
        (Tick::ZERO, fps.frame_start(60))
    );
    // B's start from its end and length: 100 − 60 = 40, over A's end.
    assert_eq!(ev[1].start, fps.frame_start(40));
    assert_eq!(ev[1].transition, Some(Transition::Dissolve));
    assert_eq!(doc.media.len(), 1);
    assert_eq!(doc.media[0].source, MediaRefSource::File("D:/a.mov".into()));
    assert_eq!(doc.skipped, 1, "the generator");
}

#[test]
fn other_files_are_refused() {
    assert!(read("<fcpxml/>").is_err());
    assert!(read("not xml").is_err());
    assert!(read("<xmeml version=\"5\"/>").is_err());
}

/// An event's start, length and media time in frames, whether it is
/// muted, its speed.
type Shape = (i64, i64, i64, bool, f64);

/// A title of BananaV's saying `text`.
fn title(p: &mut Project, text: &str) -> MediaId {
    let mut params = std::collections::BTreeMap::new();
    params.insert(
        "text".to_string(),
        PluginParam {
            value: PluginValue::Text(text.into()),
            keys: Vec::new(),
        },
    );
    let g = Generator::Plugin(Box::new(PluginFx {
        id: TITLES_PLUGIN.into(),
        version: [1, 0],
        name: "Titles & Text".into(),
        params,
    }));
    let m = MediaInfo::generated(g, &p.settings);
    p.add_media(m)
}

/// A project of BananaV's written and read back: its events in their
/// places with their crossfades, fades and their curves, speeds, mutes,
/// levels, streams and channels, groups, titles and colors, markers and
/// regions.
#[test]
fn a_project_comes_back_as_it_went() {
    let mut p = Project::new(ProjectSettings::default());
    let r = p.settings.frame_rate;
    let v1 = p.tracks[0].id;
    let a1 = p.tracks[1].id;
    let v2 = p.add_track(TrackKind::Video);
    // A recording with two streams: its picture on V1, its sounds on two
    // tracks.
    let a = file(&mut p, r"C:\Vídeos\meu clipe.mp4", 2);
    let ids = p.insert_media(a, Tick::ZERO, Some(v1), Some(a1)).unwrap();
    for &id in &ids {
        p.trim_end(id, s(6)).unwrap();
    }
    p.clip_mut(ids[2]).unwrap().level = 0.5;
    // Another file crossfading into it at 5 s, through black, with a
    // linear crossfade of its sound, its channels swapped, fading out.
    let b = file(&mut p, r"C:\Media\two.mov", 1);
    let bs = p.insert_media(b, s(5), Some(v1), Some(a1)).unwrap();
    for &id in &bs {
        p.trim_end(id, s(9)).unwrap();
    }
    p.clip_mut(bs[0]).unwrap().transition = Transition::FadeThroughBlack;
    p.clip_mut(bs[0]).unwrap().fade_out = s(1);
    p.clip_mut(ids[1]).unwrap().fade_out_curve = FadeCurve::Linear;
    p.clip_mut(bs[1]).unwrap().fade_in_curve = FadeCurve::Linear;
    p.clip_mut(bs[1]).unwrap().channels = ChannelMap::Swap;
    // A title fading in and a muted color above; a part played twice as
    // fast.
    let t = title(&mut p, "Olá & <bem-vindo>");
    let ts = p.insert_media(t, s(2), Some(v2), None).unwrap();
    p.trim_end(ts[0], s(4)).unwrap();
    p.clip_mut(ts[0]).unwrap().fade_in = s(1);
    let red = p.add_media(MediaInfo::generated(
        Generator::SolidColor {
            rgba: [1.0, 0.0, 0.0, 1.0],
        },
        &p.settings,
    ));
    let rs = p.insert_media(red, s(10), Some(v2), None).unwrap();
    p.trim_end(rs[0], s(11)).unwrap();
    p.clip_mut(rs[0]).unwrap().muted = true;
    let c = file(&mut p, r"C:\Media\three.mov", 0);
    let cs = p.insert_media(c, s(12), Some(v1), None).unwrap();
    p.trim_end(cs[0], s(14)).unwrap();
    p.clip_mut(cs[0]).unwrap().rate = 2.0;
    let id = MarkerId(p.alloc_id());
    p.markers.push(Marker {
        id,
        time: s(3),
        name: "Começo".into(),
    });
    let id = MarkerId(p.alloc_id());
    p.regions.push(Region {
        id,
        range: bv_core::TimeRange::new(s(4), s(1)),
        name: "Parte".into(),
    });
    p.check_invariants().unwrap();

    let (text, partial) = export(&p, "Corte & teste");
    assert_eq!(partial, 0, "{text}");
    assert!(text.contains("<name>Corte &amp; teste</name>"));
    assert!(text.contains("file://localhost/C:/V%C3%ADdeos/meu%20clipe.mp4"));
    assert_eq!(
        text.matches("<pathurl>").count(),
        3,
        "each file described once"
    );
    assert_eq!(
        text.matches("<track>").count() + text.matches("<track ").count(),
        text.matches("</track>").count()
    );

    let doc = read(&text).unwrap();
    assert_eq!(doc.skipped, 0);
    let found: Vec<Option<MediaInfo>> = doc
        .media
        .iter()
        .map(|m| match &m.source {
            MediaRefSource::File(path) => p
                .media
                .iter()
                .find(|x| matches!(&x.source, MediaSource::File { path: q } if q.replace('\\', "/") == *path))
                .cloned(),
            _ => None,
        })
        .collect();
    assert!(found.iter().filter(|f| f.is_some()).count() == 3);
    let back = otio::build(&doc, &found, &ProjectSettings::default())
        .unwrap()
        .project;
    assert_eq!(back.settings.frame_rate, r);
    let f = |t: Tick| r.frame_at(r.snap_nearest(t));
    let shape = |p: &Project| -> Vec<Vec<Shape>> {
        p.tracks
            .iter()
            .map(|t| {
                t.clips
                    .iter()
                    .map(|c| (f(c.start), f(c.duration), f(c.source_in), c.muted, c.rate))
                    .collect()
            })
            .collect()
    };
    let kinds = |p: &Project| p.tracks.iter().map(|t| t.kind).collect::<Vec<_>>();
    assert_eq!(kinds(&back), kinds(&p));
    assert_eq!(shape(&back), shape(&p));
    let same = |x: &Project, id| {
        let (t, c) = x.clips().find(|(_, c)| c.id == id).unwrap();
        (
            x.tracks.iter().position(|o| o.id == t.id).unwrap(),
            t.clips.iter().position(|o| o.id == c.id).unwrap(),
        )
    };
    let clip = |at: (usize, usize)| &back.tracks[at.0].clips[at.1];
    let (b_picture, b_sound) = (clip(same(&p, bs[0])), clip(same(&p, bs[1])));
    assert_eq!(b_picture.transition, Transition::FadeThroughBlack);
    assert_eq!(f(b_picture.fade_out), f(s(1)));
    assert_eq!(b_sound.fade_in_curve, FadeCurve::Linear);
    assert_eq!(clip(same(&p, ids[1])).fade_out_curve, FadeCurve::Linear);
    assert_eq!(b_sound.channels, ChannelMap::Swap);
    let second = clip(same(&p, ids[2]));
    assert_eq!((second.audio_stream, second.level), (1, 0.5));
    assert_eq!(f(clip(same(&p, ts[0])).fade_in), f(s(1)));
    // Groups: each file's picture with its sounds.
    let group = |id| clip(same(&p, id)).group;
    assert!(group(ids[0]).is_some());
    assert_eq!(
        (group(ids[1]), group(ids[2])),
        (group(ids[0]), group(ids[0]))
    );
    assert!(group(bs[0]).is_some() && group(bs[0]) == group(bs[1]));
    assert_ne!(group(bs[0]), group(ids[0]));
    // Generated media.
    let shows = |id| &back.media(clip(same(&p, id)).media).unwrap().source;
    assert!(matches!(
        shows(ts[0]),
        MediaSource::Generated(Generator::Plugin(fx)) if fx.params["text"].value == PluginValue::Text("Olá & <bem-vindo>".into())
    ));
    assert!(matches!(
        shows(rs[0]),
        MediaSource::Generated(Generator::SolidColor { .. })
    ));
    let marks: Vec<(i64, &str)> = back
        .markers
        .iter()
        .map(|m| (f(m.time), m.name.as_str()))
        .collect();
    assert_eq!(marks, vec![(f(s(3)), "Começo")]);
    let regions: Vec<(i64, i64, &str)> = back
        .regions
        .iter()
        .map(|g| (f(g.range.start), f(g.range.duration), g.name.as_str()))
        .collect();
    assert_eq!(regions, vec![(f(s(4)), f(s(1)), "Parte")]);
}

/// What the format cannot say is counted: an event's effects, its
/// Pan/Crop, a frozen frame, a generator other than a title or a color.
#[test]
fn what_the_format_cannot_say_is_counted() {
    let mut p = Project::new(ProjectSettings::default());
    let v = p.tracks[0].id;
    let a = file(&mut p, r"C:\a.mp4", 0);
    let ids = p.insert_media(a, Tick::ZERO, Some(v), None).unwrap();
    p.trim_end(ids[0], s(2)).unwrap();
    p.clip_mut(ids[0]).unwrap().transform.scale = 1.5;
    let bars = p.add_media(MediaInfo::generated(Generator::ColorBars, &p.settings));
    let bs = p.insert_media(bars, s(3), Some(v), None).unwrap();
    p.trim_end(bs[0], s(4)).unwrap();
    let (text, partial) = export(&p, "S");
    assert_eq!(partial, 2);
    assert_eq!(text.matches("<clipitem ").count(), 1);
}
