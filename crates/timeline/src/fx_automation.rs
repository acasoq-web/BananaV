//! Audio effects' automation (VEGAS's FX Automation): envelopes of the
//! effects' parameters and of their bypass among a track's — or a bus's
//! or an assignable chain's — envelopes. Each names its effect by the
//! number the effect has in its chain ([`Effect::id`], given when it is
//! first automated) and the parameter by its index, and holds 0–1 of the
//! parameter's range ([`crate::fx::ParamSpec::to_unit`]; a VST 3 effect's
//! parameter by its id, its normalized value as it is), so that its lane
//! draws any parameter alike. Bypass FX Automation
//! ([`Effect::automation_bypassed`]) leaves an effect's envelopes out: its
//! own values hold.

use std::borrow::Cow;

use bv_core::Tick;

use crate::envelope::{Envelope, EnvelopeKind, EnvelopeOwner};
use crate::fx::{Effect, KeyCurve};
use crate::model::Project;

/// What of an effect is automated: its bypass (`None`) or a parameter.
pub type Automated = Option<u32>;

/// Whether the FX Automation Chooser offers `e`: BananaV's own audio
/// effects and VST 3 effects.
pub fn automatable(e: &Effect) -> bool {
    e.kind.is_audio() && e.plugin.is_none() && (e.vst.is_some() || !e.kind.params().is_empty())
}

/// The envelope kind of `what` of the effect numbered `fx`.
pub fn kind_of(fx: u32, what: Automated) -> EnvelopeKind {
    match what {
        None => EnvelopeKind::FxBypass(fx),
        Some(param) => EnvelopeKind::FxParam { fx, param },
    }
}

/// The effect an FX automation envelope of `kind` automates in `effects`,
/// and what of it.
pub fn target(effects: &[Effect], kind: EnvelopeKind) -> Option<(usize, Automated)> {
    let (fx, what) = match kind {
        EnvelopeKind::FxParam { fx, param } => (fx, Some(param)),
        EnvelopeKind::FxBypass(fx) => (fx, None),
        _ => return None,
    };
    let i = effects.iter().position(|e| e.id != 0 && e.id == fx)?;
    Some((i, what))
}

/// `effects` as their automation has them at `t` (borrowed when nothing
/// is automated).
pub fn automated<'a>(effects: &'a [Effect], envelopes: &[Envelope], t: Tick) -> Cow<'a, [Effect]> {
    if !envelopes.iter().any(|e| e.kind.is_fx()) {
        return Cow::Borrowed(effects);
    }
    let mut out = effects.to_vec();
    for e in out
        .iter_mut()
        .filter(|e| e.id != 0 && !e.automation_bypassed)
    {
        for env in envelopes {
            match env.kind {
                EnvelopeKind::FxParam { fx, param } if fx == e.id && e.vst.is_some() => {
                    if let Some(v) = e.vst.as_deref_mut() {
                        v.params.insert(param, f64::from(env.value_at(t)));
                    }
                }
                EnvelopeKind::FxParam { fx, param } if fx == e.id => {
                    if let Some(spec) = e.kind.params().get(param as usize) {
                        let v = spec.from_unit(env.value_at(t));
                        e.set_param(param as usize, v);
                    }
                }
                EnvelopeKind::FxBypass(fx) if fx == e.id => e.enabled = env.value_at(t) >= 0.5,
                _ => {}
            }
        }
    }
    Cow::Owned(out)
}

impl Project {
    /// The effects and the envelopes FX automation of `owner` works with:
    /// a track's, a bus's or a chain's.
    pub fn fx_chain_and_envelopes(&self, owner: EnvelopeOwner) -> Option<(&[Effect], &[Envelope])> {
        match owner {
            EnvelopeOwner::Track(t) => self.track(t).map(|t| (&t.effects[..], &t.envelopes[..])),
            EnvelopeOwner::Strip(s) => self
                .strip_channel(s)
                .map(|c| (&c.effects[..], &c.envelopes[..])),
            _ => None,
        }
    }

    /// [`Self::fx_chain_and_envelopes`], to change them.
    pub fn fx_chain_and_envelopes_mut(
        &mut self,
        owner: EnvelopeOwner,
    ) -> Option<(&mut Vec<Effect>, &mut Vec<Envelope>)> {
        match owner {
            EnvelopeOwner::Track(t) => self
                .track_by_id_mut(t)
                .map(|t| (&mut t.effects, &mut t.envelopes)),
            EnvelopeOwner::Strip(s) => self
                .strip_channel_mut(s)
                .map(|c| (&mut c.effects, &mut c.envelopes)),
            _ => None,
        }
    }

    /// What of effect `i` of `owner` is automated.
    pub fn fx_automation(&self, owner: EnvelopeOwner, i: usize) -> Vec<Automated> {
        let Some((effects, envelopes)) = self.fx_chain_and_envelopes(owner) else {
            return Vec::new();
        };
        let Some(e) = effects.get(i).filter(|e| e.id != 0) else {
            return Vec::new();
        };
        envelopes
            .iter()
            .filter_map(|env| match env.kind {
                EnvelopeKind::FxBypass(fx) if fx == e.id => Some(None),
                EnvelopeKind::FxParam { fx, param } if fx == e.id => Some(Some(param)),
                _ => None,
            })
            .collect()
    }

    /// Automates exactly `wanted` of effect `i` of `owner`: envelopes added
    /// — a point holding its value now, with `curve` — or taken away, the
    /// effect numbered first. Whether anything changed.
    pub fn set_fx_automation(
        &mut self,
        owner: EnvelopeOwner,
        i: usize,
        wanted: &[Automated],
        curve: KeyCurve,
    ) -> bool {
        let Some((effects, envelopes)) = self.fx_chain_and_envelopes_mut(owner) else {
            return false;
        };
        if i >= effects.len() {
            return false;
        }
        let mut id = effects[i].id;
        if id == 0 {
            if wanted.is_empty() {
                return false;
            }
            id = effects.iter().map(|e| e.id).max().unwrap_or(0) + 1;
            effects[i].id = id;
        }
        let e = &effects[i];
        let before = envelopes.len();
        envelopes.retain(|env| match env.kind {
            EnvelopeKind::FxBypass(fx) if fx == id => wanted.contains(&None),
            EnvelopeKind::FxParam { fx, param } if fx == id => wanted.contains(&Some(param)),
            _ => true,
        });
        let mut changed = envelopes.len() != before;
        for w in wanted {
            let kind = kind_of(id, *w);
            if envelopes.iter().any(|env| env.kind == kind) {
                continue;
            }
            let value = match w {
                None => {
                    if e.enabled {
                        1.0
                    } else {
                        0.0
                    }
                }
                // A VST 3 effect's: its value now (normalized).
                Some(p) if e.vst.is_some() => e
                    .vst
                    .as_deref()
                    .and_then(|v| v.params.get(p))
                    .map_or(0.5, |&v| v as f32),
                Some(p) => e
                    .kind
                    .params()
                    .get(*p as usize)
                    .map_or(0.0, |s| s.to_unit(e.param(*p as usize))),
            };
            let mut env = Envelope::with_curve(kind, curve);
            env.points[0].value = value;
            envelopes.push(env);
            changed = true;
        }
        changed
    }

    /// Envelopes of effects no longer in their chains go (an effect taken
    /// out takes its automation with it).
    pub fn prune_fx_automation(&mut self) {
        let prune = |effects: &[Effect], envelopes: &mut Vec<Envelope>| {
            envelopes.retain(|env| match env.kind {
                EnvelopeKind::FxParam { fx, .. } | EnvelopeKind::FxBypass(fx) => {
                    effects.iter().any(|e| e.id == fx)
                }
                _ => true,
            });
        };
        for t in &mut self.tracks {
            prune(&t.effects, &mut t.envelopes);
        }
        for b in &mut self.busses {
            prune(&b.channel.effects, &mut b.channel.envelopes);
        }
        for c in &mut self.fx_chains {
            prune(&c.channel.effects, &mut c.channel.envelopes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::EffectKind;
    use crate::model::TrackKind;

    #[test]
    fn a_parameter_follows_its_envelope() {
        let mut p = Project::default();
        let t = p
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Audio)
            .unwrap()
            .id;
        p.track_by_id_mut(t)
            .unwrap()
            .effects
            .push(Effect::new(EffectKind::Gain));
        let owner = EnvelopeOwner::Track(t);
        // The gain (−24 to 24 dB) and the bypass, automated.
        assert!(p.set_fx_automation(owner, 0, &[Some(0), None], KeyCurve::Linear));
        let track = p.track(t).unwrap();
        assert_eq!(track.effects[0].id, 1);
        assert_eq!(p.fx_automation(owner, 0).len(), 2);
        // The gain's envelope from 0 dB (0.5) up to 24 dB (1) over a second.
        let track = p.track_by_id_mut(t).unwrap();
        let env = track
            .envelopes
            .iter_mut()
            .find(|e| e.kind == EnvelopeKind::FxParam { fx: 1, param: 0 })
            .unwrap();
        assert!((env.points[0].value - 0.5).abs() < 1e-6, "its value now");
        env.points.push(crate::envelope::EnvelopePoint {
            at: Tick::from_seconds(1),
            value: 1.0,
            curve: KeyCurve::Linear,
        });
        let track = p.track(t).unwrap();
        let at = |s: f64| {
            automated(&track.effects, &track.envelopes, Tick::from_seconds_f64(s)).into_owned()
        };
        assert!((at(0.5)[0].param(0) - 12.0).abs() < 1e-3);
        assert!((at(2.0)[0].param(0) - 24.0).abs() < 1e-3);
        // Bypass FX Automation: its own value holds.
        let mut fx = track.effects.clone();
        fx[0].automation_bypassed = true;
        assert_eq!(
            automated(&fx, &track.envelopes, Tick::from_seconds(2))[0].param(0),
            0.0
        );
        // Nothing automated any more: the envelopes go.
        assert!(p.set_fx_automation(owner, 0, &[], KeyCurve::Linear));
        assert!(p.track(t).unwrap().envelopes.is_empty());
        // An effect taken out takes its automation with it.
        p.set_fx_automation(owner, 0, &[Some(0)], KeyCurve::Linear);
        p.track_by_id_mut(t).unwrap().effects.clear();
        p.prune_fx_automation();
        assert!(p.track(t).unwrap().envelopes.is_empty());
    }

    /// A VST 3 effect's parameter automated: its envelope holds its value
    /// now, and the chain plays the envelope's.
    #[test]
    fn vst_parameters_are_automated_by_id() {
        let mut p = Project::new(crate::model::ProjectSettings::default());
        let t = p
            .tracks
            .iter()
            .find(|t| t.kind == crate::model::TrackKind::Audio)
            .unwrap()
            .id;
        let fx = crate::fx::VstFx {
            path: "C:/VST3/Echo.vst3".into(),
            params: [(42, 0.75)].into_iter().collect(),
            ..Default::default()
        };
        p.track_by_id_mut(t).unwrap().effects.push(Effect::vst(fx));
        let owner = EnvelopeOwner::Track(t);
        let e = &p.track(t).unwrap().effects[0];
        assert!(automatable(e));
        assert!(p.set_fx_automation(owner, 0, &[Some(42)], KeyCurve::Linear));
        let track = p.track(t).unwrap();
        let env = &track.envelopes[0];
        assert_eq!(env.kind, kind_of(track.effects[0].id, Some(42)));
        assert!((env.value_at(Tick::ZERO) - 0.75).abs() < 1e-6);
        let mut envs = track.envelopes.clone();
        envs[0].points[0].value = 0.25;
        let now = automated(&track.effects, &envs, Tick::ZERO);
        assert_eq!(now[0].vst.as_deref().unwrap().params[&42], 0.25);
    }
}
