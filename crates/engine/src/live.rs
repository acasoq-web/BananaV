//! Live input (VEGAS's input monitoring and input busses heard through
//! the Master): inputs of the computer played to the output as they come
//! in, each route through its effects chains (an input bus's, then a
//! track's), volume and pan, while it is active. Each input device has a
//! stream of its own; its callback works its routes out — their levels
//! always (the armed tracks' meters show their input), their sound when
//! active — into a ring each, and the output's stream mixes the rings,
//! at most a fifth of a second behind (older samples go).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use bv_mixer::dsp::Chain;
use bv_timeline::{Effect, RecordInput};

use crate::resample::LinearResampler;

/// What plays live: an input, through effects chains in turn, at a level
/// and pan, heard while `active`.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveRoute {
    /// Who it is: a track's id, an input bus's (the levels name it so).
    pub key: u64,
    /// The device (empty: the default) and its channels (no input bus).
    pub input: RecordInput,
    pub chains: Vec<Vec<Effect>>,
    /// Linear.
    pub gain: f32,
    /// −1 left to 1 right.
    pub pan: f32,
    pub active: bool,
}

/// A route as its device's callback works it.
struct RouteState {
    route: LiveRoute,
    chains: Vec<Chain>,
    resampler: Option<LinearResampler>,
    ring: VecDeque<[f32; 2]>,
    peak: [f32; 2],
}

/// What the streams share: the routes, by device.
#[derive(Default)]
struct Shared {
    routes: Vec<RouteState>,
}

pub(crate) struct Live {
    /// The devices open (their names as routes give them) and their rates.
    devices: Vec<(String, u32)>,
    _inputs: Vec<cpal::Stream>,
    _output: cpal::Stream,
    out_rate: u32,
    shared: Arc<Mutex<Shared>>,
}

/// Opens input `name` (empty: the default input).
fn input_device(name: &str) -> Option<cpal::Device> {
    let host = cpal::default_host();
    if name.is_empty() {
        return host.default_input_device();
    }
    host.input_devices()
        .ok()?
        .find(|d| d.description().is_ok_and(|x| x.name() == name))
        .or_else(|| host.default_input_device())
}

/// Equal-power pan of a mono sample: left and right gains.
fn pan_gains(pan: f32) -> [f32; 2] {
    let a = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    [
        a.cos() * std::f32::consts::SQRT_2,
        a.sin() * std::f32::consts::SQRT_2,
    ]
}

/// A device's block (`stride` channels a frame, at `rate`) through its
/// routes, into their rings at `out_rate`.
fn take_block(
    shared: &Mutex<Shared>,
    device: &str,
    block: &[f32],
    stride: usize,
    rate: u32,
    out_rate: u32,
) {
    let Ok(mut s) = shared.lock() else {
        return;
    };
    let frames = block.len() / stride.max(1);
    for r in s
        .routes
        .iter_mut()
        .filter(|r| r.route.input.device == device)
    {
        let first = usize::from(r.route.input.first).min(stride.saturating_sub(1));
        let channels = usize::from(r.route.input.channels)
            .clamp(1, 2)
            .min(stride - first);
        let mut planar: Vec<Vec<f32>> = (0..channels)
            .map(|c| (0..frames).map(|i| block[i * stride + first + c]).collect())
            .collect();
        if r.route.active {
            for chain in &mut r.chains {
                chain.process(&mut planar);
            }
        }
        // Stereo: a mono input panned, a stereo one balanced.
        let pan = pan_gains(r.route.pan);
        let g = r.route.gain;
        let (left, right) = match channels {
            1 => (&planar[0], &planar[0]),
            _ => (&planar[0], &planar[1]),
        };
        let (gl, gr) = match channels {
            1 => (pan[0], pan[1]),
            _ => (pan[0].min(1.0), pan[1].min(1.0)),
        };
        let stereo: Vec<Vec<f32>> = vec![
            left.iter().map(|v| v * gl * g).collect(),
            right.iter().map(|v| v * gr * g).collect(),
        ];
        for (c, ch) in stereo.iter().enumerate() {
            let p = ch.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            r.peak[c] = r.peak[c].max(p);
        }
        if !r.route.active {
            r.ring.clear();
            continue;
        }
        let stereo = match &mut r.resampler {
            Some(res) if rate != out_rate => res.process(&stereo),
            _ => stereo,
        };
        for (l, rr) in stereo[0].iter().zip(&stereo[1]) {
            r.ring.push_back([*l, *rr]);
        }
        let most = (out_rate / 5) as usize;
        while r.ring.len() > most {
            r.ring.pop_front();
        }
    }
}

/// The output's callback: the active routes' rings mixed.
fn mix_into<T: Copy + Default>(
    out: &mut [T],
    channels: usize,
    shared: &Mutex<Shared>,
    convert: impl Fn(f32) -> T,
) {
    out.fill(T::default());
    let Ok(mut s) = shared.lock() else {
        return;
    };
    for frame in out.chunks_mut(channels.max(1)) {
        let mut sum = [0.0f32; 2];
        for r in s.routes.iter_mut().filter(|r| r.route.active) {
            if let Some([l, rr]) = r.ring.pop_front() {
                sum[0] += l;
                sum[1] += rr;
            }
        }
        for (k, v) in frame.iter_mut().enumerate().take(2) {
            *v = convert(sum[k.min(1)]);
        }
    }
}

impl Live {
    /// Opens the devices `routes` take and the output.
    fn open(routes: &[LiveRoute]) -> Result<Live, String> {
        let host = cpal::default_host();
        let named = crate::playback::output_device().and_then(|name| {
            host.output_devices()
                .ok()?
                .find(|d| d.description().is_ok_and(|x| x.name() == name))
        });
        let out_device = named
            .or_else(|| host.default_output_device())
            .ok_or("no audio output device")?;
        let out_config = out_device
            .default_output_config()
            .map_err(|e| e.to_string())?;
        let out_rate = out_config.sample_rate();
        let out_channels = out_config.channels() as usize;
        let shared = Arc::new(Mutex::new(Shared::default()));
        let mut devices: Vec<(String, u32)> = Vec::new();
        let mut inputs = Vec::new();
        for r in routes {
            if devices.iter().any(|(d, _)| *d == r.input.device) {
                continue;
            }
            let device = input_device(&r.input.device).ok_or("no audio input device")?;
            let supported = device.default_input_config().map_err(|e| e.to_string())?;
            let config = supported.config();
            let (stride, rate) = (config.channels as usize, config.sample_rate);
            let name = r.input.device.clone();
            let mine = shared.clone();
            let err = |_: cpal::Error| {};
            let stream = match supported.sample_format() {
                cpal::SampleFormat::F32 => device.build_input_stream::<f32, _, _>(
                    config,
                    move |data: &[f32], _| take_block(&mine, &name, data, stride, rate, out_rate),
                    err,
                    None,
                ),
                cpal::SampleFormat::I16 => device.build_input_stream::<i16, _, _>(
                    config,
                    move |data: &[i16], _| {
                        let block: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                        take_block(&mine, &name, &block, stride, rate, out_rate);
                    },
                    err,
                    None,
                ),
                cpal::SampleFormat::I32 => device.build_input_stream::<i32, _, _>(
                    config,
                    move |data: &[i32], _| {
                        let block: Vec<f32> =
                            data.iter().map(|&s| s as f32 / 2_147_483_648.0).collect();
                        take_block(&mine, &name, &block, stride, rate, out_rate);
                    },
                    err,
                    None,
                ),
                other => return Err(format!("input sample format {other:?}")),
            }
            .map_err(|e| e.to_string())?;
            stream.play().map_err(|e| e.to_string())?;
            inputs.push(stream);
            devices.push((r.input.device.clone(), rate));
        }
        let mine = shared.clone();
        let err = |_: cpal::Error| {};
        let output = match out_config.sample_format() {
            cpal::SampleFormat::F32 => out_device.build_output_stream::<f32, _, _>(
                out_config.config(),
                move |out: &mut [f32], _| mix_into(out, out_channels, &mine, |s| s),
                err,
                None,
            ),
            cpal::SampleFormat::I16 => out_device.build_output_stream::<i16, _, _>(
                out_config.config(),
                move |out: &mut [i16], _| {
                    mix_into(out, out_channels, &mine, |s| {
                        (s.clamp(-1.0, 1.0) * 32767.0) as i16
                    })
                },
                err,
                None,
            ),
            other => return Err(format!("output sample format {other:?}")),
        }
        .map_err(|e| e.to_string())?;
        output.play().map_err(|e| e.to_string())?;
        Ok(Live {
            devices,
            _inputs: inputs,
            _output: output,
            out_rate,
            shared,
        })
    }

    /// Whether it has every device `routes` take.
    fn covers(&self, routes: &[LiveRoute]) -> bool {
        routes
            .iter()
            .all(|r| self.devices.iter().any(|(d, _)| *d == r.input.device))
    }

    /// The routes now: each kept (its chains and filters) while it is
    /// the same, its effects matched again when they change.
    fn set(&self, routes: Vec<LiveRoute>) {
        let Ok(mut s) = self.shared.lock() else {
            return;
        };
        let mut old: HashMap<u64, RouteState> =
            s.routes.drain(..).map(|r| (r.route.key, r)).collect();
        for route in routes {
            let rate = self
                .devices
                .iter()
                .find(|(d, _)| *d == route.input.device)
                .map_or(self.out_rate, |&(_, r)| r);
            let channels = usize::from(route.input.channels).clamp(1, 2);
            let mut state = old.remove(&route.key).unwrap_or_else(|| RouteState {
                route: route.clone(),
                chains: Vec::new(),
                resampler: None,
                ring: VecDeque::new(),
                peak: [0.0; 2],
            });
            if state.chains.len() != route.chains.len()
                || state.route.chains != route.chains
                || state.route.input != route.input
            {
                state.chains = route
                    .chains
                    .iter()
                    .map(|effects| {
                        let mut c = Chain::default();
                        let list: Vec<&Effect> = effects.iter().collect();
                        c.sync(&list, rate, channels, false);
                        c
                    })
                    .collect();
            }
            if state.resampler.is_none() && rate != self.out_rate {
                state.resampler = Some(LinearResampler::with_step(
                    f64::from(rate) / f64::from(self.out_rate),
                    2,
                ));
            }
            state.route = route;
            s.routes.push(state);
        }
    }

    /// Each route's loudest sample since the last call (left, right).
    fn take_levels(&self) -> Vec<(u64, [f32; 2])> {
        let Ok(mut s) = self.shared.lock() else {
            return Vec::new();
        };
        s.routes
            .iter_mut()
            .map(|r| (r.route.key, std::mem::take(&mut r.peak)))
            .collect()
    }
}

impl crate::Engine {
    /// What plays live from the computer's inputs now (`routes`; none
    /// closes the devices). Routes that stay the same keep their effects'
    /// state; a device not open yet opens them all again.
    pub fn set_live(&mut self, routes: Vec<LiveRoute>) {
        if routes.is_empty() {
            self.live = None;
            self.live_error = None;
            return;
        }
        if self.live.as_ref().is_none_or(|l| !l.covers(&routes)) {
            self.live = None;
            match Live::open(&routes) {
                Ok(l) => {
                    self.live = Some(l);
                    self.live_error = None;
                }
                Err(e) => {
                    self.live_error = Some(e);
                    return;
                }
            }
        }
        if let Some(l) = &self.live {
            l.set(routes);
        }
    }

    /// Each live route's loudest sample since the last call.
    pub fn live_levels(&self) -> Vec<(u64, [f32; 2])> {
        self.live.as_ref().map_or_else(Vec::new, Live::take_levels)
    }

    /// Why live input could not open, if it could not.
    pub fn live_error(&self) -> Option<&str> {
        self.live_error.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A route's block: through its chain (a gain), panned, into its ring
    /// when active; its level either way.
    #[test]
    fn routes_take_their_channels_through_their_effects() {
        let mut gain = Effect::new(bv_timeline::EffectKind::Gain);
        gain.params[0] = 6.0206;
        let route = LiveRoute {
            key: 1,
            input: RecordInput {
                device: "mic".into(),
                first: 1,
                channels: 1,
                bus: None,
            },
            chains: vec![vec![gain.clone()]],
            gain: 1.0,
            pan: 0.0,
            active: true,
        };
        let mut chain = Chain::default();
        chain.sync(&[&gain], 48_000, 1, false);
        let shared = Mutex::new(Shared {
            routes: vec![RouteState {
                route,
                chains: vec![chain],
                resampler: None,
                ring: VecDeque::new(),
                peak: [0.0; 2],
            }],
        });
        // Two channels a frame: the second one (0.25) is the route's.
        let block: Vec<f32> = (0..64)
            .map(|i| if i % 2 == 1 { 0.25 } else { 0.9 })
            .collect();
        take_block(&shared, "mic", &block, 2, 48_000, 48_000);
        take_block(&shared, "other", &block, 2, 48_000, 48_000);
        let s = shared.lock().unwrap();
        let r = &s.routes[0];
        assert_eq!(r.ring.len(), 32, "only its device's");
        let [l, rr] = r.ring[31];
        // Doubled by the effect, centered at equal power (× 1 each side).
        assert!(
            (l - 0.5).abs() < 0.01 && (rr - 0.5).abs() < 0.01,
            "{l} {rr}"
        );
        assert!((r.peak[0] - 0.5).abs() < 0.01);
        drop(s);
        let mut out = vec![0.0f32; 8];
        mix_into(&mut out, 2, &shared, |v| v);
        assert!((out[0] - 0.5).abs() < 0.01 && (out[1] - 0.5).abs() < 0.01);
    }
}
