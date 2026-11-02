//! Formatação de timecode SMPTE (`HH:MM:SS:FF`, ou `HH:MM:SS;FF` em drop-frame).

use crate::time::{FrameRate, Tick};

/// Formata o instante `t` como timecode na taxa `rate`.
///
/// Em 29,97 e 59,94 usa drop-frame: os números de quadro 0–1 (ou 0–3) são
/// pulados no início de cada minuto, exceto nos minutos múltiplos de 10, para
/// que o timecode acompanhe o relógio de parede.
pub fn format(t: Tick, rate: FrameRate) -> String {
    let frame = rate.frame_at(t);
    format_frame(frame, rate)
}

/// Formata o número de quadro `frame` como timecode.
pub fn format_frame(frame: i64, rate: FrameRate) -> String {
    let sign = if frame < 0 { "-" } else { "" };
    let mut n = frame.unsigned_abs();
    let base = rate.timecode_base().max(1) as u64;
    let drop_frame = rate.is_drop_frame();
    if drop_frame {
        let drop = base / 15; // 2 em 29,97; 4 em 59,94
        let per_10_min = base * 600 - drop * 9;
        let per_min = base * 60 - drop;
        let tens = n / per_10_min;
        let rem = n % per_10_min;
        n += drop * 9 * tens;
        if rem > drop {
            n += drop * ((rem - drop) / per_min);
        }
    }
    let ff = n % base;
    let total_seconds = n / base;
    let ss = total_seconds % 60;
    let mm = (total_seconds / 60) % 60;
    let hh = total_seconds / 3600;
    let sep = if drop_frame { ';' } else { ':' };
    let ff_width = if base > 99 { 3 } else { 2 };
    format!("{sign}{hh:02}:{mm:02}:{ss:02}{sep}{ff:0ff_width$}")
}

/// Lê um timecode (`HH:MM:SS:FF`, `HH:MM:SS;FF`, `MM:SS:FF`, `SS:FF`) ou
/// um número de segundos (`12,5` ou `12.5`). Devolve o início do quadro.
pub fn parse(s: &str, rate: FrameRate) -> Option<Tick> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if !s.contains([':', ';']) {
        let secs: f64 = s.replace(',', ".").parse().ok()?;
        return (secs >= 0.0).then(|| rate.snap_nearest(Tick::from_seconds_f64(secs)));
    }
    let parts: Vec<u64> = s
        .split([':', ';'])
        .map(|p| p.trim().parse().ok())
        .collect::<Option<_>>()?;
    let (hh, mm, ss, ff) = match parts[..] {
        [h, m, s, f] => (h, m, s, f),
        [m, s, f] => (0, m, s, f),
        [s, f] => (0, 0, s, f),
        _ => return None,
    };
    let base = rate.timecode_base().max(1) as u64;
    if mm >= 60 || ss >= 60 || ff >= base {
        return None;
    }
    let mut frame = ((hh * 60 + mm) * 60 + ss) * base + ff;
    if rate.is_drop_frame() {
        let drop = base / 15;
        let minutes = hh * 60 + mm;
        frame -= drop * (minutes - minutes / 10);
    }
    Some(rate.frame_start(frame as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leitura_de_timecode() {
        let r = FrameRate::FPS_29_97;
        for f in [0i64, 1, 1799, 1800, 17982, 107_892, 123_456] {
            let tc = format_frame(f, r);
            assert_eq!(parse(&tc, r), Some(r.frame_start(f)), "{tc}");
        }
        let r25 = FrameRate::FPS_25;
        assert_eq!(parse("00:00:02:10", r25), Some(r25.frame_start(60)));
        assert_eq!(parse("2:10", r25), Some(r25.frame_start(60)));
        assert_eq!(parse("2,4", r25), Some(r25.frame_start(60)));
        assert_eq!(parse("00:61:00:00", r25), None);
        assert_eq!(parse("abc", r25), None);
    }

    #[test]
    fn nao_drop_frame() {
        let r = FrameRate::FPS_25;
        assert_eq!(format_frame(0, r), "00:00:00:00");
        assert_eq!(format_frame(24, r), "00:00:00:24");
        assert_eq!(format_frame(25, r), "00:00:01:00");
        assert_eq!(format_frame(25 * 3600 + 25 * 61 + 3, r), "01:01:01:03");
        assert_eq!(format_frame(-25, r), "-00:00:01:00");
        assert_eq!(
            format(Tick::from_seconds(2), FrameRate::FPS_24),
            "00:00:02:00"
        );
    }

    #[test]
    fn drop_frame_29_97() {
        let r = FrameRate::FPS_29_97;
        assert_eq!(format_frame(1799, r), "00:00:59;29");
        // O quadro seguinte pula ;00 e ;01.
        assert_eq!(format_frame(1800, r), "00:01:00;02");
        // No décimo minuto nada é pulado.
        assert_eq!(format_frame(17982, r), "00:10:00;00");
        assert_eq!(format_frame(17981, r), "00:09:59;29");
        // Uma hora de relógio.
        assert_eq!(format_frame(107_892, r), "01:00:00;00");
    }

    #[test]
    fn drop_frame_59_94() {
        let r = FrameRate::FPS_59_94;
        assert_eq!(format_frame(3599, r), "00:00:59;59");
        assert_eq!(format_frame(3600, r), "00:01:00;04");
        assert_eq!(format_frame(215_784, r), "01:00:00;00");
    }
}
