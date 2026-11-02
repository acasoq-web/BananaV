//! Buffers de áudio decodificado: float de 32 bits, planar.

use crate::time::SampleRate;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ChannelLayout {
    Mono,
    Stereo,
    /// L, R, C, LFE, Ls, Rs.
    Surround51,
    /// Outro número de canais, sem posições definidas.
    Discrete(u16),
}

impl ChannelLayout {
    pub fn channel_count(self) -> usize {
        match self {
            ChannelLayout::Mono => 1,
            ChannelLayout::Stereo => 2,
            ChannelLayout::Surround51 => 6,
            ChannelLayout::Discrete(n) => n as usize,
        }
    }

    pub fn from_count(n: usize) -> ChannelLayout {
        match n {
            1 => ChannelLayout::Mono,
            2 => ChannelLayout::Stereo,
            6 => ChannelLayout::Surround51,
            n => ChannelLayout::Discrete(n as u16),
        }
    }
}

/// Áudio planar: `channels[c][i]` é a amostra `i` do canal `c`, em [-1, 1].
#[derive(Clone, Debug, PartialEq)]
pub struct AudioBuffer {
    pub sample_rate: SampleRate,
    pub layout: ChannelLayout,
    pub channels: Vec<Vec<f32>>,
}

impl AudioBuffer {
    pub fn silent(sample_rate: SampleRate, layout: ChannelLayout, frames: usize) -> AudioBuffer {
        AudioBuffer {
            sample_rate,
            layout,
            channels: vec![vec![0.0; frames]; layout.channel_count()],
        }
    }

    /// Número de amostras por canal.
    pub fn frames(&self) -> usize {
        self.channels.first().map_or(0, Vec::len)
    }

    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Intercala os canais (`L R L R …`), como esperam saídas de áudio e encoders.
    pub fn interleaved(&self) -> Vec<f32> {
        let n = self.frames();
        let c = self.channel_count();
        let mut out = Vec::with_capacity(n * c);
        for i in 0..n {
            for ch in &self.channels {
                out.push(ch[i]);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intercalacao() {
        let mut b = AudioBuffer::silent(SampleRate::HZ_48000, ChannelLayout::Stereo, 2);
        b.channels[0] = vec![1.0, 2.0];
        b.channels[1] = vec![-1.0, -2.0];
        assert_eq!(b.interleaved(), vec![1.0, -1.0, 2.0, -2.0]);
        assert_eq!(b.frames(), 2);
    }
}
