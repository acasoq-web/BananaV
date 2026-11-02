//! C's `printf` formatting, for the messages plug-ins send (the message
//! suite takes a format and its values).

/// Where the values of a format come from.
pub(crate) trait Args {
    /// The next integer slot, all 64 bits.
    fn word(&mut self) -> u64;
    fn double(&mut self) -> f64;
    /// The next value, a C string (`None` for a null pointer).
    fn string(&mut self) -> Option<String>;
}

/// `fmt` with its conversions replaced by `args`' values. Supports the
/// flags `-+ #0`, width and precision (also `*`), the length modifiers and
/// the conversions `d i u o x X c s p f F e E g G a A %` (`n` is skipped).
pub(crate) fn format(fmt: &[u8], args: &mut impl Args) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < fmt.len() {
        let c = fmt[i];
        if c != b'%' {
            let start = i;
            while i < fmt.len() && fmt[i] != b'%' {
                i += 1;
            }
            out.push_str(&String::from_utf8_lossy(&fmt[start..i]));
            continue;
        }
        i += 1;
        let mut spec = Spec::default();
        while let Some(&f) = fmt.get(i) {
            match f {
                b'-' => spec.left = true,
                b'+' => spec.plus = true,
                b' ' => spec.space = true,
                b'#' => spec.alt = true,
                b'0' => spec.zero = true,
                _ => break,
            }
            i += 1;
        }
        if fmt.get(i) == Some(&b'*') {
            let w = args.word() as u32 as i32;
            if w < 0 {
                spec.left = true;
            }
            spec.width = w.unsigned_abs() as usize;
            i += 1;
        } else {
            while let Some(d) = fmt.get(i).filter(|d| d.is_ascii_digit()) {
                spec.width = spec.width * 10 + (d - b'0') as usize;
                i += 1;
            }
        }
        if fmt.get(i) == Some(&b'.') {
            i += 1;
            let mut p = 0;
            if fmt.get(i) == Some(&b'*') {
                p = (args.word() as u32 as i32).max(0) as usize;
                i += 1;
            } else {
                while let Some(d) = fmt.get(i).filter(|d| d.is_ascii_digit()) {
                    p = p * 10 + (d - b'0') as usize;
                    i += 1;
                }
            }
            spec.precision = Some(p);
        }
        // Length: 64-bit with `ll`, `j`, `z`, `t` (and `l` where long is).
        let mut wide = false;
        while let Some(&l) = fmt.get(i) {
            match l {
                b'h' | b'L' | b'q' => {}
                b'l' => wide |= cfg!(not(windows)) || fmt.get(i + 1) == Some(&b'l'),
                b'j' | b'z' | b't' => wide = true,
                _ => break,
            }
            i += 1;
        }
        let Some(&conv) = fmt.get(i) else {
            break;
        };
        i += 1;
        let text = match conv {
            b'%' => "%".to_string(),
            b'd' | b'i' => {
                let w = args.word();
                let v = if wide {
                    w as i64
                } else {
                    w as u32 as i32 as i64
                };
                signed(v, &spec)
            }
            b'u' | b'o' | b'x' | b'X' => {
                let w = args.word();
                let v = if wide { w } else { w as u32 as u64 };
                unsigned(v, conv, &spec)
            }
            b'c' => char::from(args.word() as u8).to_string(),
            b's' => {
                let s = args.string().unwrap_or_else(|| "(null)".to_string());
                match spec.precision {
                    Some(p) => s.chars().take(p).collect(),
                    None => s,
                }
            }
            b'p' => format!("{:#x}", args.word()),
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' | b'a' | b'A' => {
                floating(args.double(), conv, &spec)
            }
            b'n' => {
                args.word();
                String::new()
            }
            other => format!("%{}", char::from(other)),
        };
        pad(&mut out, text, &spec, matches!(conv, b's' | b'c' | b'%'));
    }
    out
}

#[derive(Debug, Default)]
struct Spec {
    left: bool,
    plus: bool,
    space: bool,
    alt: bool,
    zero: bool,
    width: usize,
    precision: Option<usize>,
}

fn sign(negative: bool, spec: &Spec) -> &'static str {
    if negative {
        "-"
    } else if spec.plus {
        "+"
    } else if spec.space {
        " "
    } else {
        ""
    }
}

fn signed(v: i64, spec: &Spec) -> String {
    let digits = v.unsigned_abs().to_string();
    let digits = match spec.precision {
        Some(p) if digits.len() < p => format!("{digits:0>p$}"),
        _ => digits,
    };
    format!("{}{digits}", sign(v < 0, spec))
}

fn unsigned(v: u64, conv: u8, spec: &Spec) -> String {
    let digits = match conv {
        b'o' => format!("{v:o}"),
        b'x' => format!("{v:x}"),
        b'X' => format!("{v:X}"),
        _ => v.to_string(),
    };
    let digits = match spec.precision {
        Some(p) if digits.len() < p => format!("{digits:0>p$}"),
        _ => digits,
    };
    match (spec.alt && v != 0, conv) {
        (true, b'x') => format!("0x{digits}"),
        (true, b'X') => format!("0X{digits}"),
        (true, b'o') => format!("0{digits}"),
        _ => digits,
    }
}

fn floating(v: f64, conv: u8, spec: &Spec) -> String {
    if !v.is_finite() {
        let s = if v.is_nan() { "nan" } else { "inf" };
        let s = format!("{}{s}", sign(v.is_sign_negative() && !v.is_nan(), spec));
        return if conv.is_ascii_uppercase() {
            s.to_uppercase()
        } else {
            s
        };
    }
    let p = spec.precision.unwrap_or(6);
    let body = match conv {
        b'f' | b'F' => format!("{:.p$}", v.abs()),
        b'e' | b'E' => exponent(v.abs(), p),
        b'a' | b'A' => format!("{:e}", v.abs()),
        _ => {
            // %g: the shorter of %e and %f, trailing zeros removed.
            let p = p.max(1);
            let exp = if v == 0.0 {
                0
            } else {
                v.abs().log10().floor() as i32
            };
            let s = if exp < -4 || exp >= p as i32 {
                exponent(v.abs(), p - 1)
            } else {
                let decimals = (p as i32 - 1 - exp).max(0) as usize;
                format!("{:.decimals$}", v.abs())
            };
            if spec.alt { s } else { trim_zeros(s) }
        }
    };
    let s = format!("{}{body}", sign(v.is_sign_negative() && v != 0.0, spec));
    if conv.is_ascii_uppercase() {
        s.to_uppercase()
    } else {
        s
    }
}

/// `v` as C's `%e`: one digit, `p` decimals, an exponent of two digits.
fn exponent(v: f64, p: usize) -> String {
    let s = format!("{v:.p$e}");
    match s.split_once('e') {
        Some((mantissa, exp)) => {
            let e: i32 = exp.parse().unwrap_or(0);
            format!("{mantissa}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
        }
        None => s,
    }
}

fn trim_zeros(s: String) -> String {
    match s.split_once('e') {
        Some((m, e)) => format!("{}e{e}", trim_zeros(m.to_string())),
        None if s.contains('.') => s.trim_end_matches('0').trim_end_matches('.').to_string(),
        None => s,
    }
}

fn pad(out: &mut String, text: String, spec: &Spec, textual: bool) {
    let len = text.chars().count();
    if len >= spec.width {
        out.push_str(&text);
        return;
    }
    let fill = spec.width - len;
    if spec.left {
        out.push_str(&text);
        out.extend(std::iter::repeat_n(' ', fill));
    } else if spec.zero && !textual {
        // Zeros after the sign.
        let (sign, digits) = match text.chars().next() {
            Some(c @ ('-' | '+' | ' ')) => (c.to_string(), text[1..].to_string()),
            _ => (String::new(), text),
        };
        out.push_str(&sign);
        out.extend(std::iter::repeat_n('0', fill));
        out.push_str(&digits);
    } else {
        out.extend(std::iter::repeat_n(' ', fill));
        out.push_str(&text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        words: Vec<u64>,
        doubles: Vec<f64>,
        strings: Vec<Option<String>>,
    }

    impl Args for Fake {
        fn word(&mut self) -> u64 {
            self.words.remove(0)
        }
        fn double(&mut self) -> f64 {
            self.doubles.remove(0)
        }
        fn string(&mut self) -> Option<String> {
            self.strings.remove(0)
        }
    }

    fn fmt(f: &str, words: &[u64], doubles: &[f64], strings: &[Option<&str>]) -> String {
        let mut a = Fake {
            words: words.to_vec(),
            doubles: doubles.to_vec(),
            strings: strings.iter().map(|s| s.map(str::to_string)).collect(),
        };
        format(f.as_bytes(), &mut a)
    }

    #[test]
    fn like_c() {
        assert_eq!(fmt("plain 100%%", &[], &[], &[]), "plain 100%");
        assert_eq!(
            fmt(
                "%d|%5d|%-5d|%05d",
                &[(-7i32) as u32 as u64, 42, 42, 42],
                &[],
                &[]
            ),
            "-7|   42|42   |00042"
        );
        assert_eq!(
            fmt("%x %#X %o %u", &[255, 255, 8, 3], &[], &[]),
            "ff 0XFF 10 3"
        );
        assert_eq!(
            fmt("%.2f %8.3f %e", &[], &[1.23456, -2.5, 1234.5], &[]),
            "1.23   -2.500 1.234500e+03"
        );
        assert_eq!(
            fmt("%g %g %g", &[], &[0.0001, 100000.0, 1e7], &[]),
            "0.0001 100000 1e+07"
        );
        assert_eq!(
            fmt(
                "%s, %.3s, %s",
                &[],
                &[],
                &[Some("hello"), Some("world"), None]
            ),
            "hello, wor, (null)"
        );
        assert_eq!(fmt("%c%c", &[b'o' as u64, b'k' as u64], &[], &[]), "ok");
        assert_eq!(fmt("%lld", &[u64::MAX], &[], &[]), "-1");
        assert_eq!(fmt("%*d", &[4, 9], &[], &[]), "   9");
    }
}
