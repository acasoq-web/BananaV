//! The effects' tools: what effects, transitions and generators there are;
//! a chain's effects (an event's, a track's, a media's, the video
//! output's, the Master bus's, a transition's); adding, removing and
//! reading them, setting their parameters (keyframes with `at`);
//! transitions; generated media (titles, solid colors, patterns…).

use bv_core::Tick;
use bv_engine::plugins::{TRANSITION_PARAM, filter_context, plugin_value};
use bv_ofx::{Context, ParamInfo, ParamType, PluginInfo, Value as OfxValue};
use bv_timeline::{
    ClipId, Effect, EffectKind, Generator, KeyCurve, MediaSource, PluginFx, PluginKey, PluginParam,
    Project,
};
use serde_json::{Map, Value, json};

use super::timeline::{KEYFRAME_CURVES, key_curve, key_curve_name};
use super::{Args, Kind, Tool, arg, choice, clip_of, event_json, media_of, secs, track_at};
use crate::agents::{Outcome, Output};
use crate::app::BananaApp;
use crate::fx_window::FxTarget;
use crate::i18n::tr;
use crate::ofx::FxPlace;

fn done(v: Value) -> Outcome {
    Ok(Output::value(v))
}

/// An effect's name for the agent (the plug-in's, in English).
pub(super) fn effect_name(e: &Effect) -> String {
    e.name().to_string()
}

/// The id an agent names an effect by: the plug-in's identifier, `audio:`
/// or `video:` and one of BananaV's own built-in kinds, or `vst:` and a VST
/// plug-in's key.
fn effect_id(e: &Effect) -> String {
    match (&e.plugin, &e.vst) {
        (Some(p), _) => p.id.clone(),
        (_, Some(v)) => format!("vst:{}|{}", v.path.display(), v.class_id),
        _ if e.kind.is_audio() => format!("audio:{:?}", e.kind),
        _ => format!("video:{:?}", e.kind),
    }
}

fn kind_of_effect(e: &Effect) -> &'static str {
    if e.plugin.is_some() {
        "ofx"
    } else if e.vst.is_some() {
        "vst"
    } else {
        "builtin"
    }
}

/// The chain a tool's arguments name, and whether it is a sound's.
fn target(app: &BananaApp, a: &Args<'_>) -> Result<FxTarget, String> {
    let p = app.project();
    let output = a.bool_or("output", false)?;
    let master = a.bool_or("master", false)?;
    let given = [
        a.has("event"),
        a.has("track"),
        a.has("media"),
        output,
        master,
    ]
    .iter()
    .filter(|g| **g)
    .count();
    if given != 1 {
        return Err(tr("Give one of \"event\", \"track\", \"media\", \"output\" (the video output) or \"master\" (the Master bus).").into());
    }
    if let Some(e) = a.opt_u64("event")? {
        let id = clip_of(p, e)?.id;
        return Ok(match a.opt_str("transition")? {
            Some("in") => FxTarget::Transition(id, false),
            Some("out") => FxTarget::Transition(id, true),
            Some(other) => {
                return Err(trf!(
                    "\"transition\" is \"in\" or \"out\", not \"{}\".",
                    other
                ));
            }
            None => FxTarget::Clip(id),
        });
    }
    if let Some(t) = a.opt_u64("track")? {
        return Ok(FxTarget::Track(track_at(p, t)?.id));
    }
    if let Some(m) = a.opt_u64("media")? {
        let m = media_of(p, m)?;
        let audio = a.opt_bool("audio")?.unwrap_or(m.video.is_none());
        return Ok(FxTarget::Media(m.id, audio));
    }
    Ok(if output {
        FxTarget::VideoOutput
    } else {
        FxTarget::Master
    })
}

/// The arguments naming a chain, for the tools' schemas.
fn target_args(index: bool) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("event".into(), arg("integer", "An event's chain (its id)."));
    m.insert(
        "transition".into(),
        choice(
            &["in", "out"],
            "With `event`: its entrance (in) or exit (out) transition instead (index 0).",
        ),
    );
    m.insert(
        "track".into(),
        arg("integer", "A track's chain (its index)."),
    );
    m.insert("media".into(), arg("integer", "A media's chain (its id)."));
    m.insert(
        "audio".into(),
        arg(
            "boolean",
            "With `media`: its sound's chain, not its picture's.",
        ),
    );
    m.insert("output".into(), arg("boolean", "The video output's chain."));
    m.insert("master".into(), arg("boolean", "The Master bus's chain."));
    if index {
        m.insert(
            "index".into(),
            arg("integer", "The effect's place in its chain (0 first)."),
        );
    }
    m
}

fn chain_of(app: &BananaApp, t: FxTarget) -> Result<Vec<Effect>, String> {
    t.chain(app.project())
        .ok_or_else(|| tr("That has no effects chain.").to_string())
}

/// The OFX context a plug-in effect works in, in this chain.
fn context_in(t: FxTarget, info: &PluginInfo) -> Option<Context> {
    match t {
        FxTarget::Transition(..) => Some(Context::Transition),
        _ => filter_context(info),
    }
}

/// A plug-in's parameters an agent can set (none of its groups, pages,
/// buttons, hidden ones or the transition's progress).
fn params_of(info: &PluginInfo, ctx: Context) -> Vec<ParamInfo> {
    info.context(ctx)
        .map(|c| c.params.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|p| {
            !p.secret
                && p.name != TRANSITION_PARAM
                && !matches!(
                    p.ty,
                    ParamType::Group | ParamType::Page | ParamType::PushButton
                )
        })
        .collect()
}

/// Letters and digits only, lowercase: names met however they are written.
fn plain(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn type_name(t: ParamType) -> &'static str {
    match t {
        ParamType::Integer => "integer",
        ParamType::Double => "number",
        ParamType::Boolean => "boolean",
        ParamType::Choice | ParamType::StrChoice => "choice",
        ParamType::Rgba => "rgba",
        ParamType::Rgb => "rgb",
        ParamType::Double2D | ParamType::Integer2D => "point2d",
        ParamType::Double3D | ParamType::Integer3D => "point3d",
        ParamType::String => "text",
        ParamType::Parametric => "curves",
        _ => "data",
    }
}

/// A plug-in's value for the agent.
fn from_ofx(info: &ParamInfo, v: &OfxValue) -> Value {
    match (info.ty, v) {
        (ParamType::Boolean, OfxValue::Num(n)) => json!(n.first().copied().unwrap_or(0.0) > 0.5),
        (ParamType::Choice, OfxValue::Num(n)) => {
            let i = n.first().copied().unwrap_or(0.0).round().max(0.0) as usize;
            info.options.get(i).map_or_else(|| json!(i), |o| json!(o))
        }
        (ParamType::StrChoice, OfxValue::Text(t)) => {
            let i = info.option_values.iter().position(|o| o == t);
            i.and_then(|i| info.options.get(i))
                .map_or_else(|| json!(t), |o| json!(o))
        }
        (ParamType::Integer, OfxValue::Num(n)) => {
            json!(n.first().copied().unwrap_or(0.0).round() as i64)
        }
        (_, OfxValue::Num(n)) if n.len() == 1 => json!(n[0]),
        (_, OfxValue::Num(n)) => json!(n),
        (_, OfxValue::Text(t)) => json!(t),
        _ => Value::Null,
    }
}

/// A color from "#rrggbb" or "#rrggbbaa".
fn hex_color(s: &str) -> Option<Vec<f64>> {
    let h = s.strip_prefix('#')?;
    if !(h.len() == 6 || h.len() == 8) || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..h.len() / 2)
        .map(|i| {
            u8::from_str_radix(&h[i * 2..i * 2 + 2], 16)
                .ok()
                .map(|b| f64::from(b) / 255.0)
        })
        .collect()
}

/// An agent's value as a plug-in's.
fn to_ofx(info: &ParamInfo, v: &Value) -> Result<OfxValue, String> {
    let bad = |what: &str| trf!("\"{}\" takes {}.", info.name, what);
    let clamp = |i: usize, x: f64| {
        let lo = info.min.get(i).copied().unwrap_or(f64::NEG_INFINITY);
        let hi = info.max.get(i).copied().unwrap_or(f64::INFINITY);
        if lo <= hi { x.clamp(lo, hi) } else { x }
    };
    let numbers = |n: usize| -> Result<Vec<f64>, String> {
        match v {
            Value::Number(x) if n == 1 => Ok(vec![clamp(0, x.as_f64().unwrap_or(0.0))]),
            Value::Array(a) if a.len() == n => a
                .iter()
                .enumerate()
                .map(|(i, x)| x.as_f64().map(|x| clamp(i, x)))
                .collect::<Option<Vec<f64>>>()
                .ok_or_else(|| bad(&trf!("{} numbers", n))),
            _ => Err(bad(&trf!("{} numbers", n))),
        }
    };
    Ok(match info.ty {
        ParamType::Integer => OfxValue::Num(vec![numbers(1)?[0].round()]),
        ParamType::Double => OfxValue::Num(numbers(1)?),
        ParamType::Boolean => match v {
            Value::Bool(b) => OfxValue::Num(vec![f64::from(u8::from(*b))]),
            Value::Number(x) => {
                OfxValue::Num(vec![f64::from(u8::from(x.as_f64().unwrap_or(0.0) != 0.0))])
            }
            _ => return Err(bad(tr("true or false"))),
        },
        ParamType::Choice => {
            let i = match v {
                Value::Number(x) => x.as_u64().map(|i| i as usize),
                Value::String(s) => info.options.iter().position(|o| plain(o) == plain(s)),
                _ => None,
            }
            .filter(|i| *i < info.options.len().max(1))
            .ok_or_else(|| bad(&trf!("one of: {}", info.options.join(", "))))?;
            OfxValue::Num(vec![i as f64])
        }
        ParamType::StrChoice => {
            let s = v
                .as_str()
                .ok_or_else(|| bad(&trf!("one of: {}", info.options.join(", "))))?;
            let i = info
                .options
                .iter()
                .position(|o| plain(o) == plain(s))
                .or_else(|| info.option_values.iter().position(|o| o == s))
                .ok_or_else(|| bad(&trf!("one of: {}", info.options.join(", "))))?;
            OfxValue::Text(
                info.option_values
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| s.to_string()),
            )
        }
        ParamType::Rgb | ParamType::Rgba => {
            let n = if info.ty == ParamType::Rgb { 3 } else { 4 };
            let mut c = match v {
                Value::String(s) => hex_color(s).ok_or_else(|| bad("\"#rrggbb\""))?,
                Value::Array(a) if a.len() == 3 || a.len() == 4 => a
                    .iter()
                    .map(|x| x.as_f64())
                    .collect::<Option<Vec<f64>>>()
                    .ok_or_else(|| bad(tr("a color: [r, g, b] from 0 to 1, or \"#rrggbb\"")))?,
                _ => return Err(bad(tr("a color: [r, g, b] from 0 to 1, or \"#rrggbb\""))),
            };
            c.resize(n, 1.0);
            OfxValue::Num(c)
        }
        ParamType::Double2D => OfxValue::Num(numbers(2)?),
        ParamType::Integer2D => OfxValue::Num(numbers(2)?.into_iter().map(f64::round).collect()),
        ParamType::Double3D => OfxValue::Num(numbers(3)?),
        ParamType::Integer3D => OfxValue::Num(numbers(3)?.into_iter().map(f64::round).collect()),
        ParamType::String => OfxValue::Text(v.as_str().ok_or_else(|| bad(tr("text")))?.to_string()),
        _ => return Err(trf!("\"{}\" is set in its window, not here.", info.name)),
    })
}

/// A plug-in parameter's description and value for the agent.
fn param_json(info: &ParamInfo, fx: &PluginFx, place: &FxPlace) -> Value {
    let value = crate::ofx::value_now(info, fx, place);
    let mut j = json!({
        "name": info.name,
        "label": info.label,
        "type": type_name(info.ty),
        "value": from_ofx(info, &value),
        "default": from_ofx(info, &info.default),
    });
    if !info.hint.is_empty() {
        j["hint"] = json!(info.hint);
    }
    if !info.parent.is_empty() {
        j["group"] = json!(info.parent);
    }
    let range = |v: &[f64]| if v.len() == 1 { json!(v[0]) } else { json!(v) };
    if !info.display_min.is_empty()
        && matches!(
            info.ty,
            ParamType::Integer | ParamType::Double | ParamType::Double2D | ParamType::Integer2D
        )
    {
        j["min"] = range(&info.display_min);
        j["max"] = range(&info.display_max);
    }
    if !info.options.is_empty() {
        j["options"] = json!(info.options);
    }
    if info.coordinates.contains("ormalised") {
        j["normalized"] = json!(true);
    }
    if let Some(p) = fx.params.get(&info.name)
        && !p.keys.is_empty()
    {
        let origin = place.origin;
        j["keyframes"] = json!(
            p.keys
                .iter()
                .map(|k| {
                    let v = bv_engine::plugins::value(&k.value);
                    json!({
                        "at": origin.map(|o| secs(o + k.at)),
                        "local": secs(k.at),
                        "value": from_ofx(info, &v),
                        "curve": key_curve_name(k.curve),
                    })
                })
                .collect::<Vec<_>>()
        );
    }
    j
}

/// `v` set on parameter `info` of `fx`: at once, or as a keyframe at
/// `place`'s time (`keyed`) — the parameter animated from what it was when
/// it had no keyframes (a keyframe at its owner's start holding that).
fn set_ofx(
    info: &ParamInfo,
    fx: &mut PluginFx,
    place: &FxPlace,
    v: OfxValue,
    keyed: Option<KeyCurve>,
) -> Result<(), String> {
    let animated = fx
        .params
        .get(&info.name)
        .is_some_and(|p| !p.keys.is_empty());
    let Some(curve) = keyed else {
        if animated {
            return Err(trf!(
                "\"{}\" has keyframes: give \"at\" (a keyframe there), or \"clear_keyframes\".",
                info.name
            ));
        }
        crate::ofx::set_now(info, fx, place, v);
        return Ok(());
    };
    let Some(value) = plugin_value(&v) else {
        return Ok(());
    };
    let start = FxPlace {
        time: place.span.0,
        ..*place
    };
    let before = plugin_value(&crate::ofx::value_now(info, fx, &start));
    let p = fx
        .params
        .entry(info.name.clone())
        .or_insert_with(|| PluginParam {
            value: value.clone(),
            keys: Vec::new(),
        });
    if p.keys.is_empty()
        && place.time != start.time
        && let Some(b) = before
    {
        p.keys.push(PluginKey::new(start.time, b, curve));
    }
    match p.keys.iter_mut().find(|k| k.at == place.time) {
        Some(k) => {
            k.value = value;
            k.curve = curve;
        }
        None => {
            let i = p.keys.partition_point(|k| k.at < place.time);
            p.keys.insert(i, PluginKey::new(place.time, value, curve));
        }
    }
    Ok(())
}

/// A built-in effect's parameter for the agent.
fn spec_json(e: &Effect, i: usize) -> Option<Value> {
    let s = e.kind.params().get(i)?;
    if s.name.is_empty()
        || matches!(
            s.kind,
            bv_timeline::ParamKind::Data | bv_timeline::ParamKind::Graph
        )
    {
        return None;
    }
    let v = e.param(i);
    let mut j = json!({
        "name": s.name,
        "value": v,
        "default": s.default,
        "min": s.min,
        "max": s.max,
    });
    match s.kind {
        bv_timeline::ParamKind::Toggle => {
            j["type"] = json!("boolean");
            j["value"] = json!(v > 0.5);
        }
        bv_timeline::ParamKind::Choice => {
            j["type"] = json!("choice");
            j["options"] = json!(s.options);
            let k = (v - s.min).round().max(0.0) as usize;
            if let Some(o) = s.options.get(k) {
                j["value"] = json!(o);
            }
        }
        _ => {
            j["type"] = json!("number");
            if !s.unit.is_empty() {
                j["unit"] = json!(s.unit);
            }
        }
    }
    if !s.group.is_empty() {
        j["group"] = json!(s.group);
    }
    Some(j)
}

fn set_spec(e: &mut Effect, name: &str, v: &Value) -> Result<(), String> {
    let specs = e.kind.params();
    let i = specs
        .iter()
        .position(|s| !s.name.is_empty() && plain(s.name) == plain(name))
        .ok_or_else(|| trf!("The effect has no parameter \"{}\".", name))?;
    let s = &specs[i];
    let x = match (s.kind, v) {
        (_, Value::Number(n)) => n.as_f64().unwrap_or(0.0) as f32,
        (_, Value::Bool(b)) => f32::from(u8::from(*b)),
        (bv_timeline::ParamKind::Choice, Value::String(o)) => {
            let k = s
                .options
                .iter()
                .position(|x| plain(x) == plain(o))
                .ok_or_else(|| trf!("\"{}\" takes one of: {}.", s.name, s.options.join(", ")))?;
            s.min + k as f32
        }
        _ => return Err(trf!("\"{}\" takes a number.", s.name)),
    };
    e.set_param(i, x);
    Ok(())
}

/// A VST effect's parameters for the agent (its values from 0 to 1).
fn vst_json(e: &Effect) -> Vec<Value> {
    let Some(v) = &e.vst else {
        return Vec::new();
    };
    crate::vst_ui::param_infos(v)
        .iter()
        .filter(|p| p.shown())
        .map(|p| {
            json!({
                "name": p.title,
                "id": p.id,
                "value": v.params.get(&p.id).copied().unwrap_or(p.default),
                "default": p.default,
                "min": 0.0,
                "max": 1.0,
                "unit": p.units,
            })
        })
        .collect()
}

fn set_vst(e: &mut Effect, name: &str, v: &Value) -> Result<(), String> {
    let Some(vst) = e.vst.as_deref_mut() else {
        return Ok(());
    };
    let infos = crate::vst_ui::param_infos(vst);
    let p = infos
        .iter()
        .find(|p| plain(&p.title) == plain(name) || p.id.to_string() == name)
        .ok_or_else(|| trf!("The effect has no parameter \"{}\".", name))?;
    let x = v
        .as_f64()
        .ok_or_else(|| trf!("\"{}\" takes a number from 0 to 1.", p.title))?;
    vst.params.insert(p.id, x.clamp(0.0, 1.0));
    Ok(())
}

/// The place keyframes of `t` count from, at timeline time `at`.
fn place(p: &Project, t: FxTarget, at: Tick) -> FxPlace {
    crate::ofx::place_of(t, p, at)
}

fn effect_json(p: &Project, t: FxTarget, index: usize, e: &Effect, at: Tick) -> Value {
    let mut j = json!({
        "index": index,
        "id": effect_id(e),
        "name": effect_name(e),
        "kind": kind_of_effect(e),
        "enabled": e.enabled,
    });
    let params: Vec<Value> = if let Some(fx) = &e.plugin {
        match bv_ofx::registry().info(&fx.id) {
            Some(info) => match context_in(t, &info) {
                Some(ctx) => {
                    let place = place(p, t, at);
                    params_of(&info, ctx)
                        .iter()
                        .map(|i| param_json(i, fx, &place))
                        .collect()
                }
                None => Vec::new(),
            },
            None => {
                j["missing"] = json!(true);
                Vec::new()
            }
        }
    } else if e.vst.is_some() {
        vst_json(e)
    } else {
        (0..e.kind.params().len())
            .filter_map(|i| spec_json(e, i))
            .collect()
    };
    j["parameters"] = json!(params);
    j
}

/// What effects there are: video (OFX filters and BananaV's built-in
/// one), audio (BananaV's own and the VST plug-ins found), transitions and
/// generators.
fn available(app: &BananaApp, kind: Option<&str>, search: Option<&str>) -> Vec<Value> {
    let wanted = |k: &str| kind.is_none_or(|w| w == k);
    let fits = |s: &[&str]| {
        search.is_none_or(|q| {
            let q = q.to_lowercase();
            s.iter().any(|x| x.to_lowercase().contains(&q))
        })
    };
    let ofx = |list: Vec<std::sync::Arc<PluginInfo>>, k: &str, out: &mut Vec<Value>| {
        for info in list {
            if fits(&[&info.identifier, info.name(), &info.grouping]) {
                out.push(json!({"id": info.identifier, "name": info.name(), "group": info.grouping, "kind": k}));
            }
        }
    };
    let mut out = Vec::new();
    if wanted("video") {
        ofx(crate::ofx::filters(), "video", &mut out);
        for k in EffectKind::VIDEO {
            let id = format!("video:{k:?}");
            if fits(&[&id, k.name(), k.category()]) {
                out.push(
                    json!({"id": id, "name": k.name(), "group": k.category(), "kind": "video"}),
                );
            }
        }
    }
    if wanted("audio") {
        for k in EffectKind::AUDIO {
            let id = format!("audio:{k:?}");
            if fits(&[&id, k.name(), k.category()]) {
                out.push(
                    json!({"id": id, "name": k.name(), "group": k.category(), "kind": "audio"}),
                );
            }
        }
        for v in vst_found(app) {
            let id = format!("vst:{}|{}", v.path.display(), v.class_id);
            if fits(&[&id, &v.name, &v.vendor, &v.category]) {
                out.push(json!({"id": id, "name": v.name, "group": v.vendor, "kind": "audio"}));
            }
        }
    }
    if wanted("transition") {
        ofx(crate::ofx::transitions(), "transition", &mut out);
    }
    if wanted("generator") {
        ofx(crate::ofx::generators(), "generator", &mut out);
    }
    out
}

/// The VST effects found (the search goes on in the background the first
/// time), less those left out or turned off in the preferences.
fn vst_found(app: &BananaApp) -> Vec<crate::vst::VstPlugin> {
    let prefs = &app.settings.prefs;
    crate::vst::plugins(&prefs.vst_folders, false, None)
        .plugins
        .unwrap_or_default()
        .into_iter()
        .filter(|p| {
            p.usable()
                && !prefs.vst_left_out.contains(&p.path)
                && !prefs.vst_effects_off.contains(&p.key())
        })
        .collect()
}

/// A new effect of `id` for a chain of sound (`audio`) or picture.
fn new_effect(app: &BananaApp, id: &str, audio: bool) -> Result<Effect, String> {
    let wrong = || {
        if audio {
            trf!(
                "{} is not an audio effect: list_available_effects with kind \"audio\" gives them.",
                id
            )
        } else {
            trf!(
                "{} is not a video effect: list_available_effects with kind \"video\" gives them.",
                id
            )
        }
    };
    if let Some(name) = id.strip_prefix("audio:") {
        let k = EffectKind::AUDIO
            .iter()
            .find(|k| format!("{k:?}") == name)
            .ok_or_else(wrong)?;
        return if audio {
            Ok(Effect::new(*k))
        } else {
            Err(wrong())
        };
    }
    if let Some(name) = id.strip_prefix("video:") {
        let k = EffectKind::VIDEO
            .iter()
            .find(|k| format!("{k:?}") == name)
            .ok_or_else(wrong)?;
        return if audio {
            Err(wrong())
        } else {
            Ok(Effect::new(*k))
        };
    }
    if let Some(key) = id.strip_prefix("vst:") {
        if !audio {
            return Err(wrong());
        }
        let p = vst_found(app)
            .into_iter()
            .find(|p| format!("{}|{}", p.path.display(), p.class_id) == key)
            .ok_or_else(|| {
                trf!(
                    "There is no VST effect {} (the search may still be going on).",
                    key
                )
            })?;
        return Ok(crate::vst_ui::new_effect(&p));
    }
    let info = bv_ofx::registry()
        .info(id)
        .ok_or_else(|| trf!("There is no effect {}.", id))?;
    if audio || filter_context(&info).is_none() || bv_engine::plugins::compositor_only(id) {
        return Err(wrong());
    }
    Ok(crate::ofx::new_effect(&info))
}

fn list_available_effects(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let kind = a.opt_str("kind")?;
    let search = a.opt_str("search")?;
    done(json!({"effects": available(app, kind, search)}))
}

fn list_effects(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let t = target(app, a)?;
    let chain = chain_of(app, t)?;
    let p = app.project();
    let mut v = json!({
        "audio": t.audio(p),
        "effects": chain.iter().enumerate().map(|(i, e)| json!({
            "index": i,
            "id": effect_id(e),
            "name": effect_name(e),
            "kind": kind_of_effect(e),
            "enabled": e.enabled,
        })).collect::<Vec<_>>(),
    });
    if let FxTarget::Clip(id) = t
        && let Some(c) = p.clip(id)
    {
        v["pan_crop_at"] = json!(c.pan_crop_at);
    }
    done(v)
}

/// The effect at `index` of `t`'s chain.
fn index_in(chain: &[Effect], a: &Args<'_>) -> Result<usize, String> {
    let i = a.u64("index")? as usize;
    if i >= chain.len() {
        return Err(trf!(
            "There is no effect {} in that chain (it has {}).",
            i,
            chain.len()
        ));
    }
    Ok(i)
}

fn add_effect(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let t = target(app, a)?;
    if matches!(t, FxTarget::Transition(..)) {
        return Err(tr("A transition's effect is set with set_transition.").into());
    }
    let mut chain = chain_of(app, t)?;
    let audio = t.audio(app.project());
    let e = new_effect(app, a.str("effect")?, audio)?;
    let at = a
        .opt_u64("at_index")?
        .map_or(chain.len(), |i| (i as usize).min(chain.len()));
    chain.insert(at, e);
    app.agent_modify(|p| {
        t.set_chain(p, chain);
        // The fixed buttons (Pan/Crop, Composite) keep their places.
        match t {
            FxTarget::Clip(id) => {
                if let Some(c) = p.clip_mut(id)
                    && at < c.pan_crop_at
                {
                    c.pan_crop_at += 1;
                }
            }
            FxTarget::Track(id) => {
                if let Some(track) = p.track_by_id_mut(id)
                    && let Some(c) = track.composite_at.as_mut()
                    && at < *c
                {
                    *c += 1;
                }
            }
            _ => {}
        }
    });
    let p = app.project();
    let chain = chain_of(app, t)?;
    done(effect_json(p, t, at, &chain[at], app.cursor))
}

fn remove_effect(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let t = target(app, a)?;
    if matches!(t, FxTarget::Transition(..)) {
        return Err(tr("A transition is taken away with set_transition and \"remove\".").into());
    }
    let mut chain = chain_of(app, t)?;
    let i = index_in(&chain, a)?;
    chain.remove(i);
    app.agent_modify(|p| {
        t.set_chain(p, chain);
        match t {
            FxTarget::Clip(id) => {
                if let Some(c) = p.clip_mut(id)
                    && i < c.pan_crop_at
                {
                    c.pan_crop_at -= 1;
                }
            }
            FxTarget::Track(id) => {
                if let Some(track) = p.track_by_id_mut(id)
                    && let Some(c) = track.composite_at.as_mut()
                    && i < *c
                {
                    *c -= 1;
                }
            }
            _ => {}
        }
    });
    done(json!({"removed": true, "effects": chain_of(app, t)?.len()}))
}

fn get_effect(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let t = target(app, a)?;
    let chain = chain_of(app, t)?;
    let i = index_in(&chain, a)?;
    let at = a.opt_time("at")?.unwrap_or(app.cursor);
    done(effect_json(app.project(), t, i, &chain[i], at))
}

/// `values` set on `fx` (a plug-in of `ctx`) at `place`, keyframed with
/// `keyed`; parameters in `clear` lose their keyframes first.
fn set_plugin_values(
    fx: &mut PluginFx,
    ctx: Context,
    place: &FxPlace,
    values: &Map<String, Value>,
    keyed: Option<KeyCurve>,
    clear: bool,
) -> Result<(), String> {
    let info = bv_ofx::registry()
        .info(&fx.id)
        .ok_or_else(|| trf!("The plug-in {} is not on this computer.", fx.id))?;
    let params = params_of(&info, ctx);
    let find = |name: &str| {
        params
            .iter()
            .find(|p| p.name == name)
            .or_else(|| {
                params
                    .iter()
                    .find(|p| plain(&p.name) == plain(name) || plain(&p.label) == plain(name))
            })
            .cloned()
            .ok_or_else(|| {
                trf!(
                    "The effect has no parameter \"{}\" (get_effect lists them).",
                    name
                )
            })
    };
    if clear {
        let names: Vec<String> = if values.is_empty() {
            fx.params.keys().cloned().collect()
        } else {
            values
                .keys()
                .map(|n| find(n).map(|p| p.name))
                .collect::<Result<_, _>>()?
        };
        for n in names {
            if let Some(info) = params.iter().find(|p| p.name == n) {
                let now = crate::ofx::value_now(info, fx, place);
                if let (Some(p), Some(v)) = (fx.params.get_mut(&n), plugin_value(&now)) {
                    p.keys.clear();
                    p.value = v;
                }
            }
        }
    }
    let old_text = fx
        .params
        .get("text")
        .map(|p| bv_engine::plugins::value(&p.value).text().to_string());
    for (name, v) in values {
        let info = find(name)?;
        let value = to_ofx(&info, v)?;
        set_ofx(&info, fx, place, value, keyed)?;
    }
    // Titles & Text: its styled runs follow the text's change.
    if fx.id == bv_engine::plugins::TITLES_ID
        && let (Some(old), Some(new)) = (
            old_text,
            fx.params
                .get("text")
                .map(|p| bv_engine::plugins::value(&p.value).text().to_string()),
        )
        && old != new
        && let Some(styles) = fx.params.get_mut("styles")
        && let bv_timeline::PluginValue::Text(runs) = &mut styles.value
    {
        *runs = crate::ofx::shift_runs(runs, &old, &new);
    }
    Ok(())
}

fn set_effect(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let t = target(app, a)?;
    let mut chain = chain_of(app, t)?;
    let i = index_in(&chain, a)?;
    let values = a
        .get("values")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let at = a.opt_time("at")?;
    let keyed = at.map(|_| key_curve(a.opt_str("curve").ok().flatten().unwrap_or("linear")));
    let clear = a.bool_or("clear_keyframes", false)?;
    let when = at.map_or(app.cursor, |t| {
        app.project().settings.frame_rate.snap_nearest(t)
    });
    let place = place(app.project(), t, when);
    let e = &mut chain[i];
    if let Some(on) = a.opt_bool("enabled")? {
        e.enabled = on;
    }
    if let Some(fx) = e.plugin.as_deref_mut() {
        let info = bv_ofx::registry()
            .info(&fx.id)
            .ok_or_else(|| trf!("The plug-in {} is not on this computer.", fx.id))?;
        let ctx = context_in(t, &info)
            .ok_or_else(|| tr("That plug-in has no settings here.").to_string())?;
        set_plugin_values(fx, ctx, &place, &values, keyed, clear)?;
    } else if e.vst.is_some() {
        if at.is_some() {
            return Err(tr(
                "A VST effect's parameters change over time through envelopes, not keyframes here.",
            )
            .into());
        }
        for (name, v) in &values {
            set_vst(e, name, v)?;
        }
    } else {
        if at.is_some() {
            return Err(tr(
                "This effect's parameters change over time through envelopes, not keyframes here.",
            )
            .into());
        }
        for (name, v) in &values {
            set_spec(e, name, v)?;
        }
    }
    app.agent_modify(|p| t.set_chain(p, chain));
    let chain = chain_of(app, t)?;
    done(effect_json(app.project(), t, i, &chain[i], when))
}

fn set_transition(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let c = clip_of(app.project(), a.u64("event")?)?;
    let id = c.id;
    let out = match a.opt_str("side")?.unwrap_or("in") {
        "out" => true,
        "in" => false,
        other => return Err(trf!("\"side\" is \"in\" or \"out\", not \"{}\".", other)),
    };
    let length = a
        .opt_time("length")?
        .map(|t| app.project().settings.frame_rate.snap_nearest(t));
    if a.bool_or("remove", false)? {
        app.agent_modify(|p| {
            if let Some(c) = p.clip_mut(id) {
                c.set_transition_plugin(out, None, false);
            }
        });
        return done(event_json_of(app, id));
    }
    let fx = match a.opt_str("effect")? {
        Some(e) => {
            let info = bv_ofx::registry()
                .info(e)
                .filter(|i| bv_engine::plugins::transition_context(i).is_some())
                .ok_or_else(|| trf!("{} is not a transition: list_available_effects with kind \"transition\" gives them.", e))?;
            Some(crate::ofx::plugin_fx(&info))
        }
        None => None,
    };
    if fx.is_none() && length.is_none() {
        return Err(tr("Give \"effect\" (a transition), \"length\" or \"remove\".").into());
    }
    if !out {
        if let Some(fx) = &fx {
            crate::tabs::transitions::apply_plugin(app, &[id], fx);
        }
        if let Some(l) = length {
            app.agent_edit(|p| p.set_fades(id, Some(l), None))?;
        }
    } else {
        let len = length.unwrap_or(Tick::from_seconds(1));
        app.agent_edit(|p| -> Result<(), bv_timeline::EditError> {
            if let Some(fx) = fx
                && let Some(c) = p.clip_mut(id)
            {
                c.set_transition_plugin(true, Some(fx), false);
            }
            if length.is_some() || p.clip(id).is_some_and(|c| c.fade_out == Tick::ZERO) {
                p.set_fades(id, None, Some(len))?;
            }
            Ok(())
        })?;
    }
    done(event_json_of(app, id))
}

fn event_json_of(app: &BananaApp, id: ClipId) -> Value {
    let p = app.project();
    match p.find_clip(id) {
        Some((t, _)) => {
            let c = p
                .clip(id)
                .map(|c| event_json(p, t, c))
                .unwrap_or(Value::Null);
            let mut v = c;
            if let Some(c) = p.clip(id) {
                let (fin, _) = c.transition_plugin(false);
                let (fout, _) = c.transition_plugin(true);
                v["transition_in"] = json!(fin.map(|f| f.id.clone()));
                v["transition_out"] = json!(fout.map(|f| f.id.clone()));
            }
            v
        }
        None => Value::Null,
    }
}

/// The plug-in a generated event's media is made by.
fn generator_of(p: &Project, id: ClipId) -> Result<(bv_timeline::MediaId, PluginFx), String> {
    let c = p
        .clip(id)
        .ok_or_else(|| trf!("There is no event {}.", id.0))?;
    let m = p
        .media(c.media)
        .ok_or_else(|| trf!("There is no media {}.", c.media.0))?;
    match &m.source {
        MediaSource::Generated(Generator::Plugin(fx)) => Ok((m.id, (**fx).clone())),
        _ => Err(tr("That event's media is not made by a generator plug-in.").into()),
    }
}

fn add_generated_media(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let id = a.str("generator")?;
    let info = bv_ofx::registry()
        .info(id)
        .filter(|i| bv_engine::plugins::generator_context(i).is_some())
        .ok_or_else(|| {
            trf!(
                "{} is not a generator: list_available_effects with kind \"generator\" gives them.",
                id
            )
        })?;
    let Generator::Plugin(mut fx) = crate::ofx::new_generator(&info) else {
        return Err(trf!(
            "{} is not a generator: list_available_effects with kind \"generator\" gives them.",
            id
        ));
    };
    let values = a
        .get("values")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let p = app.project();
    let place = FxPlace {
        rate: p.settings.frame_rate,
        time: Tick::ZERO,
        duration: Generator::DURATION,
        size: (p.settings.width, p.settings.height),
        map: crate::ofx::place_of(FxTarget::VideoOutput, p, Tick::ZERO).map,
        span: (Tick::ZERO, Generator::DURATION),
        origin: None,
    };
    set_plugin_values(&mut fx, Context::Generator, &place, &values, None, false)?;
    let track = match a.opt_u64("track")? {
        Some(t) => Some(track_at(app.project(), t)?.id),
        None => None,
    };
    let at = a.opt_time("at")?.map_or(app.cursor, |t| {
        app.project().settings.frame_rate.snap_nearest(t)
    });
    let length = a.opt_time("length")?;
    let before = app.history.revision();
    app.as_one_undo(|app| {
        app.add_generator(Generator::Plugin(fx), at, track);
        if let Some(l) = length {
            let ids: Vec<ClipId> = app.selection.iter().copied().collect();
            app.agent_modify(|p| {
                for id in ids {
                    let _ = p.set_clip_duration(id, l);
                }
            });
        }
    });
    if app.history.revision() == before {
        return Err(tr("Nothing was placed.").into());
    }
    let ids: Vec<ClipId> = app.selection.iter().copied().collect();
    done(json!({"events": ids.iter().map(|&id| event_json_of(app, id)).collect::<Vec<_>>()}))
}

fn get_generated_media(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let c = clip_of(app.project(), a.u64("event")?)?.id;
    let (media, fx) = generator_of(app.project(), c)?;
    let at = a.opt_time("at")?.unwrap_or(app.cursor);
    let p = app.project();
    let place = crate::ofx::generator_place(p, c, at)
        .ok_or_else(|| tr("That event's media is not made by a generator plug-in.").to_string())?;
    let params = bv_ofx::registry()
        .info(&fx.id)
        .map(|info| {
            params_of(&info, Context::Generator)
                .iter()
                .map(|i| param_json(i, &fx, &place))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    done(json!({"media": media.0, "generator": fx.id, "name": fx.name, "parameters": params}))
}

fn set_generated_media(app: &mut BananaApp, a: &Args<'_>) -> Outcome {
    let c = clip_of(app.project(), a.u64("event")?)?.id;
    let (media, mut fx) = generator_of(app.project(), c)?;
    let values = a
        .get("values")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let at = a.opt_time("at")?;
    let keyed = at.map(|_| key_curve(a.opt_str("curve").ok().flatten().unwrap_or("linear")));
    let clear = a.bool_or("clear_keyframes", false)?;
    let when = at.map_or(app.cursor, |t| {
        app.project().settings.frame_rate.snap_nearest(t)
    });
    let place = crate::ofx::generator_place(app.project(), c, when)
        .ok_or_else(|| tr("That event's media is not made by a generator plug-in.").to_string())?;
    set_plugin_values(&mut fx, Context::Generator, &place, &values, keyed, clear)?;
    app.agent_modify(|p| {
        if let Some(m) = p.media.iter_mut().find(|m| m.id == media) {
            m.source = MediaSource::Generated(Generator::Plugin(Box::new(fx)));
        }
    });
    get_generated_media(app, a)
}

pub(super) fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "list_available_effects",
            title: "List the effects there are",
            description: "The video effects, audio effects, transitions and generators BananaV has on this computer: their id (for add_effect, set_transition, add_generated_media), name and group.",
            properties: || {
                json!({
                    "kind": choice(&["video", "audio", "transition", "generator"], "Only these."),
                    "search": arg("string", "Only those whose id, name or group has this text."),
                })
            },
            required: &[],
            kind: Kind::Read,
            run: list_available_effects,
        },
        Tool {
            name: "list_effects",
            title: "List a chain's effects",
            description: "The effects of an event's, a track's, a media's, the video output's or the Master bus's chain, in order.",
            properties: || Value::Object(target_args(false)),
            required: &[],
            kind: Kind::Read,
            run: list_effects,
        },
        Tool {
            name: "add_effect",
            title: "Add an effect",
            description: "Adds an effect (an id from list_available_effects) to a chain: a video one to an event, a video track, a media's picture or the video output; an audio one to an audio event, an audio track, a media's sound or the Master bus. Returns it with its parameters.",
            properties: || {
                let mut m = target_args(false);
                m.insert("effect".into(), arg("string", "The effect's id."));
                m.insert(
                    "at_index".into(),
                    arg("integer", "Where in the chain (the end if not given)."),
                );
                Value::Object(m)
            },
            required: &["effect"],
            kind: Kind::Edit,
            run: add_effect,
        },
        Tool {
            name: "remove_effect",
            title: "Remove an effect",
            description: "Takes an effect out of its chain.",
            properties: || Value::Object(target_args(true)),
            required: &["index"],
            kind: Kind::Remove,
            run: remove_effect,
        },
        Tool {
            name: "get_effect",
            title: "Get an effect's parameters",
            description: "An effect's parameters: name, label, type, value (at `at`, else the cursor), default, range, options, and keyframes (their times from the project's start, `local` from the owner's: the event, media or transition).",
            properties: || {
                let mut m = target_args(true);
                m.insert(
                    "at".into(),
                    arg("number", "The time the values are read at (seconds)."),
                );
                Value::Object(m)
            },
            required: &["index"],
            kind: Kind::Read,
            run: get_effect,
        },
        Tool {
            name: "set_effect",
            title: "Set an effect's parameters",
            description: "Sets an effect's parameters by name (`values`: {\"name\": value}); numbers, true/false, an option's label, colors as [r, g, b(, a)] from 0 to 1 or \"#rrggbb\", points as [x, y]. With `at` (seconds from the project's start) each becomes a keyframe there — the first one also keeps the value it had at the owner's start. `enabled` turns the effect on or off; `clear_keyframes` takes the keyframes away (of the parameters in `values`, else of all).",
            properties: || {
                let mut m = target_args(true);
                m.insert(
                    "values".into(),
                    arg("object", "{\"parameter name\": value}."),
                );
                m.insert(
                    "at".into(),
                    arg("number", "Keyframes at this time (seconds)."),
                );
                m.insert(
                    "curve".into(),
                    choice(
                        &KEYFRAME_CURVES,
                        "The keyframes' curve (linear if not given).",
                    ),
                );
                m.insert("enabled".into(), arg("boolean", "The effect on or off."));
                m.insert(
                    "clear_keyframes".into(),
                    arg("boolean", "Take the keyframes away first."),
                );
                Value::Object(m)
            },
            required: &["index"],
            kind: Kind::Edit,
            run: set_effect,
        },
        Tool {
            name: "set_transition",
            title: "Set a transition",
            description: "Gives an event an entrance (side in: over its fade-in or the overlap with the event before) or exit (side out: over its fade-out) transition — an id from list_available_effects with kind transition — of a length; or changes its length, or takes it away (`remove`). Its parameters: get_effect/set_effect with `event` and `transition`.",
            properties: || {
                json!({
                    "event": arg("integer", "The event's id."),
                    "side": choice(&["in", "out"], "Its entrance (default) or its exit."),
                    "effect": arg("string", "The transition's id."),
                    "length": arg("number", "How long it lasts (seconds)."),
                    "remove": arg("boolean", "Take it away."),
                })
            },
            required: &["event"],
            kind: Kind::Edit,
            run: set_transition,
        },
        Tool {
            name: "add_generated_media",
            title: "Add generated media",
            description: "Places media a generator makes (an id from list_available_effects with kind generator: titles and text, solid color, gradients, patterns, credits…) on a video track at `at` (the cursor if not given), with its parameters (`values`, as set_effect takes them; e.g. Titles & Text's \"text\").",
            properties: || {
                json!({
                    "generator": arg("string", "The generator's id (Titles & Text: org.bananav.TitlesText)."),
                    "values": arg("object", "{\"parameter name\": value}."),
                    "track": arg("integer", "The video track (index)."),
                    "at": arg("number", "Where it starts (seconds)."),
                    "length": arg("number", "How long the event is (seconds)."),
                })
            },
            required: &["generator"],
            kind: Kind::Edit,
            run: add_generated_media,
        },
        Tool {
            name: "get_generated_media",
            title: "Get a generated media's parameters",
            description: "The parameters of the generator that makes an event's media, as get_effect gives an effect's.",
            properties: || {
                json!({
                    "event": arg("integer", "The event's id."),
                    "at": arg("number", "The time the values are read at (seconds)."),
                })
            },
            required: &["event"],
            kind: Kind::Read,
            run: get_generated_media,
        },
        Tool {
            name: "set_generated_media",
            title: "Set a generated media's parameters",
            description: "Sets the parameters of the generator that makes an event's media (every event of that media changes), as set_effect sets an effect's (with `at`, keyframes).",
            properties: || {
                json!({
                    "event": arg("integer", "The event's id."),
                    "values": arg("object", "{\"parameter name\": value}."),
                    "at": arg("number", "Keyframes at this time (seconds)."),
                    "curve": choice(&KEYFRAME_CURVES, "The keyframes' curve (linear if not given)."),
                    "clear_keyframes": arg("boolean", "Take the keyframes away first."),
                })
            },
            required: &["event"],
            kind: Kind::Edit,
            run: set_generated_media,
        },
    ]
}
