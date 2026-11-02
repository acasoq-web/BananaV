//! Reamostragem linear em fluxo contínuo (taxa do projeto → taxa do dispositivo).
//!
//! Suficiente para monitorar. A exportação nunca passa por aqui: ela usa a
//! taxa do projeto diretamente.

pub(crate) struct LinearResampler {
    /// Amostras de entrada avançadas por amostra de saída.
    step: f64,
    /// Posição da próxima saída num vetor virtual em que o índice 0 é a última
    /// amostra do bloco anterior e o índice 1 é a primeira do bloco atual.
    pos: f64,
    prev: Vec<f32>,
}

impl LinearResampler {
    #[cfg(test)]
    pub(crate) fn new(in_rate: u32, out_rate: u32, channels: usize) -> LinearResampler {
        LinearResampler::with_step(in_rate as f64 / out_rate as f64, channels)
    }

    /// `step` = amostras de entrada por amostra de saída.
    pub(crate) fn with_step(step: f64, channels: usize) -> LinearResampler {
        LinearResampler {
            step,
            pos: 1.0,
            prev: vec![0.0; channels],
        }
    }

    pub(crate) fn process(&mut self, input: &[Vec<f32>]) -> Vec<Vec<f32>> {
        let n = input.first().map_or(0, Vec::len);
        let mut out: Vec<Vec<f32>> = vec![Vec::new(); input.len()];
        if n == 0 {
            return out;
        }
        while self.pos < n as f64 {
            let i = self.pos.floor() as usize;
            let f = (self.pos - i as f64) as f32;
            for (c, ch) in input.iter().enumerate() {
                let a = if i == 0 { self.prev[c] } else { ch[i - 1] };
                let b = ch[i];
                out[c].push(a + (b - a) * f);
            }
            self.pos += self.step;
        }
        self.pos -= n as f64;
        for (c, ch) in input.iter().enumerate() {
            self.prev[c] = ch[n - 1];
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesma_taxa_e_identidade_com_uma_amostra_de_atraso() {
        let mut r = LinearResampler::new(48000, 48000, 1);
        let a = r.process(&[vec![1.0, 2.0, 3.0]]);
        let b = r.process(&[vec![4.0, 5.0]]);
        let all: Vec<f32> = a[0].iter().chain(&b[0]).copied().collect();
        assert_eq!(all, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn conta_de_amostras_bate_com_a_razao() {
        let mut r = LinearResampler::new(48000, 44100, 2);
        let mut total = 0;
        for _ in 0..100 {
            total += r.process(&[vec![0.0; 480], vec![0.0; 480]])[0].len();
        }
        // 48 000 de entrada → 44 100 de saída (±1).
        assert!((total as i64 - 44100).abs() <= 1, "{total}");
    }

    #[test]
    fn rampa_continua_entre_blocos() {
        let mut r = LinearResampler::new(1, 2, 1);
        let a = r.process(&[vec![0.0, 1.0]]);
        let b = r.process(&[vec![2.0, 3.0]]);
        let all: Vec<f32> = a[0].iter().chain(&b[0]).copied().collect();
        assert_eq!(all, vec![0.0, 0.5, 1.0, 1.5, 2.0, 2.5]);
    }
}
