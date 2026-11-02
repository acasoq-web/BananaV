//! Leitura e escrita de bits (MSB primeiro, como em H.264/HEVC/AAC) e de bytes
//! big-endian (como em ISOBMFF e Matroska).

mod bits;
mod bytes;
mod emulation;
pub mod isobmff;

pub use bits::{BitReader, BitWriter};
pub use bytes::{ByteReader, ByteWriter};
pub use emulation::{escape_rbsp, unescape_rbsp};

/// Fim inesperado dos dados, ou valor fora do permitido pelo formato.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Tentou ler além do fim do buffer.
    Eof,
    /// Código Exp-Golomb com mais de 31 zeros iniciais.
    InvalidExpGolomb,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Eof => f.write_str("fim inesperado dos dados"),
            Error::InvalidExpGolomb => f.write_str("código Exp-Golomb inválido"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
