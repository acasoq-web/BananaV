//! Pictures BananaV hands a plug-in when one of its push buttons asks for
//! one (BananaV's `kBananaVParamPropPicture`, see `bv_ofx_sys::extensions`):
//! the Video Preview's frame, the effect's own input at the cursor, the
//! Trimmer's frame or a picture file the user picks — as effects get their
//! pictures (`bv_engine::frame_picture`, `file_picture`). The clipboard's
//! and a part of the screen wait, their buttons greyed.

use std::sync::Arc;

use bv_core::ImageRgbaF32;
use bv_engine::PreviewFrame;

use crate::i18n::tr;

/// A picture a button asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Preview,
    Input,
    Trimmer,
    File,
    Clipboard,
    Screen,
}

impl Kind {
    pub(crate) fn of(name: &str) -> Option<Kind> {
        Some(match name {
            "Preview" => Kind::Preview,
            "Input" => Kind::Input,
            "Trimmer" => Kind::Trimmer,
            "File" => Kind::File,
            "Clipboard" => Kind::Clipboard,
            "Screen" => Kind::Screen,
            _ => return None,
        })
    }

    /// Whether BananaV gives it yet (else its button waits greyed).
    pub(crate) fn ready(self) -> bool {
        !matches!(self, Kind::Clipboard | Kind::Screen)
    }

    /// What to tell the user when there is no such picture now.
    pub(crate) fn missing(self) -> &'static str {
        match self {
            Kind::Preview => tr("The Video Preview shows no picture yet."),
            Kind::Input => tr(
                "Put the cursor over the effect's event first: its picture comes from the Video Preview.",
            ),
            Kind::Trimmer => tr("The Trimmer shows no picture."),
            Kind::File => tr("The picture could not be read."),
            Kind::Clipboard | Kind::Screen => tr("Coming soon to BananaV"),
        }
    }
}

/// What the app tells the plug-ins' editors each frame, for the pictures.
#[derive(Clone, Default)]
pub(crate) struct Sources {
    pub preview: Option<Arc<PreviewFrame>>,
    pub trimmer: Option<Arc<PreviewFrame>>,
    /// Effects get video's values (the project composites on them), not
    /// light.
    pub video_gamma: bool,
}

impl std::fmt::Debug for Sources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sources")
            .field("preview", &self.preview.is_some())
            .field("trimmer", &self.trimmer.is_some())
            .field("video_gamma", &self.video_gamma)
            .finish()
    }
}

impl Sources {
    /// Picture `kind` (`input`: the effect's, when the preview gave it
    /// one): `None` when there is none now, or the user picked no file
    /// (`Err` with what to say when one cannot be had).
    pub(crate) fn picture(
        &self,
        kind: Kind,
        input: Option<Arc<ImageRgbaF32>>,
    ) -> Result<Option<Arc<ImageRgbaF32>>, &'static str> {
        let frame = |f: &Option<Arc<PreviewFrame>>| {
            f.as_ref()
                .map(|f| Arc::new(bv_engine::frame_picture(f, self.video_gamma)))
                .ok_or(kind.missing())
        };
        match kind {
            Kind::Preview => frame(&self.preview).map(Some),
            Kind::Trimmer => frame(&self.trimmer).map(Some),
            Kind::Input => input.map(Some).ok_or(kind.missing()),
            Kind::File => {
                let picked = rfd::FileDialog::new()
                    .add_filter(tr("Pictures"), bv_engine::IMAGE_EXTENSIONS)
                    .pick_file();
                let Some(path) = picked else {
                    return Ok(None);
                };
                bv_engine::file_picture(&path.display().to_string(), self.video_gamma)
                    .map(|img| Some(Arc::new(img)))
                    .map_err(|_| kind.missing())
            }
            Kind::Clipboard | Kind::Screen => Err(kind.missing()),
        }
    }
}
