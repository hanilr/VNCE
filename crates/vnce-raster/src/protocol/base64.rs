//! Standard base64 encoding (RFC 4648, with padding).

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Appends the base64 encoding of `data` to `out`.
pub fn encode_into(data: &[u8], out: &mut Vec<u8>) {
    out.reserve(data.len().div_ceil(3) * 4);
    let symbol = |index: u32| ALPHABET[(index & 0x3f) as usize];
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(symbol(n >> 18));
        out.push(symbol(n >> 12));
        out.push(if chunk.len() > 1 {
            symbol(n >> 6)
        } else {
            b'='
        });
        out.push(if chunk.len() > 2 { symbol(n) } else { b'=' });
    }
}

/// The base64 encoding of `data`.
#[must_use]
pub fn encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    encode_into(data, &mut out);
    out
}

#[cfg(test)]
#[allow(
    clippy::cast_possible_truncation,
    reason = "test-only decoder; values fit by construction"
)]
pub(crate) fn decode(text: &[u8]) -> Vec<u8> {
    let value = |c: u8| ALPHABET.iter().position(|&a| a == c).map(|p| p as u32);
    let mut out = Vec::new();
    for chunk in text.chunks(4) {
        let mut n = 0u32;
        let mut count: usize = 0;
        for &c in chunk {
            if let Some(v) = value(c) {
                n = (n << 6) | v;
                count += 1;
            }
        }
        n <<= 6 * (4 - count);
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..count.saturating_sub(1)]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc_4648_vectors() {
        for (input, output) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode(input.as_bytes()), output.as_bytes());
            assert_eq!(decode(output.as_bytes()), input.as_bytes());
        }
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(decode(&encode(&all)), all);
    }
}
