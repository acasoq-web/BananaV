//! The shapes of the icons, on a grid of 24 units, in SVG's path syntax
//! (`path.rs` reads it). Each icon lists its layers from the bottom up.

use super::path::{self, Sub};
use super::{Edge, Fill, Icon, Ink, Layer};

/// The layers of an icon, bottom first.
pub(super) fn layers(icon: Icon) -> Vec<Layer> {
    match icon {
        // A page with a plus.
        Icon::New => vec![
            body(d("M14 3H8A3 3 0 0 0 5 6V18A3 3 0 0 0 8 21H16A3 3 0 0 0 19 18V8Z")),
            line(d("M14 3V6A2 2 0 0 0 16 8H19")).dim(),
            line(d("M12 11.5V17.5M9 14.5H15")),
        ],
        // A folder with its front panel lifted.
        Icon::Open => vec![
            line(d("M3 18V6A3 3 0 0 1 6 3H9.2A2 2 0 0 1 10.6 3.6L12.4 5.5H18A3 3 0 0 1 21 8.5V11")),
            body(d("M3.5 19.5L6.2 11.5H21L18 19.5Z")),
        ],
        // A floppy disk: shutter above, label below.
        Icon::Save => vec![
            body(d("M6 3H16.5L21 7.5V18A3 3 0 0 1 18 21H6A3 3 0 0 1 3 18V6A3 3 0 0 1 6 3Z")),
            line(d("M8 3V7A1 1 0 0 0 9 8H14.5A1 1 0 0 0 15.5 7V3")).dim(),
            line(d("M7.5 21V15.5A1 1 0 0 1 8.5 14.5H15.5A1 1 0 0 1 16.5 15.5V21")).dim(),
        ],
        // A clapperboard: render to a file.
        Icon::Render => vec![
            body(d("M4 10.5H20V18A3 3 0 0 1 17 21H7A3 3 0 0 1 4 18Z")),
            body(d("M3.45 7.04L19.26 4.54 19.8 8 4 10.5Z")),
            line(d("M7.48 9.95L9.89 6.02M11.9 9.25L14.32 5.32M16.33 8.55L18.74 4.62")).dim().fine(),
        ],
        // A cog of eight teeth with its hub.
        Icon::Settings => vec![
            body(d("M9.81 4.93L10.49 2.62H13.51L14.19 4.93A7.4 7.4 0 0 1 15.45 5.45L17.56 4.3 19.7 6.44 18.55 8.55A7.4 7.4 0 0 1 19.07 9.81L21.38 10.49V13.51L19.07 14.19A7.4 7.4 0 0 1 18.55 15.45L19.7 17.56 17.56 19.7 15.45 18.55A7.4 7.4 0 0 1 14.19 19.07L13.51 21.38H10.49L9.81 19.07A7.4 7.4 0 0 1 8.55 18.55L6.44 19.7 4.3 17.56 5.45 15.45A7.4 7.4 0 0 1 4.93 14.19L2.62 13.51V10.49L4.93 9.81A7.4 7.4 0 0 1 5.45 8.55L4.3 6.44 6.44 4.3 8.55 5.45A7.4 7.4 0 0 1 9.81 4.93Z")),
            line(d("M15 12A3 3 0 1 1 9 12 3 3 0 1 1 15 12Z")),
        ],
        // Scissors, blades crossed over two rings.
        Icon::Cut => vec![
            line(d("M8.9 14.7L18 3M15.1 14.7L6 3")),
            line(d("M10 17.5A3 3 0 1 1 4 17.5 3 3 0 1 1 10 17.5Z")),
            line(d("M20 17.5A3 3 0 1 1 14 17.5 3 3 0 1 1 20 17.5Z")),
        ],
        // Two sheets, one behind the other.
        Icon::Copy => vec![
            line(d("M15 9V6A3 3 0 0 0 12 3H6A3 3 0 0 0 3 6V12A3 3 0 0 0 6 15H9")).dim(),
            body(d("M12 9H18A3 3 0 0 1 21 12V18A3 3 0 0 1 18 21H12A3 3 0 0 1 9 18V12A3 3 0 0 1 12 9Z")),
        ],
        // A clipboard with its clip.
        Icon::Paste => vec![
            body(d("M8 5H16A3 3 0 0 1 19 8V18A3 3 0 0 1 16 21H8A3 3 0 0 1 5 18V8A3 3 0 0 1 8 5Z")),
            line(d("M9 12H15M9 16H13")).dim().fine(),
            solid(d("M10.5 3.6H13.5A1.4 1.4 0 0 1 14.9 5H14.9A1.4 1.4 0 0 1 13.5 6.4H10.5A1.4 1.4 0 0 1 9.1 5H9.1A1.4 1.4 0 0 1 10.5 3.6Z")),
        ],
        // An arrow turning back.
        Icon::Undo => vec![
            line(d("M8 4.5L4 8.5 8 12.5M4.5 8.5H14.5A5.5 5.5 0 0 1 14.5 19.5H10")),
        ],
        // An arrow turning forward.
        Icon::Redo => vec![
            line(d("M16 4.5L20 8.5 16 12.5M19.5 8.5H9.5A5.5 5.5 0 0 0 9.5 19.5H14")),
        ],
        // A rounded play triangle.
        Icon::Play => vec![
            solid(d("M6.1 4.53L19.28 12 6.1 19.47Z")),
        ],
        // Two bars.
        Icon::Pause => vec![
            solid(d("M7.3 4.6H8.2A1.2 1.2 0 0 1 9.4 5.8V18.2A1.2 1.2 0 0 1 8.2 19.4H7.3A1.2 1.2 0 0 1 6.1 18.2V5.8A1.2 1.2 0 0 1 7.3 4.6Z")),
            solid(d("M15.8 4.6H16.7A1.2 1.2 0 0 1 17.9 5.8V18.2A1.2 1.2 0 0 1 16.7 19.4H15.8A1.2 1.2 0 0 1 14.6 18.2V5.8A1.2 1.2 0 0 1 15.8 4.6Z")),
        ],
        // A rounded square.
        Icon::Stop => vec![
            solid(d("M8 5.6H16A2.4 2.4 0 0 1 18.4 8V16A2.4 2.4 0 0 1 16 18.4H8A2.4 2.4 0 0 1 5.6 16V8A2.4 2.4 0 0 1 8 5.6Z")),
        ],
        // A red dot in a ring.
        Icon::Record => vec![
            line(d("M21 12A9 9 0 1 1 3 12 9 9 0 1 1 21 12Z")).dim(),
            solid(d("M17.4 12A5.4 5.4 0 1 1 6.6 12 5.4 5.4 0 1 1 17.4 12Z")).red(),
        ],
        // Two arrows chasing each other round a rectangle.
        Icon::Loop => vec![
            line(d("M3.5 11V10A3 3 0 0 1 6.5 7H19.5M17.67 4.17L20.5 7 17.67 9.83M20.5 13V14A3 3 0 0 1 17.5 17H4.5M6.33 19.83L3.5 17 6.33 14.17")),
        ],
        // A bar and a play triangle.
        Icon::PlayFromStart => vec![
            solid(d("M5.2 5.6H5.2A0.6 0.6 0 0 1 5.8 6.2V17.8A0.6 0.6 0 0 1 5.2 18.4H5.2A0.6 0.6 0 0 1 4.6 17.8V6.2A0.6 0.6 0 0 1 5.2 5.6Z")),
            solid(d("M10.1 5.64L19.43 12 10.1 18.36Z")),
        ],
        // A bar and two triangles pointing at it.
        Icon::GoStart => vec![
            solid(d("M4.7 5.6H4.7A0.6 0.6 0 0 1 5.3 6.2V17.8A0.6 0.6 0 0 1 4.7 18.4H4.7A0.6 0.6 0 0 1 4.1 17.8V6.2A0.6 0.6 0 0 1 4.7 5.6Z")),
            solid(d("M8.88 12L13.8 7.38V16.62Z")),
            solid(d("M15.28 12L20.2 7.38V16.62Z")),
        ],
        // Two triangles and a bar.
        Icon::GoEnd => vec![
            solid(d("M19.3 5.6H19.3A0.6 0.6 0 0 0 18.7 6.2V17.8A0.6 0.6 0 0 0 19.3 18.4H19.3A0.6 0.6 0 0 0 19.9 17.8V6.2A0.6 0.6 0 0 0 19.3 5.6Z")),
            solid(d("M15.12 12L10.2 7.38V16.62Z")),
            solid(d("M8.72 12L3.8 7.38V16.62Z")),
        ],
        // A bar and a triangle pointing at it.
        Icon::PrevFrame => vec![
            solid(d("M6.2 5.6H6.2A0.6 0.6 0 0 1 6.8 6.2V17.8A0.6 0.6 0 0 1 6.2 18.4H6.2A0.6 0.6 0 0 1 5.6 17.8V6.2A0.6 0.6 0 0 1 6.2 5.6Z")),
            solid(d("M11.56 12L19.4 6.64V17.36Z")),
        ],
        // A triangle and a bar.
        Icon::NextFrame => vec![
            solid(d("M17.8 5.6H17.8A0.6 0.6 0 0 0 17.2 6.2V17.8A0.6 0.6 0 0 0 17.8 18.4H17.8A0.6 0.6 0 0 0 18.4 17.8V6.2A0.6 0.6 0 0 0 17.8 5.6Z")),
            solid(d("M12.44 12L4.6 6.64V17.36Z")),
        ],
        // The pointer.
        Icon::EditTool => vec![
            body(d("M6 3V19L10.5 14.8 13.5 21 16.3 19.7 13.5 13.8H19.5Z")),
        ],
        // An envelope line with two nodes.
        Icon::EnvelopeTool => vec![
            line(d("M3 17H6M9.25 15.44L14.75 8.56M18 7H21")),
            body(d("M10 17A2 2 0 1 1 6 17 2 2 0 1 1 10 17Z")),
            body(d("M18 7A2 2 0 1 1 14 7 2 2 0 1 1 18 7Z")),
        ],
        // A dashed selection rectangle.
        Icon::SelectTool => vec![
            dashed(d("M6 5H18A2 2 0 0 1 20 7V17A2 2 0 0 1 18 19H6A2 2 0 0 1 4 17V7A2 2 0 0 1 6 5Z")),
        ],
        // A magnifier with a plus.
        Icon::ZoomTool => vec![
            body(d("M17 10.5A6.5 6.5 0 1 1 4 10.5 6.5 6.5 0 1 1 17 10.5Z")),
            line(d("M15.5 15.5L20.5 20.5")),
            line(d("M10.5 7.5V13.5M7.5 10.5H13.5")).dim(),
        ],
        // A waste bin.
        Icon::Delete => vec![
            body(d("M6 7L6.9 18A3 3 0 0 0 9.9 21H14.1A3 3 0 0 0 17.1 18L18 7")),
            line(d("M4 7H20M9.5 7V5A1 1 0 0 1 10.5 4H13.5A1 1 0 0 1 14.5 5V7")),
            line(d("M10 11V17M14 11V17")).dim().fine(),
        ],
        // A bracket and a triangle: keep what follows.
        Icon::TrimStart => vec![
            line(d("M7.5 4H5A1.5 1.5 0 0 0 3.5 5.5V18.5A1.5 1.5 0 0 0 5 20H7.5")),
            solid(d("M10.6 7.03L19.29 12 10.6 16.97Z")),
        ],
        // A triangle and a bracket: keep what comes before.
        Icon::TrimEnd => vec![
            line(d("M16.5 4H19A1.5 1.5 0 0 1 20.5 5.5V18.5A1.5 1.5 0 0 1 19 20H16.5")),
            solid(d("M13.4 7.03L4.71 12 13.4 16.97Z")),
        ],
        // An event cut in two at the playhead.
        Icon::Split => vec![
            body(d("M5 8H7A2 2 0 0 1 9 10V14A2 2 0 0 1 7 16H5A2 2 0 0 1 3 14V10A2 2 0 0 1 5 8Z")),
            body(d("M17 8H19A2 2 0 0 1 21 10V14A2 2 0 0 1 19 16H17A2 2 0 0 1 15 14V10A2 2 0 0 1 17 8Z")),
            line(d("M12 3V21")),
        ],
        // A padlock.
        Icon::Lock => vec![
            line(d("M8 10.5V8A4 4 0 0 1 16 8V10.5")),
            body(d("M8 10.5H16A3 3 0 0 1 19 13.5V18A3 3 0 0 1 16 21H8A3 3 0 0 1 5 18V13.5A3 3 0 0 1 8 10.5Z")),
            solid(d("M12.9 15.7A0.9 0.9 0 1 1 11.1 15.7 0.9 0.9 0 1 1 12.9 15.7Z")),
        ],
        // A pennant on its pole.
        Icon::Marker => vec![
            body(d("M6 4H19L15.5 9 19 14H6Z")),
            line(d("M6 4V21")),
        ],
        // Two pennants facing each other over a range.
        Icon::Region => vec![
            line(d("M5 4V20M19 4V20")),
            body(d("M5 4L11 7 5 10Z")),
            body(d("M19 4L13 7 19 10Z")),
            line(d("M5 17H19")).dim(),
        ],
        // A magnet.
        Icon::Snap => vec![
            body(d("M5 3.5H9.5V12A2.5 2.5 0 0 0 14.5 12V3.5H19V12A7 7 0 0 1 5 12Z")),
            line(d("M5 8H9.5M14.5 8H19")).dim(),
        ],
        // An event pushing the next one along.
        Icon::Ripple => vec![
            body(d("M5 7.5H6.5A2 2 0 0 1 8.5 9.5V14.5A2 2 0 0 1 6.5 16.5H5A2 2 0 0 1 3 14.5V9.5A2 2 0 0 1 5 7.5Z")),
            line(d("M10.5 12H14M11.88 9.88L14 12 11.88 14.12")),
            body(d("M17.5 7.5H19A2 2 0 0 1 21 9.5V14.5A2 2 0 0 1 19 16.5H17.5A2 2 0 0 1 15.5 14.5V9.5A2 2 0 0 1 17.5 7.5Z")),
        ],
        // Two fades crossing.
        Icon::Crossfade => vec![
            line(d("M3 18C9 18 15 6 21 6")),
            line(d("M3 6C9 6 15 18 21 18")).dim(),
        ],
        // A grid.
        Icon::Grid => vec![
            body(d("M6 3H18A3 3 0 0 1 21 6V18A3 3 0 0 1 18 21H6A3 3 0 0 1 3 18V6A3 3 0 0 1 6 3Z")),
            line(d("M9 3V21M15 3V21M3 9H21M3 15H21")).dim(),
        ],
        // A still camera.
        Icon::Camera => vec![
            body(d("M5 7H8L9.5 4.5A1 1 0 0 1 10.3 4H13.7A1 1 0 0 1 14.5 4.5L16 7H19A2 2 0 0 1 21 9V18A2 2 0 0 1 19 20H5A2 2 0 0 1 3 18V9A2 2 0 0 1 5 7Z")),
            line(d("M15.8 13A3.8 3.8 0 1 1 8.2 13 3.8 3.8 0 1 1 15.8 13Z")),
        ],
        // A camera with a down arrow: save a capture.
        Icon::CameraSave => vec![
            body(d("M4.48 6.48H6.7L7.81 4.63A0.74 0.74 0 0 1 8.4 4.26H10.92A0.74 0.74 0 0 1 11.51 4.63L12.62 6.48H14.84A1.48 1.48 0 0 1 16.32 7.96V14.62A1.48 1.48 0 0 1 14.84 16.1H4.48A1.48 1.48 0 0 1 3 14.62V7.96A1.48 1.48 0 0 1 4.48 6.48Z")),
            line(d("M12.47 10.92A2.81 2.81 0 1 1 6.85 10.92 2.81 2.81 0 1 1 12.47 10.92Z")),
            line(d("M19.5 12V20M21.76 17.74L19.5 20 17.24 17.74")),
        ],
        // A closed folder.
        Icon::Folder => vec![
            body(d("M3 7A3 3 0 0 1 6 4H9.3A2 2 0 0 1 10.7 4.6L12.5 6.5H18A3 3 0 0 1 21 9.5V17A3 3 0 0 1 18 20H6A3 3 0 0 1 3 17Z")),
            line(d("M3 10H21")).dim(),
        ],
        // A page.
        Icon::File => vec![
            body(d("M14 3H8A3 3 0 0 0 5 6V18A3 3 0 0 0 8 21H16A3 3 0 0 0 19 18V8Z")),
            line(d("M14 3V6A2 2 0 0 0 16 8H19")).dim(),
        ],
        // A page with a music note.
        Icon::FileAudio => vec![
            body(d("M14 3H8A3 3 0 0 0 5 6V18A3 3 0 0 0 8 21H16A3 3 0 0 0 19 18V8Z")),
            line(d("M14 3V6A2 2 0 0 0 16 8H19")).dim(),
            solid(d("M11.9 16.4A1.5 1.5 0 1 1 8.9 16.4 1.5 1.5 0 1 1 11.9 16.4Z")),
            line(d("M12.5 16.2V10M12.5 10C14.2 10.3 15.6 11.2 15.8 13")),
        ],
        // A page with a picture.
        Icon::FileImage => vec![
            body(d("M14 3H8A3 3 0 0 0 5 6V18A3 3 0 0 0 8 21H16A3 3 0 0 0 19 18V8Z")),
            line(d("M14 3V6A2 2 0 0 0 16 8H19")).dim(),
            line(d("M7.5 18.5L11 13.8 13.6 16.8 15 15.2 16.8 18.5")),
            solid(d("M15.4 11.6A0.6 0.6 0 1 1 14.2 11.6 0.6 0.6 0 1 1 15.4 11.6Z")),
        ],
        // A page with a play triangle.
        Icon::FileVideo => vec![
            body(d("M14 3H8A3 3 0 0 0 5 6V18A3 3 0 0 0 8 21H16A3 3 0 0 0 19 18V8Z")),
            line(d("M14 3V6A2 2 0 0 0 16 8H19")).dim(),
            solid(d("M10.6 12.84L14 14.8 10.6 16.76Z")),
        ],
        // An arrow up.
        Icon::Up => vec![
            line(d("M12 19.5V4.5M6.5 10L12 4.5 17.5 10")),
        ],
        // An arrow left.
        Icon::Back => vec![
            line(d("M19.5 12H4.5M10 17.5L4.5 12 10 6.5")),
        ],
        // An arrow right.
        Icon::Forward => vec![
            line(d("M4.5 12H19.5M14 6.5L19.5 12 14 17.5")),
        ],
        // A circular arrow.
        Icon::Refresh => vec![
            line(d("M18.13 17.14A8 8 0 1 1 18.13 6.86M18.49 2.67L18.13 6.86 13.94 6.49")),
        ],
        // A plus.
        Icon::Plus => vec![
            line(d("M5 12H19M12 5V19")),
        ],
        // The letters f and x: effects.
        Icon::Fx => vec![
            line(d("M8 20V9A4 4 0 0 1 12 5H13M5 12H12M14 12L20 20M20 12L14 20")),
        ],
        // A loudspeaker with sound waves.
        Icon::Speaker => vec![
            body(d("M4 9.5H7.5L12.5 5.5V18.5L7.5 14.5H4Z")),
            line(d("M15.8 9A4.5 4.5 0 0 1 15.8 15")),
            line(d("M18.5 6.3A8.5 8.5 0 0 1 18.5 17.7")).dim(),
        ],
        // Events inside a dashed group.
        Icon::Group => vec![
            dashed(d("M6 4H18A3 3 0 0 1 21 7V17A3 3 0 0 1 18 20H6A3 3 0 0 1 3 17V7A3 3 0 0 1 6 4Z")),
            body(d("M8 7.5H16A1.5 1.5 0 0 1 17.5 9V10A1.5 1.5 0 0 1 16 11.5H8A1.5 1.5 0 0 1 6.5 10V9A1.5 1.5 0 0 1 8 7.5Z")),
            body(d("M7.5 13.5H10.5A1 1 0 0 1 11.5 14.5V16A1 1 0 0 1 10.5 17H7.5A1 1 0 0 1 6.5 16V14.5A1 1 0 0 1 7.5 13.5Z")),
            body(d("M13.5 13.5H16.5A1 1 0 0 1 17.5 14.5V16A1 1 0 0 1 16.5 17H13.5A1 1 0 0 1 12.5 16V14.5A1 1 0 0 1 13.5 13.5Z")),
        ],
        // Crop corners round a framed picture.
        Icon::PanCrop => vec![
            tint(d("M7 7H17V17H7Z")),
            line(d("M7 3V15A2 2 0 0 0 9 17H21M3 7H15A2 2 0 0 1 17 9V21")),
        ],
        // Three dots.
        Icon::More => vec![
            solid(d("M6.8 12A1.3 1.3 0 1 1 4.2 12 1.3 1.3 0 1 1 6.8 12Z")),
            solid(d("M13.3 12A1.3 1.3 0 1 1 10.7 12 1.3 1.3 0 1 1 13.3 12Z")),
            solid(d("M19.8 12A1.3 1.3 0 1 1 17.2 12 1.3 1.3 0 1 1 19.8 12Z")),
        ],
        // Brackets round what is kept.
        Icon::Trim => vec![
            line(d("M7.5 4H5A1.5 1.5 0 0 0 3.5 5.5V18.5A1.5 1.5 0 0 0 5 20H7.5")),
            line(d("M16.5 4H19A1.5 1.5 0 0 1 20.5 5.5V18.5A1.5 1.5 0 0 1 19 20H16.5")),
            solid(d("M11 8.6H13A0.9 0.9 0 0 1 13.9 9.5V14.5A0.9 0.9 0 0 1 13 15.4H11A0.9 0.9 0 0 1 10.1 14.5V9.5A0.9 0.9 0 0 1 11 8.6Z")),
        ],
        // Two events and the gap between them closing.
        Icon::SmartSplit => vec![
            body(d("M4.5 8H5.5A1.5 1.5 0 0 1 7 9.5V14.5A1.5 1.5 0 0 1 5.5 16H4.5A1.5 1.5 0 0 1 3 14.5V9.5A1.5 1.5 0 0 1 4.5 8Z")),
            body(d("M18.5 8H19.5A1.5 1.5 0 0 1 21 9.5V14.5A1.5 1.5 0 0 1 19.5 16H18.5A1.5 1.5 0 0 1 17 14.5V9.5A1.5 1.5 0 0 1 18.5 8Z")),
            solid(d("M9.4 10.05L11.19 12 9.4 13.95Z")),
            solid(d("M14.6 10.05L12.81 12 14.6 13.95Z")),
        ],
        // An envelope line and a padlock.
        Icon::LockEnvelopes => vec![
            line(d("M3 10H7L10 5H14")),
            line(d("M13.5 13V11.5A2.5 2.5 0 0 1 18.5 11.5V13")),
            body(d("M13.5 13H18.5A2.5 2.5 0 0 1 21 15.5V18.5A2.5 2.5 0 0 1 18.5 21H13.5A2.5 2.5 0 0 1 11 18.5V15.5A2.5 2.5 0 0 1 13.5 13Z")),
        ],
        // A group crossed out.
        Icon::IgnoreGroups => vec![
            dashed(d("M6 4H18A3 3 0 0 1 21 7V17A3 3 0 0 1 18 20H6A3 3 0 0 1 3 17V7A3 3 0 0 1 6 4Z")).dim(),
            body(d("M8 7.5H16A1.5 1.5 0 0 1 17.5 9V10A1.5 1.5 0 0 1 16 11.5H8A1.5 1.5 0 0 1 6.5 10V9A1.5 1.5 0 0 1 8 7.5Z")).dim(),
            body(d("M7.5 13.5H10.5A1 1 0 0 1 11.5 14.5V16A1 1 0 0 1 10.5 17H7.5A1 1 0 0 1 6.5 16V14.5A1 1 0 0 1 7.5 13.5Z")).dim(),
            body(d("M13.5 13.5H16.5A1 1 0 0 1 17.5 14.5V16A1 1 0 0 1 16.5 17H13.5A1 1 0 0 1 12.5 16V14.5A1 1 0 0 1 13.5 13.5Z")).dim(),
            line(d("M4.5 4.5L19.5 19.5")),
        ],
        // Three overlapping discs of color.
        Icon::ColorGrading => vec![
            body(d("M17.5 8.5A5.5 5.5 0 1 1 6.5 8.5 5.5 5.5 0 1 1 17.5 8.5Z")),
            body(d("M13.5 15.2A5.5 5.5 0 1 1 2.5 15.2 5.5 5.5 0 1 1 13.5 15.2Z")),
            body(d("M21.5 15.2A5.5 5.5 0 1 1 10.5 15.2 5.5 5.5 0 1 1 21.5 15.2Z")),
        ],
        // A timeline with an arrow leaving it upward.
        Icon::ParentTimeline => vec![
            line(d("M9 5H6A3 3 0 0 0 3 8V16A3 3 0 0 0 6 19H18A3 3 0 0 0 21 16V8A3 3 0 0 0 18 5H15")),
            body(d("M9.5 11H14.5A1.5 1.5 0 0 1 16 12.5V15A1.5 1.5 0 0 1 14.5 16.5H9.5A1.5 1.5 0 0 1 8 15V12.5A1.5 1.5 0 0 1 9.5 11Z")),
            line(d("M12 11V3M9.45 5.55L12 3 14.55 5.55")),
        ],
        // A timeline within a timeline.
        Icon::NestedTimeline => vec![
            body(d("M6 5H18A3 3 0 0 1 21 8V16A3 3 0 0 1 18 19H6A3 3 0 0 1 3 16V8A3 3 0 0 1 6 5Z")),
            body(d("M9.5 9H14.5A1.5 1.5 0 0 1 16 10.5V13.5A1.5 1.5 0 0 1 14.5 15H9.5A1.5 1.5 0 0 1 8 13.5V10.5A1.5 1.5 0 0 1 9.5 9Z")),
        ],
        // A nested timeline with a plus.
        Icon::NewNestedTimeline => vec![
            body(d("M6 4H14A3 3 0 0 1 17 7V12A3 3 0 0 1 14 15H6A3 3 0 0 1 3 12V7A3 3 0 0 1 6 4Z")),
            body(d("M8 7.5H12A1 1 0 0 1 13 8.5V10.5A1 1 0 0 1 12 11.5H8A1 1 0 0 1 7 10.5V8.5A1 1 0 0 1 8 7.5Z")),
            line(d("M15.5 18H20.5M18 15.5V20.5")),
        ],
        // An audio lane with a plus.
        Icon::NewAudioTrack => vec![
            body(d("M5.5 5H12.5A2.5 2.5 0 0 1 15 7.5V12A2.5 2.5 0 0 1 12.5 14.5H5.5A2.5 2.5 0 0 1 3 12V7.5A2.5 2.5 0 0 1 5.5 5Z")),
            line(d("M5.8 9.8H7.2L8.5 7.2 10.2 12.3 11.5 9.8H12.4")).dim(),
            line(d("M15.5 18H20.5M18 15.5V20.5")),
        ],
        // A video lane with a plus.
        Icon::NewVideoTrack => vec![
            body(d("M5.5 5H12.5A2.5 2.5 0 0 1 15 7.5V12A2.5 2.5 0 0 1 12.5 14.5H5.5A2.5 2.5 0 0 1 3 12V7.5A2.5 2.5 0 0 1 5.5 5Z")),
            solid(d("M7.4 7.69L10.67 9.75 7.4 11.81Z")),
            line(d("M15.5 18H20.5M18 15.5V20.5")),
        ],
        // A lane of sliders with a plus: adjustments over the tracks below.
        Icon::AdjustmentTrack => vec![
            body(d("M5.5 5H12.5A2.5 2.5 0 0 1 15 7.5V12A2.5 2.5 0 0 1 12.5 14.5H5.5A2.5 2.5 0 0 1 3 12V7.5A2.5 2.5 0 0 1 5.5 5Z")),
            line(d("M5.5 8.4H12.5M5.5 11.6H12.5")).dim().fine(),
            solid(d("M8.8 8.4A0.6 0.6 0 1 1 7.6 8.4 0.6 0.6 0 1 1 8.8 8.4Z")).fine(),
            solid(d("M11 11.6A0.6 0.6 0 1 1 9.8 11.6 0.6 0.6 0 1 1 11 11.6Z")).fine(),
            line(d("M15.5 18H20.5M18 15.5V20.5")),
        ],
        // A loudspeaker beside a rising envelope.
        Icon::VolumeEnvelope => vec![
            body(d("M3 9.5H5.8L9.5 6.5V17.5L5.8 14.5H3Z")),
            line(d("M12 18H14.5L18 8H21")),
            solid(d("M15.6 18A1.1 1.1 0 1 1 13.4 18 1.1 1.1 0 1 1 15.6 18Z")),
            solid(d("M19.1 8A1.1 1.1 0 1 1 16.9 8 1.1 1.1 0 1 1 19.1 8Z")),
        ],
        // A left-right arrow over an envelope node.
        Icon::PanEnvelope => vec![
            line(d("M5 7H19M6.43 9.3L4.5 7 6.43 4.7M17.57 4.7L19.5 7 17.57 9.3")),
            line(d("M3 16H9.2M14.8 16H21")),
            body(d("M14.6 16A2.6 2.6 0 1 1 9.4 16 2.6 2.6 0 1 1 14.6 16Z")),
        ],
        // Signals merging into one.
        Icon::Bus => vec![
            line(d("M3 6H7C10.5 6 11 12 14.5 12M3 18H7C10.5 18 11 12 14.5 12")).dim(),
            line(d("M3 12H14.5")),
            line(d("M14.5 12H21M18.45 9.45L21 12 18.45 14.55")),
        ],
        // An arrow into a bus.
        Icon::InputBus => vec![
            line(d("M3 12H12.5M9.67 9.17L12.5 12 9.67 14.83")),
            body(d("M18 5H19A2 2 0 0 1 21 7V17A2 2 0 0 1 19 19H18A2 2 0 0 1 16 17V7A2 2 0 0 1 18 5Z")),
        ],
        // The letters fx with a plus: a chain of effects to assign.
        Icon::AssignableFx => vec![
            line(d("M6.6 15.24V7.32A2.88 2.88 0 0 1 9.48 4.44H10.2M4.44 9.48H9.48M10.92 9.48L15.24 15.24M15.24 9.48L10.92 15.24")),
            line(d("M15.8 18.2H20.6M18.2 15.8V20.6")),
        ],
        // The letters fx struck through.
        Icon::BypassFx => vec![
            line(d("M8 20V9A4 4 0 0 1 12 5H13M5 12H12M14 12L20 20M20 12L14 20")).dim(),
            line(d("M4 20L20 4")),
        ],
        // A fade in and out: a trapezoid.
        Icon::QuickFade => vec![
            body(d("M3.5 19L8.5 7H15.5L20.5 19Z")),
        ],
        // The letter T.
        Icon::Text => vec![
            line(d("M5 7V5H19V7M12 5V19M9 19H15")),
        ],
        // A loudspeaker crossed out.
        Icon::MuteAudio => vec![
            body(d("M3 9.5H6.5L11.5 5.5V18.5L6.5 14.5H3Z")),
            line(d("M15 9.5L20.5 15M20.5 9.5L15 15")),
        ],
        // A video lane crossed out.
        Icon::MuteVideo => vec![
            body(d("M6 6H12A3 3 0 0 1 15 9V14A3 3 0 0 1 12 17H6A3 3 0 0 1 3 14V9A3 3 0 0 1 6 6Z")),
            solid(d("M7.6 10.19L10.1 11.5 7.6 12.81Z")),
            line(d("M16.4 9.1L21.2 13.9M21.2 9.1L16.4 13.9")),
        ],
        // An event showing its first and last frames.
        Icon::EdgeFrames => vec![
            body(d("M6 6H18A3 3 0 0 1 21 9V15A3 3 0 0 1 18 18H6A3 3 0 0 1 3 15V9A3 3 0 0 1 6 6Z")),
            line(d("M6.5 8.5H9A1 1 0 0 1 10 9.5V14.5A1 1 0 0 1 9 15.5H6.5A1 1 0 0 1 5.5 14.5V9.5A1 1 0 0 1 6.5 8.5Z")).dim(),
            line(d("M15 8.5H17.5A1 1 0 0 1 18.5 9.5V14.5A1 1 0 0 1 17.5 15.5H15A1 1 0 0 1 14 14.5V9.5A1 1 0 0 1 15 8.5Z")).dim(),
        ],
        // An event with a marker of its media.
        Icon::MediaMarkers => vec![
            body(d("M6 11H18A3 3 0 0 1 21 14V17A3 3 0 0 1 18 20H6A3 3 0 0 1 3 17V14A3 3 0 0 1 6 11Z")),
            line(d("M7 11V4")),
            body(d("M7 4L13.5 6.8 7 9.6Z")),
        ],
        // A grid with a point to snap to.
        Icon::SnapGrid => vec![
            line(d("M8.5 3V21M15.5 3V21M3 8.5H21M3 15.5H21")).dim(),
            solid(d("M17.3 15.5A1.8 1.8 0 1 1 13.7 15.5 1.8 1.8 0 1 1 17.3 15.5Z")),
        ],
        // A pennant planted on a snap point.
        Icon::SnapMarkers => vec![
            body(d("M7 3.5H18L15 8 18 12.5H7Z")),
            line(d("M7 3.5V17")),
            solid(d("M8.6 19.5A1.6 1.6 0 1 1 5.4 19.5 1.6 1.6 0 1 1 8.6 19.5Z")),
        ],
        // Two events side by side with a guide between.
        Icon::SnapEvents => vec![
            body(d("M5.5 7H8.5A2.5 2.5 0 0 1 11 9.5V14.5A2.5 2.5 0 0 1 8.5 17H5.5A2.5 2.5 0 0 1 3 14.5V9.5A2.5 2.5 0 0 1 5.5 7Z")),
            body(d("M15.5 7H18.5A2.5 2.5 0 0 1 21 9.5V14.5A2.5 2.5 0 0 1 18.5 17H15.5A2.5 2.5 0 0 1 13 14.5V9.5A2.5 2.5 0 0 1 15.5 7Z")),
            line(d("M12 3.5V20.5")).dim(),
        ],
        // A ruler of frame ticks with the playhead on one.
        Icon::Quantize => vec![
            line(d("M3 17H21")).dim(),
            line(d("M5 17V13M9 17V13M15 17V13M19 17V13")).dim(),
            line(d("M12 17V9")),
            solid(d("M9.92 4.6H14.08L12 7.72Z")),
        ],
        // Two takes with the media slipping between them.
        Icon::SlipTakes => vec![
            body(d("M5 3H19A2 2 0 0 1 21 5V6A2 2 0 0 1 19 8H5A2 2 0 0 1 3 6V5A2 2 0 0 1 5 3Z")),
            body(d("M5 16H19A2 2 0 0 1 21 18V19A2 2 0 0 1 19 21H5A2 2 0 0 1 3 19V18A2 2 0 0 1 5 16Z")),
            line(d("M6.5 12H17.5M7.41 13.69L6 12 7.41 10.31M16.59 10.31L18 12 16.59 13.69")),
        ],
        // A viewfinder locked on a target.
        Icon::MotionTracking => vec![
            line(d("M3 8V5A2 2 0 0 1 5 3H8M16 3H19A2 2 0 0 1 21 5V8M21 16V19A2 2 0 0 1 19 21H16M8 21H5A2 2 0 0 1 3 19V16")),
            line(d("M16.5 12A4.5 4.5 0 1 1 7.5 12 4.5 4.5 0 1 1 16.5 12Z")).dim(),
            solid(d("M13.4 12A1.4 1.4 0 1 1 10.6 12 1.4 1.4 0 1 1 13.4 12Z")),
        ],
        // A film strip cut into scenes.
        Icon::DetectScenes => vec![
            body(d("M5.5 7H18.5A2.5 2.5 0 0 1 21 9.5V14.5A2.5 2.5 0 0 1 18.5 17H5.5A2.5 2.5 0 0 1 3 14.5V9.5A2.5 2.5 0 0 1 5.5 7Z")),
            line(d("M9 4V20M15 4V20")).dim(),
        ],
        // A floppy disk with a pencil: save under another name.
        Icon::SaveAs => vec![
            body(d("M4.98 3H11.91L14.88 5.97V12.9A1.98 1.98 0 0 1 12.9 14.88H4.98A1.98 1.98 0 0 1 3 12.9V4.98A1.98 1.98 0 0 1 4.98 3Z")),
            line(d("M6.3 3V5.64A0.66 0.66 0 0 0 6.96 6.3H10.59A0.66 0.66 0 0 0 11.25 5.64V3")).dim(),
            body(d("M20.27 13.14L21.86 14.73 16.71 19.88 14.33 20.67 15.12 18.29Z")),
            line(d("M18.89 14.53L20.47 16.11")).dim(),
        ],
        // A floppy disk with a plus: save a new version.
        Icon::SaveIncremental => vec![
            body(d("M5.16 3H12.72L15.96 6.24V13.8A2.16 2.16 0 0 1 13.8 15.96H5.16A2.16 2.16 0 0 1 3 13.8V5.16A2.16 2.16 0 0 1 5.16 3Z")),
            line(d("M6.6 3V5.88A0.72 0.72 0 0 0 7.32 6.6H11.28A0.72 0.72 0 0 0 12 5.88V3")).dim(),
            line(d("M15.5 18H20.5M18 15.5V20.5")),
        ],
        // A piano keyboard.
        Icon::Midi => vec![
            body(d("M6 5H18A3 3 0 0 1 21 8V16A3 3 0 0 1 18 19H6A3 3 0 0 1 3 16V8A3 3 0 0 1 6 5Z")),
            line(d("M7.5 13V19M12 13V19M16.5 13V19")).dim(),
            solid(d("M7 5.6H8A0.4 0.4 0 0 1 8.4 6V12A0.4 0.4 0 0 1 8 12.4H7A0.4 0.4 0 0 1 6.6 12V6A0.4 0.4 0 0 1 7 5.6Z")),
            solid(d("M11.5 5.6H12.5A0.4 0.4 0 0 1 12.9 6V12A0.4 0.4 0 0 1 12.5 12.4H11.5A0.4 0.4 0 0 1 11.1 12V6A0.4 0.4 0 0 1 11.5 5.6Z")),
            solid(d("M16 5.6H17A0.4 0.4 0 0 1 17.4 6V12A0.4 0.4 0 0 1 17 12.4H16A0.4 0.4 0 0 1 15.6 12V6A0.4 0.4 0 0 1 16 5.6Z")),
        ],
        // A metronome.
        Icon::Metronome => vec![
            body(d("M9.5 3H14.5L19 21H5Z")),
            line(d("M12 17L15.5 7.5")),
            solid(d("M15.5 10.8A1.2 1.2 0 1 1 13.1 10.8 1.2 1.2 0 1 1 15.5 10.8Z")),
        ],
        // A disc.
        Icon::Disc => vec![
            body(d("M21 12A9 9 0 1 1 3 12 9 9 0 1 1 21 12Z")),
            line(d("M14.4 12A2.4 2.4 0 1 1 9.6 12 2.4 2.4 0 1 1 14.4 12Z")),
            line(d("M6.01 10.4A6.2 6.2 0 0 1 10.4 6.01")).dim(),
        ],
        // An arrow into a tray.
        Icon::Import => vec![
            line(d("M12 3.5V14.5M7.5 10L12 14.5 16.5 10")),
            body(d("M4 14V18A3 3 0 0 0 7 21H17A3 3 0 0 0 20 18V14")),
        ],
        // A window whose side panel hides.
        Icon::Dock => vec![
            body(d("M6 4H18A3 3 0 0 1 21 7V17A3 3 0 0 1 18 20H6A3 3 0 0 1 3 17V7A3 3 0 0 1 6 4Z")),
            tint(d("M3 7A3 3 0 0 1 6 4H9V20H6A3 3 0 0 1 3 17Z")),
            line(d("M9 4V20")).dim(),
            line(d("M6.8 9.8L4.8 12 6.8 14.2")),
        ],
        // Two pictures, one behind the other.
        Icon::Slideshow => vec![
            line(d("M7 7V6A2 2 0 0 1 9 4H19A2 2 0 0 1 21 6V13A2 2 0 0 1 19 15H18")).dim(),
            body(d("M5.5 8H15.5A2.5 2.5 0 0 1 18 10.5V17.5A2.5 2.5 0 0 1 15.5 20H5.5A2.5 2.5 0 0 1 3 17.5V10.5A2.5 2.5 0 0 1 5.5 8Z")),
            line(d("M5.5 17.5L9 13 12 16.5 13.5 15 15.5 17.5")),
            solid(d("M14.5 11.2A0.5 0.5 0 1 1 13.5 11.2 0.5 0.5 0 1 1 14.5 11.2Z")),
        ],
        // A speech bubble.
        Icon::Speech => vec![
            body(d("M6 4H18A3 3 0 0 1 21 7V13A3 3 0 0 1 18 16H12L7.5 20V16H6A3 3 0 0 1 3 13V7A3 3 0 0 1 6 4Z")),
            line(d("M8 8.5H16M8 11.5H13")).dim(),
        ],
        // A big sparkle and a small one.
        Icon::Sparkle => vec![
            body(d("M10.5 5.5C11.17 10.17 13.83 12.83 18.5 13.5 13.83 14.17 11.17 16.83 10.5 21.5 9.83 16.83 7.17 14.17 2.5 13.5 7.17 12.83 9.83 10.17 10.5 5.5Z")),
            solid(d("M18 3.2C18.27 4.8 19.2 5.73 20.8 6 19.2 6.27 18.27 7.2 18 8.8 17.73 7.2 16.8 6.27 15.2 6 16.8 5.73 17.73 4.8 18 3.2Z")),
        ],
        // Sync arrows round a waveform.
        Icon::AudioSync => vec![
            line(d("M4.01 9.09A8.5 8.5 0 0 1 19.99 9.09M21.51 5.83L19.99 9.09 16.72 7.57M19.99 14.91A8.5 8.5 0 0 1 4.01 14.91M2.49 18.17L4.01 14.91 7.28 16.43")),
            line(d("M7.5 12H9.4L10.8 8.5 13.2 15.5 14.6 12H16.5")),
        ],
        // A page with code brackets.
        Icon::Script => vec![
            body(d("M14 3H8A3 3 0 0 0 5 6V18A3 3 0 0 0 8 21H16A3 3 0 0 0 19 18V8Z")),
            line(d("M14 3V6A2 2 0 0 0 16 8H19")).dim(),
            line(d("M10.3 12.4L8 14.9 10.3 17.4M13.7 12.4L16 14.9 13.7 17.4")),
        ],
        // An arrow into a frame: take the media's settings.
        Icon::MatchMedia => vec![
            body(d("M12 5H18A3 3 0 0 1 21 8V16A3 3 0 0 1 18 19H12A3 3 0 0 1 9 16V8A3 3 0 0 1 12 5Z")),
            line(d("M2.5 12H11.5M8.67 9.17L11.5 12 8.67 14.83")),
        ],
        // A pipette.
        Icon::Eyedropper => vec![
            body(d("M13.41 7.76L7.76 13.41 5.99 18.01 10.59 16.24 16.24 10.59")),
            line(d("M12.35 6.7L17.3 11.65")),
            body(d("M13.41 7.76L15.54 5.64A2 2 45 0 1 18.36 8.46L16.24 10.59Z")),
        ],
        // A sight.
        Icon::Crosshair => vec![
            line(d("M18.5 12A6.5 6.5 0 1 1 5.5 12 6.5 6.5 0 1 1 18.5 12Z")),
            line(d("M12 3V7M12 17V21M3 12H7M17 12H21")),
            solid(d("M12.9 12A0.9 0.9 0 1 1 11.1 12 0.9 0.9 0 1 1 12.9 12Z")),
        ],
        // Two levels with a range between.
        Icon::Limits => vec![
            line(d("M4 5H20M4 19H20")),
            line(d("M12 8.5V15.5M9.7 9.93L12 8 14.3 9.93M14.3 14.07L12 16 9.7 14.07")),
        ],
        // A monitor with an arrow going out.
        Icon::ExternalMonitor => vec![
            body(d("M5.5 4H18.5A2.5 2.5 0 0 1 21 6.5V13.5A2.5 2.5 0 0 1 18.5 16H5.5A2.5 2.5 0 0 1 3 13.5V6.5A2.5 2.5 0 0 1 5.5 4Z")),
            line(d("M9 20H15M12 16V20")),
            line(d("M9.5 12.5L14.5 7.5M10.2 7.5H14.5V11.8")),
        ],
        // A solid star.
        Icon::StarFilled => vec![
            solid(d("M12 3.6L14.53 9.42 20.84 10.03 16.09 14.23 17.47 20.42 12 17.2 6.53 20.42 7.91 14.23 3.16 10.03 9.47 9.42Z")),
        ],
        // A star.
        Icon::Star => vec![
            body(d("M12 2.6L14.88 8.94 21.8 9.72 16.66 14.41 18.05 21.23 12 17.8 5.95 21.23 7.34 14.41 2.2 9.72 9.12 8.94Z")),
        ],
        // A window with a list.
        Icon::Views => vec![
            body(d("M6 4H18A3 3 0 0 1 21 7V17A3 3 0 0 1 18 20H6A3 3 0 0 1 3 17V7A3 3 0 0 1 6 4Z")),
            line(d("M3 9H21")).dim(),
            solid(d("M7.9 13A0.5 0.5 0 1 1 6.9 13 0.5 0.5 0 1 1 7.9 13Z")),
            line(d("M10.5 13H17")),
            solid(d("M7.9 17A0.5 0.5 0 1 1 6.9 17 0.5 0.5 0 1 1 7.9 17Z")),
            line(d("M10.5 17H15")),
        ],
        // A checkerboard: generated media.
        Icon::Generator => vec![
            body(d("M6 3H18A3 3 0 0 1 21 6V18A3 3 0 0 1 18 21H6A3 3 0 0 1 3 18V6A3 3 0 0 1 6 3Z")),
            solid(d("M13 6.6H17A0.4 0.4 0 0 1 17.4 7V11A0.4 0.4 0 0 1 17 11.4H13A0.4 0.4 0 0 1 12.6 11V7A0.4 0.4 0 0 1 13 6.6Z")),
            solid(d("M7 12.6H11A0.4 0.4 0 0 1 11.4 13V17A0.4 0.4 0 0 1 11 17.4H7A0.4 0.4 0 0 1 6.6 17V13A0.4 0.4 0 0 1 7 12.6Z")),
        ],
        // A bow tie between two edges: a transition.
        Icon::Transition => vec![
            line(d("M6 4H18A3 3 0 0 1 21 7V17A3 3 0 0 1 18 20H6A3 3 0 0 1 3 17V7A3 3 0 0 1 6 4Z")).dim(),
            body(d("M6 7.5L11 12 6 16.5Z")),
            body(d("M18 7.5L13 12 18 16.5Z")),
        ],
        // A magnifier with a play triangle: play on click.
        Icon::AutoPreview => vec![
            body(d("M16.5 10A6.5 6.5 0 1 1 3.5 10 6.5 6.5 0 1 1 16.5 10Z")),
            line(d("M15 15L20.5 20.5")),
            solid(d("M8.2 7.52L11.92 10 8.2 12.48Z")),
        ],
        // A media tile crossed with a red mark.
        Icon::RemoveUnused => vec![
            body(d("M6 3H13A3 3 0 0 1 16 6V13A3 3 0 0 1 13 16H6A3 3 0 0 1 3 13V6A3 3 0 0 1 6 3Z")),
            line(d("M5.5 13L8.5 8.8 10.5 11.2 11.8 9.8 13.5 13")).dim(),
            line(d("M15.5 15.5L20.5 20.5M20.5 15.5L15.5 20.5")).red(),
        ],
        // A camcorder with a red recording dot.
        Icon::Capture => vec![
            body(d("M5.5 7H12.5A2.5 2.5 0 0 1 15 9.5V14.5A2.5 2.5 0 0 1 12.5 17H5.5A2.5 2.5 0 0 1 3 14.5V9.5A2.5 2.5 0 0 1 5.5 7Z")),
            body(d("M15 11L21 8V16L15 13Z")),
            solid(d("M7.3 10.7A0.8 0.8 0 1 1 5.7 10.7 0.8 0.8 0 1 1 7.3 10.7Z")).red(),
        ],
        // Two arrows going opposite ways.
        Icon::Swap => vec![
            line(d("M4 8H20M17.03 5.03L20 8 17.03 10.97")),
            line(d("M20 16H4M6.97 18.97L4 16 6.97 13.03")),
        ],
        // A strip of film.
        Icon::FilmStrip => vec![
            body(d("M6 5H18A3 3 0 0 1 21 8V16A3 3 0 0 1 18 19H6A3 3 0 0 1 3 16V8A3 3 0 0 1 6 5Z")),
            line(d("M8.5 9.2H15.5A1 1 0 0 1 16.5 10.2V13.8A1 1 0 0 1 15.5 14.8H8.5A1 1 0 0 1 7.5 13.8V10.2A1 1 0 0 1 8.5 9.2Z")).dim(),
            solid(d("M6.2 7.2A0.2 0.2 0 1 1 5.8 7.2 0.2 0.2 0 1 1 6.2 7.2Z")).fine(),
            solid(d("M10.2 7.2A0.2 0.2 0 1 1 9.8 7.2 0.2 0.2 0 1 1 10.2 7.2Z")).fine(),
            solid(d("M14.2 7.2A0.2 0.2 0 1 1 13.8 7.2 0.2 0.2 0 1 1 14.2 7.2Z")).fine(),
            solid(d("M18.2 7.2A0.2 0.2 0 1 1 17.8 7.2 0.2 0.2 0 1 1 18.2 7.2Z")).fine(),
            solid(d("M6.2 16.8A0.2 0.2 0 1 1 5.8 16.8 0.2 0.2 0 1 1 6.2 16.8Z")).fine(),
            solid(d("M10.2 16.8A0.2 0.2 0 1 1 9.8 16.8 0.2 0.2 0 1 1 10.2 16.8Z")).fine(),
            solid(d("M14.2 16.8A0.2 0.2 0 1 1 13.8 16.8 0.2 0.2 0 1 1 14.2 16.8Z")).fine(),
            solid(d("M18.2 16.8A0.2 0.2 0 1 1 17.8 16.8 0.2 0.2 0 1 1 18.2 16.8Z")).fine(),
        ],
        // A frame split in two.
        Icon::SplitScreen => vec![
            body(d("M6 5H18A3 3 0 0 1 21 8V16A3 3 0 0 1 18 19H6A3 3 0 0 1 3 16V8A3 3 0 0 1 6 5Z")),
            tint(d("M6 5H12V19H6A3 3 0 0 1 3 16V8A3 3 0 0 1 6 5Z")),
            line(d("M12 5V19")),
        ],
        // A globe with a ring round it: the view all round.
        Icon::View360 => vec![
            line(d("M2.73 15.75C2.71 15.67 2.64 15.44 2.63 15.28 2.63 15.11 2.65 14.94 2.7 14.75 2.74 14.57 2.82 14.38 2.92 14.18 3.02 13.99 3.15 13.78 3.3 13.58 3.45 13.37 3.62 13.15 3.82 12.94 4.02 12.73 4.25 12.51 4.49 12.29 4.73 12.07 5.15 11.74 5.29 11.63M16.57 7.07C16.75 7.06 17.28 7 17.6 6.99 17.93 6.98 18.24 6.98 18.53 7 18.83 7.01 19.1 7.04 19.36 7.09 19.61 7.13 19.84 7.19 20.05 7.26 20.26 7.33 20.45 7.42 20.61 7.52 20.77 7.62 20.91 7.73 21.02 7.85 21.13 7.97 21.23 8.19 21.27 8.25")).dim(),
            body(d("M17.2 12A5.2 5.2 0 1 1 6.8 12 5.2 5.2 0 1 1 17.2 12Z")),
            line(d("M21.27 8.25C21.28 8.39 21.39 8.79 21.34 9.09 21.28 9.39 21.15 9.73 20.94 10.07 20.73 10.42 20.44 10.79 20.09 11.15 19.74 11.52 19.31 11.9 18.84 12.27 18.36 12.65 17.81 13.02 17.24 13.38 16.66 13.74 16.03 14.09 15.38 14.42 14.73 14.75 14.04 15.06 13.35 15.34 12.66 15.62 11.94 15.87 11.25 16.09 10.56 16.3 9.86 16.49 9.19 16.63 8.53 16.78 7.87 16.88 7.27 16.95 6.67 17.01 6.1 17.03 5.59 17.01 5.08 16.99 4.62 16.93 4.23 16.82 3.84 16.72 3.51 16.57 3.26 16.39 3.01 16.22 2.82 15.85 2.73 15.75")),
        ],
        // A ring with half of it lit: dynamic range.
        Icon::Hdr => vec![
            line(d("M21 12A9 9 0 1 1 3 12 9 9 0 1 1 21 12Z")),
            solid(d("M12 6A6 6 0 0 1 12 18Z")),
        ],
        // A screen with a play triangle.
        Icon::PreviewSize => vec![
            body(d("M6 5H18A3 3 0 0 1 21 8V16A3 3 0 0 1 18 19H6A3 3 0 0 1 3 16V8A3 3 0 0 1 6 5Z")),
            solid(d("M10.6 10.01L14.25 12 10.6 13.99Z")),
        ],
        // A film frame, numbered.
        Icon::FrameNumber => vec![
            body(d("M6 5H18A3 3 0 0 1 21 8V16A3 3 0 0 1 18 19H6A3 3 0 0 1 3 16V8A3 3 0 0 1 6 5Z")),
            line(d("M6 8.5V10.5M6 13.5V15.5M18 8.5V10.5M18 13.5V15.5")).dim().fine(),
            line(d("M10.6 9L9.8 15M14.4 9L13.6 15M8.9 10.9H15.4M8.5 13.1H15")),
        ],
        // Four corners: the display's size.
        Icon::DisplaySize => vec![
            line(d("M3 8V5A2 2 0 0 1 5 3H8M16 3H19A2 2 0 0 1 21 5V8M21 16V19A2 2 0 0 1 19 21H16M8 21H5A2 2 0 0 1 3 19V16")),
        ],
        // Many channels merging into two.
        Icon::Downmix => vec![
            line(d("M5 6L19 9M5 12L19 9M5 12L19 15M5 18L19 15")).dim(),
            solid(d("M6.4 6A1.4 1.4 0 1 1 3.6 6 1.4 1.4 0 1 1 6.4 6Z")),
            solid(d("M6.4 12A1.4 1.4 0 1 1 3.6 12 1.4 1.4 0 1 1 6.4 12Z")),
            solid(d("M6.4 18A1.4 1.4 0 1 1 3.6 18 1.4 1.4 0 1 1 6.4 18Z")),
            solid(d("M20.4 9A1.4 1.4 0 1 1 17.6 9 1.4 1.4 0 1 1 20.4 9Z")),
            solid(d("M20.4 15A1.4 1.4 0 1 1 17.6 15 1.4 1.4 0 1 1 20.4 15Z")),
        ],
        // A loudspeaker with a down arrow.
        Icon::Dim => vec![
            body(d("M3 9.5H6.5L11 6V18L6.5 14.5H3Z")),
            line(d("M17 5.5V18M19.97 15.03L17 18 14.03 15.03")),
        ],
        // Three faders.
        Icon::MixingConsole => vec![
            line(d("M6 4V20M12 4V20M18 4V20")).dim(),
            body(d("M5 7H7A1.5 1.5 0 0 1 8.5 8.5V9.5A1.5 1.5 0 0 1 7 11H5A1.5 1.5 0 0 1 3.5 9.5V8.5A1.5 1.5 0 0 1 5 7Z")),
            body(d("M11 13H13A1.5 1.5 0 0 1 14.5 14.5V15.5A1.5 1.5 0 0 1 13 17H11A1.5 1.5 0 0 1 9.5 15.5V14.5A1.5 1.5 0 0 1 11 13Z")),
            body(d("M17 8.5H19A1.5 1.5 0 0 1 20.5 10V11A1.5 1.5 0 0 1 19 12.5H17A1.5 1.5 0 0 1 15.5 11V10A1.5 1.5 0 0 1 17 8.5Z")),
        ],
        // A bus picked.
        Icon::BusSelect => vec![
            body(d("M8.5 4H15.5A4.5 4.5 0 0 1 20 8.5V15.5A4.5 4.5 0 0 1 15.5 20H8.5A4.5 4.5 0 0 1 4 15.5V8.5A4.5 4.5 0 0 1 8.5 4Z")),
            solid(d("M11 9.6H13A1.4 1.4 0 0 1 14.4 11V13A1.4 1.4 0 0 1 13 14.4H11A1.4 1.4 0 0 1 9.6 13V11A1.4 1.4 0 0 1 11 9.6Z")),
        ],
        // A path from one node to another.
        Icon::Routing => vec![
            body(d("M7.5 7A2.5 2.5 0 1 1 2.5 7 2.5 2.5 0 1 1 7.5 7Z")),
            body(d("M22 17A2.5 2.5 0 1 1 17 17 2.5 2.5 0 1 1 22 17Z")),
            line(d("M7.5 7H11A3 3 0 0 1 14 10V14A3 3 0 0 0 17 17")),
        ],
        // A graduation cap.
        Icon::Mortarboard => vec![
            body(d("M2.5 9.5L12 5 21.5 9.5 12 14Z")),
            line(d("M6.5 12V16C6.5 17.5 9 19 12 19 15 19 17.5 17.5 17.5 16V12")),
            line(d("M21.5 9.5V15")),
            solid(d("M22.2 16.4A0.7 0.7 0 1 1 20.8 16.4 0.7 0.7 0 1 1 22.2 16.4Z")),
        ],
        // A globe.
        Icon::Globe => vec![
            body(d("M21 12A9 9 0 1 1 3 12 9 9 0 1 1 21 12Z")),
            line(d("M12 3A4 9 0 0 1 12 21 4 9 0 0 1 12 3")).dim(),
            line(d("M3 12H21")).dim(),
        ],
        // Source code: angle brackets and a slash.
        Icon::Code => vec![
            line(d("M8 6.5L2.5 12 8 17.5")),
            line(d("M16 6.5L21.5 12 16 17.5")),
            line(d("M14 4.5L10 19.5")).dim(),
        ],
        // Two people.
        Icon::People => vec![
            body(d("M12 8A3 3 0 1 1 6 8 3 3 0 1 1 12 8Z")),
            line(d("M3.5 20C3.5 16.5 6 14.5 9 14.5 12 14.5 14.5 16.5 14.5 20")),
            line(d("M19.3 9A2.3 2.3 0 1 1 14.7 9 2.3 2.3 0 1 1 19.3 9Z")).dim(),
            line(d("M15.5 14.7C18.2 14.7 20.5 16.5 20.5 20")).dim(),
        ],
        // A speech bubble with a heart.
        Icon::HeartBubble => vec![
            body(d("M6 4H18A3 3 0 0 1 21 7V13A3 3 0 0 1 18 16H12L7.5 20V16H6A3 3 0 0 1 3 13V7A3 3 0 0 1 6 4Z")),
            solid(d("M12 13.6C12 13.6 8.6 11.7 8.6 9.4A1.9 1.9 0 0 1 12 8.4 1.9 1.9 0 0 1 15.4 9.4C15.4 11.7 12 13.6 12 13.6Z")),
        ],
        // A 16:9 screen.
        Icon::ShapeWide => vec![
            body(d("M5.5 4.5H18.5A2.5 2.5 0 0 1 21 7V13.5A2.5 2.5 0 0 1 18.5 16H5.5A2.5 2.5 0 0 1 3 13.5V7A2.5 2.5 0 0 1 5.5 4.5Z")),
            line(d("M9 20H15M12 16V20")),
        ],
        // A 4:3 screen.
        Icon::ShapeFull => vec![
            body(d("M7 3.5H17A2.5 2.5 0 0 1 19.5 6V13.5A2.5 2.5 0 0 1 17 16H7A2.5 2.5 0 0 1 4.5 13.5V6A2.5 2.5 0 0 1 7 3.5Z")),
            line(d("M9 20H15M12 16V20")),
        ],
        // A phone.
        Icon::ShapePortrait => vec![
            body(d("M9.5 2.5H14.5A2.5 2.5 0 0 1 17 5V19A2.5 2.5 0 0 1 14.5 21.5H9.5A2.5 2.5 0 0 1 7 19V5A2.5 2.5 0 0 1 9.5 2.5Z")),
            line(d("M10.5 18.5H13.5")).dim(),
        ],
        // A square frame.
        Icon::ShapeSquare => vec![
            body(d("M8 4.5H16A3.5 3.5 0 0 1 19.5 8V16A3.5 3.5 0 0 1 16 19.5H8A3.5 3.5 0 0 1 4.5 16V8A3.5 3.5 0 0 1 8 4.5Z")),
        ],
        // A wide cinema frame.
        Icon::ShapeScope => vec![
            body(d("M5 8H19A2.5 2.5 0 0 1 21.5 10.5V13.7A2.5 2.5 0 0 1 19 16.2H5A2.5 2.5 0 0 1 2.5 13.7V10.5A2.5 2.5 0 0 1 5 8Z")),
        ],
        // A scope screen and a play triangle.
        Icon::ScopesWhilePlaying => vec![
            body(d("M5.5 3H12.5A2.5 2.5 0 0 1 15 5.5V10.5A2.5 2.5 0 0 1 12.5 13H5.5A2.5 2.5 0 0 1 3 10.5V5.5A2.5 2.5 0 0 1 5.5 3Z")),
            line(d("M5.6 9.6L7.6 6.4 9.6 10.4 11 7.6 12.4 9.6")).dim().fine(),
            solid(d("M16.1 16.01L19.75 18 16.1 19.99Z")),
        ],
        // A smooth curve with a node on its crest.
        Icon::Automation => vec![
            line(d("M3 17C8 17 9 7 12 7 15 7 16 17 21 17")),
            solid(d("M13.6 7A1.6 1.6 0 1 1 10.4 7 1.6 1.6 0 1 1 13.6 7Z")),
        ],
        Icon::Phase => vec![
            line(d("M3 5H6C10 5 14 19 18 19H21")),
            line(d("M3 19H6C10 19 14 5 18 5H21")),
        ],
    }
}

fn layer(fill: Fill, edge: Edge, subs: Vec<Sub>) -> Layer {
    Layer {
        fill,
        edge,
        ink: Ink::Icon,
        fine: false,
        dim: false,
        subs,
    }
}

/// An outline over a soft tint: a thing with a body.
fn body(subs: Vec<Sub>) -> Layer {
    layer(Fill::Soft, Edge::Line, subs)
}

/// An outline.
fn line(subs: Vec<Sub>) -> Layer {
    layer(Fill::None, Edge::Line, subs)
}

/// A soft tint alone.
fn tint(subs: Vec<Sub>) -> Layer {
    layer(Fill::Soft, Edge::None, subs)
}

/// A solid shape with rounded corners.
fn solid(subs: Vec<Sub>) -> Layer {
    layer(Fill::Full, Edge::Thin, subs)
}

/// A dashed outline.
fn dashed(subs: Vec<Sub>) -> Layer {
    layer(Fill::None, Edge::Dashed, subs)
}

/// Parses path data.
fn d(data: &str) -> Vec<Sub> {
    path::parse(data)
}

impl Layer {
    /// The same layer in red.
    fn red(mut self) -> Layer {
        self.ink = Ink::Red;
        self
    }

    /// The same layer at half strength: a supporting detail.
    fn dim(mut self) -> Layer {
        self.dim = true;
        self
    }

    /// The same layer, drawn only where there is room for it.
    fn fine(mut self) -> Layer {
        self.fine = true;
        self
    }
}

#[cfg(test)]
pub(super) const ALL: [Icon; 133] = [
    Icon::New,
    Icon::Open,
    Icon::Save,
    Icon::Render,
    Icon::Settings,
    Icon::Cut,
    Icon::Copy,
    Icon::Paste,
    Icon::Undo,
    Icon::Redo,
    Icon::Play,
    Icon::Pause,
    Icon::Stop,
    Icon::Record,
    Icon::Loop,
    Icon::PlayFromStart,
    Icon::GoStart,
    Icon::GoEnd,
    Icon::PrevFrame,
    Icon::NextFrame,
    Icon::EditTool,
    Icon::EnvelopeTool,
    Icon::SelectTool,
    Icon::ZoomTool,
    Icon::Delete,
    Icon::TrimStart,
    Icon::TrimEnd,
    Icon::Split,
    Icon::Lock,
    Icon::Marker,
    Icon::Region,
    Icon::Snap,
    Icon::Ripple,
    Icon::Crossfade,
    Icon::Grid,
    Icon::Camera,
    Icon::CameraSave,
    Icon::Folder,
    Icon::File,
    Icon::FileAudio,
    Icon::FileImage,
    Icon::FileVideo,
    Icon::Up,
    Icon::Back,
    Icon::Forward,
    Icon::Refresh,
    Icon::Plus,
    Icon::Fx,
    Icon::Speaker,
    Icon::Group,
    Icon::PanCrop,
    Icon::More,
    Icon::Trim,
    Icon::SmartSplit,
    Icon::LockEnvelopes,
    Icon::IgnoreGroups,
    Icon::ColorGrading,
    Icon::ParentTimeline,
    Icon::NestedTimeline,
    Icon::NewNestedTimeline,
    Icon::NewAudioTrack,
    Icon::NewVideoTrack,
    Icon::AdjustmentTrack,
    Icon::VolumeEnvelope,
    Icon::PanEnvelope,
    Icon::Bus,
    Icon::InputBus,
    Icon::AssignableFx,
    Icon::BypassFx,
    Icon::QuickFade,
    Icon::Text,
    Icon::MuteAudio,
    Icon::MuteVideo,
    Icon::EdgeFrames,
    Icon::MediaMarkers,
    Icon::SnapGrid,
    Icon::SnapMarkers,
    Icon::SnapEvents,
    Icon::Quantize,
    Icon::SlipTakes,
    Icon::MotionTracking,
    Icon::DetectScenes,
    Icon::SaveAs,
    Icon::SaveIncremental,
    Icon::Midi,
    Icon::Metronome,
    Icon::Disc,
    Icon::Import,
    Icon::Dock,
    Icon::Slideshow,
    Icon::Speech,
    Icon::Sparkle,
    Icon::AudioSync,
    Icon::Script,
    Icon::MatchMedia,
    Icon::Eyedropper,
    Icon::Crosshair,
    Icon::Limits,
    Icon::ExternalMonitor,
    Icon::StarFilled,
    Icon::Star,
    Icon::Views,
    Icon::Generator,
    Icon::Transition,
    Icon::AutoPreview,
    Icon::RemoveUnused,
    Icon::Capture,
    Icon::Swap,
    Icon::FilmStrip,
    Icon::SplitScreen,
    Icon::View360,
    Icon::Hdr,
    Icon::PreviewSize,
    Icon::FrameNumber,
    Icon::DisplaySize,
    Icon::Downmix,
    Icon::Dim,
    Icon::MixingConsole,
    Icon::BusSelect,
    Icon::Routing,
    Icon::Mortarboard,
    Icon::Globe,
    Icon::Code,
    Icon::People,
    Icon::HeartBubble,
    Icon::ShapeWide,
    Icon::ShapeFull,
    Icon::ShapePortrait,
    Icon::ShapeSquare,
    Icon::ShapeScope,
    Icon::ScopesWhilePlaying,
    Icon::Automation,
    Icon::Phase,
];
