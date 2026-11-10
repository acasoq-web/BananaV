//! Conteúdo das abas acopláveis.

pub mod chooser;
pub mod color;
pub mod compositors;
pub mod console;
pub mod details;
pub mod effects;
pub mod explorer;
pub mod generators;
pub mod loudness;
pub mod master;
pub mod media;
pub mod media_tags;
pub mod notes;
pub mod plugins;
pub mod preview;
pub mod properties;
pub mod scopes;
pub mod storyboard;
pub mod surround;
pub mod thumb_part;
pub mod transform;
pub mod transitions;
pub mod trimmer;

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use bv_engine::Thumbnail;

use crate::egui::{self, ColorImage, TextureHandle, TextureId, TextureOptions};

/// The engine's thumbnails as textures, made once per thumbnail.
///
/// Each texture is found by its thumbnail's address, and keeps a weak
/// reference to it: the address then stays the thumbnail's own, never
/// given to another one (another media's picture) while its texture is
/// here. Textures whose thumbnails are gone are let go.
#[derive(Default)]
pub struct TextureCache {
    map: HashMap<usize, (Weak<Thumbnail>, TextureHandle)>,
    /// Textures made since the last look for those of gone thumbnails.
    made: usize,
}

impl std::fmt::Debug for TextureCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextureCache")
            .field("n", &self.map.len())
            .finish()
    }
}

impl TextureCache {
    pub fn get(&mut self, ctx: &egui::Context, thumb: &Arc<Thumbnail>) -> TextureId {
        let key = Arc::as_ptr(thumb) as usize;
        if let Some((_, tex)) = self.map.get(&key) {
            return tex.id();
        }
        self.made += 1;
        if self.made >= 64 {
            self.made = 0;
            self.map.retain(|_, (weak, _)| weak.strong_count() > 0);
        }
        let tex = ctx.load_texture(
            "miniatura",
            ColorImage::from_rgba_unmultiplied(
                [thumb.width as usize, thumb.height as usize],
                &thumb.rgba,
            ),
            TextureOptions::LINEAR,
        );
        let id = tex.id();
        self.map.insert(key, (Arc::downgrade(thumb), tex));
        id
    }

    /// How many textures it keeps (tests).
    #[cfg(test)]
    fn len(&self) -> usize {
        self.map.len()
    }
}

/// Texto de ajuda discreto.
pub fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).color(crate::theme::text_dim()));
}

#[cfg(test)]
mod texture_tests {
    use super::*;

    fn thumb(shade: u8) -> Arc<Thumbnail> {
        Arc::new(Thumbnail {
            width: 2,
            height: 2,
            rgba: vec![shade; 16],
        })
    }

    #[test]
    fn a_new_thumbnail_never_gets_a_gone_ones_texture() {
        // Thumbnails made again (files changed, proxies) are new ones the
        // allocator may put where gone ones were: each still gets its own
        // picture, not the texture of another media.
        let ctx = egui::Context::default();
        let mut cache = TextureCache::default();
        for k in 0..32u8 {
            let a = thumb(k);
            let ta = cache.get(&ctx, &a);
            drop(a);
            let b = thumb(255 - k);
            assert_ne!(cache.get(&ctx, &b), ta, "round {k}");
        }
        // Those of gone thumbnails are let go.
        let kept: Vec<Arc<Thumbnail>> = (0..8).map(thumb).collect();
        for t in &kept {
            cache.get(&ctx, t);
        }
        for k in 0..64 {
            cache.get(&ctx, &thumb(k));
        }
        assert!(cache.len() <= 64 + kept.len(), "{}", cache.len());
        for t in &kept {
            let again = cache.get(&ctx, t);
            assert_eq!(cache.get(&ctx, t), again);
        }
    }
}
