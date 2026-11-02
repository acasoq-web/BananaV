//! BananaV's own configuration read and every conversion it offers worked
//! out; ACES's outputs against values worked out by hand from the
//! Academy's reference (the CTL).

use bv_ocio::{Reference, builtin};

fn near(a: [f64; 3], b: [f64; 3], by: f64) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= by)
}

#[test]
fn bananavs_configuration_reads_and_every_space_converts() {
    let c = builtin();
    assert_eq!(c.version, 2.1);
    let displays: Vec<&str> = c.shown_displays().iter().map(|d| d.name.as_str()).collect();
    assert_eq!(
        displays,
        [
            "sRGB",
            "Rec.709",
            "Rec.2020",
            "Display P3",
            "P3-D65",
            "P3-DCI"
        ]
    );
    for d in c.shown_displays() {
        let views: Vec<&str> = c.shown_views(d).iter().map(|v| v.name.as_str()).collect();
        assert_eq!(views, ["ACES", "Un-tone-mapped", "Raw"], "{}", d.name);
        for v in c.shown_views(d) {
            for src in ["ACES2065-1", "ACEScg", "ACEScct", "sRGB - Texture"] {
                c.display_processor(src, &d.name, &v.name, None)
                    .unwrap_or_else(|e| panic!("{src} → {} / {}: {e}", d.name, v.name));
            }
        }
    }
    for cs in &c.colorspaces {
        let to = c
            .processor(&cs.name, "ACES2065-1")
            .unwrap_or_else(|e| panic!("{}: {e}", cs.name));
        let back = c
            .processor("ACES2065-1", &cs.name)
            .unwrap_or_else(|e| panic!("{}: {e}", cs.name));
        if cs.reference != Reference::Scene || cs.is_data {
            continue;
        }
        for v in [0.02, 0.18, 0.6] {
            let x = [v, v * 0.8, v * 1.1];
            let y = to.apply(back.apply(x));
            assert!(
                near(y, x, 1e-6 * v.max(0.01) * 10.0),
                "{}: {x:?} → {y:?}",
                cs.name
            );
        }
    }
    // Roles, aliases (the studio configuration's names and ACES 1.x's).
    let name = |n: &str| c.colorspace(n).map(|s| s.name.as_str());
    assert_eq!(name("scene_linear"), Some("ACEScg"));
    assert_eq!(name("color_timing"), Some("ACEScct"));
    assert_eq!(name("lin_srgb"), Some("Linear Rec.709 (sRGB)"));
    assert_eq!(name("ACES - ACEScg"), Some("ACEScg"));
    assert_eq!(name("Utility - sRGB - Texture"), Some("sRGB - Texture"));
    assert_eq!(name("srgb - display"), Some("sRGB"));
    assert_eq!(name("Raw"), Some("Raw"));
    assert!(c.colorspace("nothing like it").is_none());
}

#[test]
fn spaces_meet_where_their_specifications_say() {
    let c = builtin();
    let conv = |from: &str, to: &str, x: [f64; 3]| c.processor(from, to).unwrap().apply(x);
    // ACEScct's and ACEScc's 0.4135884 is 18% grey.
    let g = conv("ACEScct", "ACEScg", [0.413_588_4; 3]);
    assert!(near(g, [0.18; 3], 1e-6), "{g:?}");
    let g = conv("ACEScc", "ACES2065-1", [0.413_588_4; 3]);
    assert!(near(g, [0.18; 3], 1e-6), "{g:?}");
    // White stays white from every gamut.
    for space in [
        "Linear Rec.709 (sRGB)",
        "Linear Rec.2020",
        "Linear P3-D65",
        "sRGB - Texture",
    ] {
        let w = conv(space, "ACES2065-1", [1.0; 3]);
        assert!(near(w, [1.0; 3], 1e-6), "{space}: {w:?}");
    }
    // The cameras' 18% greys: S-Log3's 420 of 1023, V-Log's 0.423, LogC3's
    // 0.391, Log3G10's 0.333, LogC4's 0.278, DaVinci Intermediate's 0.336.
    for (space, code) in [
        ("S-Log3 S-Gamut3.Cine", 420.0 / 1023.0),
        ("S-Log3 S-Gamut3", 420.0 / 1023.0),
        ("V-Log V-Gamut", 0.423_311),
        ("ARRI LogC3 (EI800)", 0.391_007),
        ("Log3G10 REDWideGamutRGB", 0.333_333),
        ("ARRI LogC4", 0.278_395_8),
        ("DaVinci Intermediate WideGamut", 0.336_043),
    ] {
        let g = conv(space, "ACES2065-1", [code; 3]);
        assert!(near(g, [0.18; 3], 2e-4), "{space}: {g:?}");
    }
    // Camera Rec.709: 0.409 (BT.709's curve at 18%).
    let g = conv("Camera Rec.709", "Linear Rec.709 (sRGB)", [0.409_007_7; 3]);
    assert!(near(g, [0.18; 3], 1e-6), "{g:?}");
}

#[test]
fn acess_outputs_put_grey_where_the_reference_does() {
    let c = builtin();
    let grey = |display: &str, view: &str| {
        c.display_processor("ACES2065-1", display, view, None)
            .unwrap()
            .apply([0.18; 3])
    };
    // 0.18 is the tone scale's 4.8 cd/m² of 48 (0.02 its black): 0.099625
    // of the range; video's dim surround (Y^0.9811) makes it 0.104064.
    assert!(
        near(grey("sRGB", "ACES"), [0.355_954; 3], 2e-5),
        "{:?}",
        grey("sRGB", "ACES")
    );
    assert!(near(grey("Rec.709", "ACES"), [0.389_530; 3], 2e-5));
    assert!(near(grey("Rec.2020", "ACES"), [0.389_530; 3], 2e-5));
    // Cinema's dark surround: none of that, and gamma 2.6.
    assert!(near(grey("P3-D65", "ACES"), [0.411_867; 3], 2e-5));
    // Un-tone-mapped: the light through the display's curve; Raw: nothing.
    assert!(near(grey("sRGB", "Un-tone-mapped"), [0.461_356; 3], 1e-6));
    assert!(near(grey("sRGB", "Raw"), [0.18; 3], 1e-12));
    // White (16.3, the tone scale's top) reaches the display's white, the
    // darkest black its black.
    let p = c
        .display_processor("ACES2065-1", "sRGB", "ACES", None)
        .unwrap();
    let w = p.apply([20.0; 3]);
    assert!(near(w, [1.0; 3], 2e-3), "{w:?}");
    let k = p.apply([0.0; 3]);
    assert!(near(k, [0.0; 3], 1e-3), "{k:?}");
    // A looks list: the Blue Light Artifact Fix leaves greys alone.
    let fix = c
        .display_processor(
            "ACES2065-1",
            "sRGB",
            "ACES",
            Some("Blue Light Artifact Fix"),
        )
        .unwrap()
        .apply([0.18; 3]);
    assert!(near(fix, [0.355_954; 3], 2e-5), "{fix:?}");
}
