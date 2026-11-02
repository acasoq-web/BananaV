//! Deflate (RFC 1951) e o envelope zlib (RFC 1950), escritos a partir das
//! especificações. Usados por PNG e por outros formatos com compressão zlib.

mod checksum;
mod deflate;
mod inflate;

pub use checksum::{Crc32, adler32, crc32};
pub use deflate::deflate;
pub use inflate::inflate;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Os dados acabaram antes do fim do bloco final.
    Truncated,
    /// Código ou bloco inválido.
    Corrupt(&'static str),
    /// Soma de verificação não confere.
    Checksum,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Truncated => f.write_str("dados comprimidos truncados"),
            Error::Corrupt(why) => write!(f, "dados comprimidos inválidos: {why}"),
            Error::Checksum => f.write_str("soma de verificação incorreta"),
        }
    }
}

impl std::error::Error for Error {}

/// Descomprime um fluxo zlib (cabeçalho de 2 bytes + deflate + Adler-32).
pub fn zlib_decompress(data: &[u8]) -> Result<Vec<u8>, Error> {
    if data.len() < 6 {
        return Err(Error::Truncated);
    }
    let (cmf, flg) = (data[0], data[1]);
    if cmf & 0x0F != 8 || !(((cmf as u16) << 8) | flg as u16).is_multiple_of(31) {
        return Err(Error::Corrupt("cabeçalho zlib"));
    }
    if flg & 0x20 != 0 {
        return Err(Error::Corrupt("dicionário zlib não suportado"));
    }
    let (out, used) = inflate::inflate_with_len(&data[2..])?;
    let tail = data.get(2 + used..2 + used + 4).ok_or(Error::Truncated)?;
    let expected = u32::from_be_bytes(tail.try_into().expect("4 bytes"));
    if adler32(&out) != expected {
        return Err(Error::Checksum);
    }
    Ok(out)
}

/// Comprime no formato zlib. `level` de 0 (só armazena) a 9 (mais lento).
pub fn zlib_compress(data: &[u8], level: u8) -> Vec<u8> {
    let mut out = vec![0x78, 0x9C];
    out.extend(deflate(data, level));
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_data() -> Vec<Vec<u8>> {
        let mut seed = 7u32;
        let mut rnd = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (seed >> 16) as u8
        };
        let random: Vec<u8> = (0..70_000).map(|_| rnd()).collect();
        let text = "O BananaV comprime e descomprime este texto repetido. "
            .repeat(500)
            .into_bytes();
        let runs: Vec<u8> = (0..100_000).map(|i| (i / 1000) as u8).collect();
        vec![Vec::new(), vec![42], random, text, runs, vec![0; 300_000]]
    }

    #[test]
    fn ida_e_volta_em_todos_os_niveis() {
        for data in sample_data() {
            for level in [0, 1, 6, 9] {
                let z = zlib_compress(&data, level);
                assert_eq!(
                    zlib_decompress(&z).unwrap(),
                    data,
                    "nível {level}, {} bytes",
                    data.len()
                );
            }
        }
    }

    #[test]
    fn comprime_dados_repetitivos() {
        let text = "abcabcabc".repeat(10_000).into_bytes();
        let z = zlib_compress(&text, 6);
        assert!(z.len() < text.len() / 20, "{} bytes", z.len());
    }

    #[test]
    fn checksums_conhecidos() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn detecta_corrupcao() {
        let mut z = zlib_compress(b"dados importantes dados importantes", 6);
        let n = z.len();
        z[n - 1] ^= 1;
        assert_eq!(zlib_decompress(&z), Err(Error::Checksum));
        assert!(zlib_decompress(&[0x78, 0x9C, 0xFF]).is_err());
    }

    /// Fluxos gerados por outra implementação (zlib), com blocos fixos e
    /// dinâmicos.
    #[test]
    fn le_fluxos_de_terceiros() {
        let fixed = [
            0x78, 0xda, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0xc8, 0xc0, 0x20, 0xcb, 0xf3, 0x8b,
            0x72, 0x52, 0x74, 0x14, 0x4a, 0x32, 0x32, 0x8b, 0x15, 0x80, 0xc8, 0x29, 0x31, 0x0f,
            0x08, 0xc3, 0x14, 0x33, 0x68, 0xaa, 0x1c, 0x00, 0x22, 0x60, 0x32, 0x71,
        ];
        let expected = b"hello hello hello hello world, this is BananaV!".repeat(3);
        assert_eq!(zlib_decompress(&fixed).unwrap(), expected);
        let words = include_bytes!("../testdata/palavras.zz");
        assert_eq!(
            zlib_decompress(words).unwrap(),
            include_bytes!("../testdata/palavras.txt")
        );
        let mixed = include_bytes!("../testdata/misto.zz");
        assert_eq!(
            zlib_decompress(mixed).unwrap(),
            include_bytes!("../testdata/misto.bin")
        );
    }
}
