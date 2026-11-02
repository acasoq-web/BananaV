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
pub mod transitions;
pub mod trimmer;

use std::collections::HashMap;
use std::sync::Arc;

use bv_engine::Thumbnail;

use crate::egui::{self, ColorImage, TextureHandle, TextureId, TextureOptions};

/// Texturas das miniaturas da engine, criadas uma vez por miniatura.
#[derive(Default)]
pub struct TextureCache {
    map: HashMap<usize, TextureHandle>,
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
        self.map
            .entry(key)
            .or_insert_with(|| {
                ctx.load_texture(
                    "miniatura",
                    ColorImage::from_rgba_unmultiplied(
                        [thumb.width as usize, thumb.height as usize],
                        &thumb.rgba,
                    ),
                    TextureOptions::LINEAR,
                )
            })
            .id()
    }
}

/// Texto de ajuda discreto.
pub fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).color(crate::theme::text_dim()));
}
