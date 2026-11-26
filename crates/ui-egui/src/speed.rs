//! The preview's speed: how fast Play plays, picked on the Video Preview's
//! bar or stepped through with K (slower) and L (faster), also while it
//! plays.

use crate::app::BananaApp;
use crate::egui;
use crate::i18n::tr;

/// The speeds there are to pick.
pub(crate) const SPEEDS: [f64; 7] = [0.25, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0];

/// The next one up from `rate` (which may be between two).
pub(crate) fn faster(rate: f64) -> f64 {
    SPEEDS
        .into_iter()
        .find(|s| *s > rate + 1e-6)
        .unwrap_or(SPEEDS[SPEEDS.len() - 1])
}

/// The next one down from `rate`.
pub(crate) fn slower(rate: f64) -> f64 {
    SPEEDS
        .into_iter()
        .rev()
        .find(|s| *s < rate - 1e-6)
        .unwrap_or(SPEEDS[0])
}

/// "1.5x".
pub(crate) fn label(rate: f64) -> String {
    let digits = [0, 1, 2]
        .into_iter()
        .find(|d| {
            let k = 10f64.powi(*d as i32);
            ((rate * k).round() / k - rate).abs() < 1e-6
        })
        .unwrap_or(2);
    format!("{}x", crate::fmt::num(rate, digits))
}

/// The speed on the Video Preview's bar, beside the quality.
pub(crate) fn menu(ui: &mut egui::Ui, app: &mut BananaApp) {
    let button = crate::tabs::preview::text_arrow(
        ui,
        &label(app.play_rate),
        tr("Preview Speed (K: slower, L: faster)"),
    );
    egui::Popup::menu(&button).show(|ui| {
        for s in SPEEDS {
            let on = (app.play_rate - s).abs() < 1e-6;
            if crate::menus::check_item(ui, &label(s), on, true, "") {
                app.set_play_rate(s);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speeds_step_up_and_down_and_stop_at_their_ends() {
        let mut up = vec![0.1];
        while faster(up[up.len() - 1]) > up[up.len() - 1] {
            up.push(faster(up[up.len() - 1]));
        }
        assert_eq!(up[1..], SPEEDS);
        assert_eq!(faster(4.0), 4.0);
        assert_eq!(slower(0.25), 0.25);
        assert_eq!((slower(1.0), faster(1.0)), (0.5, 1.5));
        // Between two, from the slider.
        assert_eq!((slower(1.2), faster(1.2)), (1.0, 1.5));
        assert_eq!(label(1.0), "1x");
        assert_eq!(label(0.25), "0.25x");
        assert_eq!(label(1.5), "1.5x");
    }
}
