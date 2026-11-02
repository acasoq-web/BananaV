//! Slow Motion's Adjust Event Length on the interface's side (BananaV's
//! `kBananaVParamPropEventLength`, `ofx::LengthAsk`): the effect's event
//! made as long as the rest of its media needs at the effect's playback
//! rate, so no frame is cut off (one undo step).

use bv_core::Tick;
use bv_timeline::scene::FxOwner;

use crate::app::BananaApp;
use crate::i18n::tr;

impl BananaApp {
    /// Each frame: an event length a button asked for, set.
    pub(crate) fn follow_event_length(&mut self) {
        let Some(ask) = self.ofx.take_length() else {
            return;
        };
        let FxOwner::Event(id) = ask.key.owner else {
            self.notify(tr("Adjust Event Length works on an event's effect."));
            return;
        };
        let p = self.project();
        let Some(c) = p.clip(id) else {
            return;
        };
        let Some(media) = p.media(c.media).filter(|m| m.duration > Tick::ZERO) else {
            return;
        };
        // The rest of the media from where the event starts in it, at the
        // event's own rate, played at the effect's.
        let rest = (media.duration - c.source_in).max(Tick::ZERO);
        let at_rate = rest.as_seconds_f64() / c.rate.max(1e-3) / ask.rate.max(0.01);
        let length = Tick::from_seconds_f64(at_rate).max(self.frame_rate().frame_duration());
        let end = c.start + length;
        let _ = self.edit(|p| p.trim_end(id, end));
    }
}
