//! Base64 (RFC 4648's standard alphabet, padded): bytes kept as text in
//! the project files, as a VST 3 plug-in's state.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `bytes` as Base64.
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The bytes of Base64 `text` (white space ignored); `None` when it is
/// not Base64.
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let digits: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let digits = match digits.iter().position(|&b| b == b'=') {
        Some(p) if digits[p..].iter().all(|&b| b == b'=') && digits[p..].len() <= 2 => &digits[..p],
        Some(_) => return None,
        None => &digits[..],
    };
    if digits.len() % 4 == 1 {
        return None;
    }
    let value = |b: u8| ALPHABET.iter().position(|&a| a == b).map(|v| v as u32);
    let mut out = Vec::with_capacity(digits.len() * 3 / 4);
    for chunk in digits.chunks(4) {
        let mut n = 0u32;
        for (k, &d) in chunk.iter().enumerate() {
            n |= value(d)? << (18 - 6 * k);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

/// Bytes kept as Base64 text (`#[serde(with = "bv_core::base64::text")]`).
#[cfg(feature = "serde")]
pub mod text {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(d)?;
        super::decode(&text).ok_or_else(|| serde::de::Error::custom("not Base64"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc_4648s_examples_go_and_come_back() {
        for (plain, coded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode(plain.as_bytes()), coded);
            assert_eq!(decode(coded).as_deref(), Some(plain.as_bytes()));
        }
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(decode(&encode(&all)), Some(all));
        assert_eq!(decode("Zm9v\nYmFy"), Some(b"foobar".to_vec()));
        assert_eq!(decode("Zm9*"), None);
        assert_eq!(decode("Z=m9"), None);
    }
}
