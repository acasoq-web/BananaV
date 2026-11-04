//! Numbers as the interface language writes them (`1.5` in English, `1,5`
//! in Portuguese).

/// `v` with `decimals` places and the language's decimal separator.
/// "1.25 GB": a size in bytes, two decimals from KB on.
pub fn bytes(bytes: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes.max(0.0);
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    format!("{} {}", num(v, if u == 0 { 0 } else { 2 }), UNITS[u])
}

/// A file's date and time on this computer's clock, as the language writes
/// it: "10/16/2026 2:51 PM" in English, "16/10/2026 14:51" in Portuguese.
pub fn date_time(t: std::time::SystemTime) -> String {
    let secs = match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    } + bv_platform::utc_offset();
    date_time_in(crate::i18n::language(), secs)
}

/// A moment `secs` since 1970 (UTC; the clock `utc_offset` seconds off
/// it) written out whole, as the language writes it: "Monday, November 7,
/// 2022 1:20:01 PM" in English, "segunda-feira, 7 de novembro de 2022
/// 13:20:01" in Portuguese.
pub fn long_date_time(secs: i64, utc_offset: i64) -> String {
    let (weekday, month, d, y, time) = date_words(secs, utc_offset, false);
    trf!("{0}, {1} {2}, {3} {4}", weekday, month, d, y, time)
}

/// A file's date and time as its Properties' General tab writes them, the
/// system's long date and then its time: "Tuesday, October 20, 2026,
/// 12:04:51 AM" in English, "terça-feira, 20 de outubro de 2026, 00:04:51"
/// in Portuguese.
pub fn file_date_time(secs: i64, utc_offset: i64) -> String {
    let (weekday, month, d, y, time) = date_words(secs, utc_offset, true);
    trf!("{0}, {1} {2}, {3}, {4}", weekday, month, d, y, time)
}

/// A moment's weekday and month (their names), day, year and time — in 12
/// hours in English, in 24 in the others (`padded`: its hours two digits).
fn date_words(
    secs: i64,
    utc_offset: i64,
    padded: bool,
) -> (&'static str, &'static str, u32, i64, String) {
    use crate::i18n::tr;
    let (y, m, d, wd, h, mi, s) = bv_timeline::notes::civil_time(secs, utc_offset);
    let weekday = [
        tr("Sunday"),
        tr("Monday"),
        tr("Tuesday"),
        tr("Wednesday"),
        tr("Thursday"),
        tr("Friday"),
        tr("Saturday"),
    ][wd as usize];
    let month = [
        tr("January"),
        tr("February"),
        tr("March"),
        tr("April"),
        tr("May"),
        tr("June"),
        tr("July"),
        tr("August"),
        tr("September"),
        tr("October"),
        tr("November"),
        tr("December"),
    ][(m as usize).saturating_sub(1) % 12];
    let time = if crate::i18n::language() == crate::i18n::Language::English {
        let (h12, ampm) = match h {
            0 => (12, "AM"),
            1..=11 => (h, "AM"),
            12 => (12, "PM"),
            _ => (h - 12, "PM"),
        };
        format!("{h12}:{mi:02}:{s:02} {ampm}")
    } else if padded {
        format!("{h:02}:{mi:02}:{s:02}")
    } else {
        format!("{h}:{mi:02}:{s:02}")
    };
    (weekday, month, d, y, time)
}

/// [`date_time`] of `secs` since 1970 (local), in `lang`.
pub fn date_time_in(lang: crate::i18n::Language, secs: i64) -> String {
    let (y, m, d) = bv_platform::civil_from_days(secs.div_euclid(86_400));
    let in_day = secs.rem_euclid(86_400);
    let (h, min) = (in_day / 3600, in_day % 3600 / 60);
    if lang == crate::i18n::Language::English {
        let (h12, half) = match h {
            0 => (12, "AM"),
            1..=11 => (h, "AM"),
            12 => (12, "PM"),
            _ => (h - 12, "PM"),
        };
        format!("{m}/{d}/{y} {h12}:{min:02} {half}")
    } else {
        format!("{d:02}/{m:02}/{y} {h:02}:{min:02}")
    }
}

pub fn num(v: f64, decimals: usize) -> String {
    let sep = crate::i18n::language().decimal_separator();
    let s = format!("{v:.decimals$}").replace('.', &sep.to_string());
    // No "-0.0": a negative that rounds to zero is zero.
    match s.strip_prefix('-') {
        Some(rest) if rest.chars().all(|c| c == '0' || c == sep) => rest.to_string(),
        _ => s,
    }
}

/// A whole number with the language's thousands separator: "48,000" in
/// English, "48.000" in Portuguese.
pub fn grouped(n: u64) -> String {
    grouped_in(crate::i18n::language(), n)
}

/// [`grouped`] in `lang`.
pub fn grouped_in(lang: crate::i18n::Language, n: u64) -> String {
    let sep = lang.thousands_separator();
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

/// Like [`num`], with an explicit sign (+/−).
pub fn signed(v: f64, decimals: usize) -> String {
    let s = num(v.abs(), decimals);
    if v < 0.0 && s.chars().any(|c| c.is_ascii_digit() && c != '0') {
        format!("−{s}")
    } else {
        format!("+{s}")
    }
}

/// Frame rate: "29.97 fps", "25 fps".
pub fn fps(r: bv_core::FrameRate) -> String {
    if r.den == 1 {
        format!("{} fps", r.num)
    } else {
        format!(
            "{} fps",
            num(r.as_f64(), 3)
                .trim_end_matches('0')
                .trim_end_matches([',', '.'])
        )
    }
}

/// Slider showing the language's decimal separator (typing accepts comma or
/// point).
pub fn slider<'a>(
    value: &'a mut f32,
    range: std::ops::RangeInclusive<f32>,
    decimals: usize,
    suffix: &str,
) -> crate::egui::Slider<'a> {
    let suffix = suffix.to_string();
    crate::egui::Slider::new(value, range)
        .custom_formatter(move |v, _| format!("{}{suffix}", num(v, decimals)))
        .custom_parser(parse)
}

/// Reads a number typed with comma or point (and a "−" sign), ignoring the
/// unit at the end.
pub fn parse(s: &str) -> Option<f64> {
    s.trim()
        .trim_end_matches(|c: char| !c.is_ascii_digit())
        .replace(',', ".")
        .replace('−', "-")
        .parse()
        .ok()
}

/// Drag field showing the language's decimal separator.
pub fn drag<'a>(value: &'a mut f32, decimals: usize, suffix: &str) -> crate::egui::DragValue<'a> {
    let suffix = suffix.to_string();
    crate::egui::DragValue::new(value)
        .custom_formatter(move |v, _| format!("{}{suffix}", num(v, decimals)))
        .custom_parser(parse)
}

/// [`num`] with the thousands grouped, as VEGAS's property grids show
/// numbers: "1,920.0" in English, "1.920,0" in Portuguese.
pub fn num_grouped(v: f64, decimals: usize) -> String {
    let lang = crate::i18n::language();
    let s = num(v, decimals);
    let (sign, rest) = match s.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", s.as_str()),
    };
    let (whole, frac) = match rest.find(lang.decimal_separator()) {
        Some(i) => rest.split_at(i),
        None => (rest, ""),
    };
    match whole.parse::<u64>() {
        Ok(n) => format!("{sign}{}{frac}", grouped_in(lang, n)),
        Err(_) => s,
    }
}

/// Reads a number as [`num_grouped`] writes it (or as [`parse`] reads
/// one): with both a point and a comma, the last one parts the decimals
/// and the other groups the thousands.
pub fn parse_grouped(s: &str) -> Option<f64> {
    let t = s.trim();
    match (t.rfind('.'), t.rfind(',')) {
        (Some(p), Some(c)) => {
            let thousands = if p > c { ',' } else { '.' };
            parse(&t.replace(thousands, ""))
        }
        _ => parse(t),
    }
}

/// [`drag`] showing [`num_grouped`].
pub fn drag_grouped<'a, N: crate::egui::emath::Numeric>(
    value: &'a mut N,
    decimals: usize,
) -> crate::egui::DragValue<'a> {
    crate::egui::DragValue::new(value)
        .custom_formatter(move |v, _| num_grouped(v, decimals))
        .custom_parser(parse_grouped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Language;

    #[test]
    fn thousands_follow_the_language() {
        assert_eq!(grouped_in(Language::English, 48_000), "48,000");
        assert_eq!(
            grouped_in(Language::PortugueseBrazil, 1_234_567),
            "1.234.567"
        );
        assert_eq!(grouped_in(Language::English, 999), "999");
    }

    /// VEGAS's grids: the thousands grouped, and typed back however
    /// grouped.
    #[test]
    fn grouped_numbers_read_back() {
        assert_eq!(parse_grouped("1.920,0"), Some(1920.0));
        assert_eq!(parse_grouped("1,920.5"), Some(1920.5));
        assert_eq!(parse_grouped("960,5"), Some(960.5));
        assert_eq!(parse_grouped("-12.5"), Some(-12.5));
        let s = num_grouped(-1920.04, 1);
        assert_eq!(parse_grouped(&s), Some(-1920.0), "{s}");
    }
}
