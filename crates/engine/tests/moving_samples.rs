//! The Video FX window's previews move under the pointer when their effect
//! moves by itself: the sample picture drawn later is another one for those
//! effects, the same for the others.

use std::collections::BTreeMap;

use bv_timeline::{Effect, PluginFx};

fn effect(id: &str) -> Effect {
    Effect::plugin(PluginFx {
        id: id.to_string(),
        version: [1, 0],
        name: String::new(),
        params: BTreeMap::new(),
    })
}

#[test]
fn moving_effects_show_another_picture_later() {
    let (w, h) = (96, 54);
    let moves = |id: &str| {
        let fx = [effect(id)];
        let first = bv_engine::effect_sample_at(&fx, w, h, 0.0);
        let later = bv_engine::effect_sample_at(&fx, w, h, 8.0 / 15.0);
        assert_eq!(first.len(), (w * h * 4) as usize, "{id}");
        assert_eq!(first, bv_engine::effect_sample(&fx, w, h), "{id}");
        first != later
    };
    for id in ["org.bananav.FilmGrain", "org.bananav.bananasplit.Lightning"] {
        assert!(moves(id), "{id} moves by itself");
    }
    for id in ["org.bananav.Sepia", "org.bananav.GaussianBlur"] {
        assert!(!moves(id), "{id} is still");
    }
}
