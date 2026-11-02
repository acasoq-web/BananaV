//! Curvas de transferência: valor codificado (V′, 0–1) ↔ luz linear (L).

use std::sync::OnceLock;

use bv_core::TransferCharacteristic;

/// A table per transfer curve, built the first time it is used.
fn shared<T>(
    tables: &'static [OnceLock<T>; 5],
    tc: TransferCharacteristic,
    make: impl FnOnce() -> T,
) -> &'static T {
    let i = match tc {
        TransferCharacteristic::Bt709 => 0,
        TransferCharacteristic::Srgb => 1,
        TransferCharacteristic::Linear => 2,
        TransferCharacteristic::Pq => 3,
        TransferCharacteristic::Hlg => 4,
    };
    tables[i].get_or_init(make)
}

/// BT.709 / BT.601 / BT.2020: V′ → L (inversa da OETF).
pub fn bt709_to_linear(v: f32) -> f32 {
    if v < 0.081 {
        v / 4.5
    } else {
        ((v + 0.099) / 1.099).powf(1.0 / 0.45)
    }
}

/// BT.709: L → V′ (OETF).
pub fn linear_to_bt709(l: f32) -> f32 {
    if l < 0.018 {
        4.5 * l
    } else {
        1.099 * l.powf(0.45) - 0.099
    }
}

/// sRGB (IEC 61966-2-1): V′ → L.
pub fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB: L → V′.
pub fn linear_to_srgb(l: f32) -> f32 {
    if l <= 0.003_130_8 {
        12.92 * l
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

// SMPTE ST 2084 (PQ). L em unidades onde 1,0 = 100 cd/m² (branco de referência SDR).
const PQ_M1: f32 = 2610.0 / 16384.0;
const PQ_M2: f32 = 2523.0 / 4096.0 * 128.0;
const PQ_C1: f32 = 3424.0 / 4096.0;
const PQ_C2: f32 = 2413.0 / 4096.0 * 32.0;
const PQ_C3: f32 = 2392.0 / 4096.0 * 32.0;
const PQ_PEAK_OVER_SDR_WHITE: f32 = 10000.0 / 100.0;

pub fn pq_to_linear(v: f32) -> f32 {
    let vp = v.max(0.0).powf(1.0 / PQ_M2);
    let n = (vp - PQ_C1).max(0.0);
    let d = PQ_C2 - PQ_C3 * vp;
    (n / d).powf(1.0 / PQ_M1) * PQ_PEAK_OVER_SDR_WHITE
}

pub fn linear_to_pq(l: f32) -> f32 {
    let y = (l / PQ_PEAK_OVER_SDR_WHITE).max(0.0).powf(PQ_M1);
    ((PQ_C1 + PQ_C2 * y) / (1.0 + PQ_C3 * y)).powf(PQ_M2)
}

// ARIB STD-B67 (HLG), OETF de cena; 1,0 linear = branco de referência (75% HLG).
const HLG_A: f32 = 0.178_832_77;
const HLG_B: f32 = 0.284_668_92;
const HLG_C: f32 = 0.559_910_7;
/// Valor linear de cena (0–1) correspondente ao branco de referência HLG (V′ = 0,75).
const HLG_REF_WHITE: f32 = 0.264_962_5;

pub fn hlg_to_linear(v: f32) -> f32 {
    let e = if v <= 0.5 {
        v * v / 3.0
    } else {
        (((v - HLG_C) / HLG_A).exp() + HLG_B) / 12.0
    };
    e / HLG_REF_WHITE
}

pub fn linear_to_hlg(l: f32) -> f32 {
    let e = (l * HLG_REF_WHITE).max(0.0);
    if e <= 1.0 / 12.0 {
        (3.0 * e).sqrt()
    } else {
        HLG_A * (12.0 * e - HLG_B).ln() + HLG_C
    }
}

pub fn to_linear(tc: TransferCharacteristic, v: f32) -> f32 {
    match tc {
        TransferCharacteristic::Bt709 => bt709_to_linear(v),
        TransferCharacteristic::Srgb => srgb_to_linear(v),
        TransferCharacteristic::Linear => v,
        TransferCharacteristic::Pq => pq_to_linear(v),
        TransferCharacteristic::Hlg => hlg_to_linear(v),
    }
}

pub fn from_linear(tc: TransferCharacteristic, l: f32) -> f32 {
    match tc {
        TransferCharacteristic::Bt709 => linear_to_bt709(l),
        TransferCharacteristic::Srgb => linear_to_srgb(l),
        TransferCharacteristic::Linear => l,
        TransferCharacteristic::Pq => linear_to_pq(l),
        TransferCharacteristic::Hlg => linear_to_hlg(l),
    }
}

/// Tabela para decodificar V′ ∈ [0, 1] com interpolação linear. Avaliar
/// `powf` três vezes por pixel custa caro; a tabela erra menos que 1e-5.
#[derive(Clone, Debug)]
pub struct DecodeLut {
    table: Vec<f32>,
}

impl DecodeLut {
    /// Intervals of the table (it has one more entry).
    pub const SIZE: usize = 4096;

    pub fn new(tc: TransferCharacteristic) -> DecodeLut {
        let table = (0..=Self::SIZE)
            .map(|i| to_linear(tc, i as f32 / Self::SIZE as f32))
            .collect();
        DecodeLut { table }
    }

    /// The table of `tc`, built once and shared.
    pub fn shared(tc: TransferCharacteristic) -> &'static DecodeLut {
        static TABLES: [OnceLock<DecodeLut>; 5] = [const { OnceLock::new() }; 5];
        shared(&TABLES, tc, || DecodeLut::new(tc))
    }

    /// L at V′ = i / SIZE (for the graphics card, which interpolates the
    /// same way).
    pub fn table(&self) -> &[f32] {
        &self.table
    }

    #[inline]
    pub fn eval(&self, v: f32) -> f32 {
        // Valores fora de [0, 1] (super-brancos, sub-pretos) são limitados.
        let x = v.clamp(0.0, 1.0) * Self::SIZE as f32;
        let i = (x as usize).min(Self::SIZE - 1);
        let f = x - i as f32;
        self.table[i] + (self.table[i + 1] - self.table[i]) * f
    }
}

/// Tabela inversa: L → V′ para L ∈ [0, `max`].
#[derive(Clone, Debug)]
pub struct EncodeLut {
    table: Vec<f32>,
    max: f32,
}

impl EncodeLut {
    /// Intervals of the table (it has one more entry).
    pub const SIZE: usize = 8192;

    pub fn new(tc: TransferCharacteristic) -> EncodeLut {
        let max = match tc {
            TransferCharacteristic::Pq => PQ_PEAK_OVER_SDR_WHITE,
            TransferCharacteristic::Hlg => 1.0 / HLG_REF_WHITE,
            _ => 1.0,
        };
        // Amostragem em raiz quadrada: mais pontos perto do preto, onde as
        // curvas são mais íngremes.
        let table = (0..=Self::SIZE)
            .map(|i| {
                let u = i as f32 / Self::SIZE as f32;
                from_linear(tc, u * u * max)
            })
            .collect();
        EncodeLut { table, max }
    }

    /// The table of `tc`, built once and shared.
    pub fn shared(tc: TransferCharacteristic) -> &'static EncodeLut {
        static TABLES: [OnceLock<EncodeLut>; 5] = [const { OnceLock::new() }; 5];
        shared(&TABLES, tc, || EncodeLut::new(tc))
    }

    /// V′ at L = (i / SIZE)² · max (for the graphics card).
    pub fn table(&self) -> &[f32] {
        &self.table
    }

    /// The largest light the table covers.
    pub fn max(&self) -> f32 {
        self.max
    }

    #[inline]
    pub fn eval(&self, l: f32) -> f32 {
        let u = (l.clamp(0.0, self.max) / self.max).sqrt();
        let x = u * Self::SIZE as f32;
        let i = (x as usize).min(Self::SIZE - 1);
        let f = x - i as f32;
        self.table[i] + (self.table[i + 1] - self.table[i]) * f
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [TransferCharacteristic; 5] = [
        TransferCharacteristic::Bt709,
        TransferCharacteristic::Srgb,
        TransferCharacteristic::Linear,
        TransferCharacteristic::Pq,
        TransferCharacteristic::Hlg,
    ];

    #[test]
    fn curvas_sao_inversas() {
        for tc in ALL {
            for i in 0..=100 {
                let v = i as f32 / 100.0;
                let back = from_linear(tc, to_linear(tc, v));
                assert!((back - v).abs() < 1e-4, "{tc:?} v={v} → {back}");
            }
        }
    }

    #[test]
    fn pontos_de_referencia() {
        assert!((srgb_to_linear(0.5) - 0.214_041).abs() < 1e-5);
        assert!((linear_to_bt709(0.018) - 0.081).abs() < 1e-3);
        // PQ: 100 cd/m² ≈ V′ 0,508.
        assert!((linear_to_pq(1.0) - 0.508).abs() < 1e-3);
        // HLG: branco de referência em 75%.
        assert!((linear_to_hlg(1.0) - 0.75).abs() < 1e-3);
        assert!((pq_to_linear(1.0) - 100.0).abs() < 1e-2);
    }

    #[test]
    fn tabelas_sao_precisas() {
        for tc in ALL {
            let dec = DecodeLut::new(tc);
            let enc = EncodeLut::new(tc);
            for i in 0..=1000 {
                let v = i as f32 / 1000.0;
                let exact = to_linear(tc, v);
                let tol = 1e-4 * exact.abs().max(1.0);
                assert!((dec.eval(v) - exact).abs() < tol, "{tc:?} dec {v}");
                let back = enc.eval(exact);
                assert!((back - v).abs() < 2e-3, "{tc:?} enc {v}: {back}");
            }
        }
    }
}
