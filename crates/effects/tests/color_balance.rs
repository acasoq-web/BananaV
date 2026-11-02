//! Color Balance through BananaV's OpenFX host, as any host runs it:
//! described as VEGAS's (its settings, ranges and presets) and the picture
//! through it: the colors pushed toward a channel where Type says, the
//! channels' mean kept.

use std::sync::{Arc, OnceLock};

use bv_core::ImageRgbaF32;
use bv_ofx::{
    Animation, Binary, ChangeReason, ClipSource, Context, Instance, Plugin, RenderRequest, Setup,
    Value,
};

const BALANCE: &str = "org.bananav.ColorBalance";

fn plugin() -> &'static Arc<Plugin> {
    static PLUGINS: OnceLock<Vec<Arc<Plugin>>> = OnceLock::new();
    PLUGINS
        .get_or_init(|| {
            let bin = Binary::built_in(
                bv_effects::BINARY,
                bv_effects::number_of_plugins,
                bv_effects::get_plugin,
                None,
            );
            let (plugins, failures) = bin.plugins().expect("BananaV's effects load");
            assert!(failures.is_empty(), "{failures:?}");
            plugins
        })
        .iter()
        .find(|p| p.info.identifier == BALANCE)
        .expect(BALANCE)
}

/// A still picture as a clip.
struct Still(Arc<ImageRgbaF32>);

impl ClipSource for Still {
    fn picture(&self, _time: f64) -> Option<Arc<ImageRgbaF32>> {
        Some(Arc::clone(&self.0))
    }

    fn full_size(&self) -> (f64, f64) {
        (self.0.width as f64, self.0.height as f64)
    }
}

fn instance() -> Instance {
    let setup = Setup {
        frame_rate: 30.0,
        duration: 100.0,
        size: (4.0, 2.0),
        pixel_aspect: 1.0,
        interactive: true,
    };
    Instance::new(plugin(), Context::Filter, &setup).expect("an instance")
}

fn render(inst: &Instance, src: &Arc<ImageRgbaF32>) -> ImageRgbaF32 {
    let source: Arc<dyn ClipSource> = Arc::new(Still(Arc::clone(src)));
    let req = RenderRequest {
        time: 0.0,
        width: src.width,
        height: src.height,
        scale: 1.0,
        draft: false,
        interactive: true,
        sequential: false,
        timeline: None,
    };
    inst.render_with(&[("Source", source)], &[], &req)
        .expect("it renders")
}

fn set(inst: &Instance, name: &str, v: f64) {
    inst.set_values(&[(name.into(), Animation::constant(Value::Num(vec![v])))]);
    inst.changed(&[name.into()], ChangeReason::User, 0.0);
}

#[test]
fn color_balance_describes_itself_as_vegas_does() {
    let info = &plugin().info;
    assert_eq!(info.label, "Color Balance");
    assert_eq!(info.grouping, "BananaV/Color");
    let filter = info.context(Context::Filter).unwrap();
    let labels: Vec<_> = filter.params.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(
        labels,
        ["Red", "Green", "Blue", "Type", "Preserve luminosity"]
    );
    for p in &filter.params[..3] {
        // 0 in the middle, 0.1 a tenth of the slider's way, as VEGAS's.
        assert_eq!(
            (p.display_min[0], p.display_max[0], p.default.num(0)),
            (-0.5, 0.5, 0.0)
        );
    }
    let kind = &filter.params[3];
    assert_eq!(kind.options, ["Highlights", "Midtones", "Shadow"]);
    assert_eq!(kind.default.num(0), 1.0);
    assert_eq!(filter.params[4].default.num(0), 1.0);
    let names: Vec<_> = bv_effects::presets(BALANCE)
        .iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(
        names,
        [
            "Red Midtones",
            "Green Midtones",
            "Blue Midtones",
            "Red Highlight",
            "Green Highlight",
            "Blue Highlight"
        ]
    );
}

#[test]
fn the_colors_move_where_type_says() {
    // Greys from black to white, the last half see-through.
    let mut img = ImageRgbaF32::transparent(4, 2);
    for x in 0..4 {
        let v = [0.0, 0.25, 0.5, 1.0][x as usize];
        *img.pixel_mut(x, 0) = [v, v, v, 1.0];
        *img.pixel_mut(x, 1) = [v / 2.0, v / 2.0, v / 2.0, 0.5];
    }
    let src = Arc::new(img);
    let inst = instance();
    let close = |a: [f32; 4], b: [f32; 4]| (0..4).all(|i| (a[i] - b[i]).abs() < 1e-5);
    // At first: nothing changes.
    let out = render(&inst, &src);
    assert_eq!(out.data, src.data);
    // Red 0.1 in the middle tones: the middle grey's red up 0.1, green and
    // blue down 0.05; black and white kept; its alpha kept.
    set(&inst, "red", 0.1);
    let out = render(&inst, &src);
    assert!(
        close(out.pixel(2, 0), [0.6, 0.45, 0.45, 1.0]),
        "{:?}",
        out.pixel(2, 0)
    );
    assert!(
        close(out.pixel(2, 1), [0.3, 0.225, 0.225, 0.5]),
        "{:?}",
        out.pixel(2, 1)
    );
    assert_eq!(out.pixel(0, 0), [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(out.pixel(3, 0), [1.0, 1.0, 1.0, 1.0]);
    // In the light parts: white takes all of it (red past white: 8-bit
    // projects clip what effects leave), black none.
    set(&inst, "type", 0.0);
    let out = render(&inst, &src);
    assert!(
        close(out.pixel(3, 0), [1.1, 0.95, 0.95, 1.0]),
        "{:?}",
        out.pixel(3, 0)
    );
    assert_eq!(out.pixel(0, 0), [0.0, 0.0, 0.0, 1.0]);
    // In the dark parts, without keeping the luminosity: black's red up
    // 0.1, the others as they were.
    set(&inst, "type", 2.0);
    set(&inst, "preserve", 0.0);
    let out = render(&inst, &src);
    assert!(
        close(out.pixel(0, 0), [0.1, 0.0, 0.0, 1.0]),
        "{:?}",
        out.pixel(0, 0)
    );
    assert_eq!(out.pixel(3, 0), [1.0, 1.0, 1.0, 1.0]);
}
