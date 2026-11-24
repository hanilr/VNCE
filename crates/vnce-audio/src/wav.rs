//! WAV (RIFF) decoding and encoding.

use crate::sound::{DecodeError, Sound};

fn invalid(message: &str) -> DecodeError {
    DecodeError::Invalid(message.to_owned())
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// The `fmt ` chunk fields that matter.
struct Format {
    float: bool,
    channels: u16,
    sample_rate: u32,
    bits: u16,
}

/// Decodes PCM (8, 16, 24 or 32-bit integer) and IEEE float (32-bit) WAV files, plain or
/// `WAVE_FORMAT_EXTENSIBLE`. A truncated data chunk keeps its complete frames.
pub(crate) fn decode(bytes: &[u8]) -> Result<Sound, DecodeError> {
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(invalid("missing the RIFF / WAVE header"));
    }
    let mut format = None;
    let mut data = None;
    let mut at = 12_usize;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32_at(bytes, at + 4).ok_or_else(|| invalid("chunk header"))? as usize;
        let body_start = at + 8;
        let body_end = body_start.saturating_add(size).min(bytes.len());
        let body = &bytes[body_start..body_end];
        match id {
            b"fmt " => format = Some(parse_format(body)?),
            b"data" => data = Some(body),
            _ => {}
        }
        // Chunks are padded to an even size.
        at = body_start.saturating_add(size).saturating_add(size & 1);
    }
    let format = format.ok_or_else(|| invalid("no `fmt ` chunk"))?;
    let data = data.ok_or_else(|| invalid("no `data` chunk"))?;
    if format.channels == 0 || format.sample_rate == 0 {
        return Err(invalid("zero channels or sample rate"));
    }
    let samples: Vec<f32> = match (format.float, format.bits) {
        (false, 8) => data
            .iter()
            .map(|&b| (f32::from(b) - 128.0) / 128.0)
            .collect(),
        (false, 16) => data
            .chunks_exact(2)
            .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])) / 32_768.0)
            .collect(),
        (false, 24) => data
            .chunks_exact(3)
            .map(|c| {
                let value = i32::from_le_bytes([0, c[0], c[1], c[2]]) >> 8;
                #[allow(clippy::cast_precision_loss, reason = "24-bit values are exact in f32")]
                let value = value as f32;
                value / 8_388_608.0
            })
            .collect(),
        (false, 32) => data
            .chunks_exact(4)
            .map(|c| {
                let value = i32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                #[allow(
                    clippy::cast_precision_loss,
                    reason = "a sample needs no more precision"
                )]
                let value = value as f32;
                value / 2_147_483_648.0
            })
            .collect(),
        (true, 32) => data
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
        (float, bits) => {
            let kind = if float { "float" } else { "integer" };
            return Err(DecodeError::Unsupported(format!("{bits}-bit {kind} WAV")));
        }
    };
    Ok(Sound::from_samples(
        format.channels,
        format.sample_rate,
        samples,
    ))
}

fn parse_format(body: &[u8]) -> Result<Format, DecodeError> {
    let field = |at| u16_at(body, at).ok_or_else(|| invalid("short `fmt ` chunk"));
    let mut tag = field(0)?;
    let channels = field(2)?;
    let sample_rate = u32_at(body, 4).ok_or_else(|| invalid("short `fmt ` chunk"))?;
    let bits = field(14)?;
    if tag == 0xFFFE {
        // WAVE_FORMAT_EXTENSIBLE: the real format is the start of the sub-format GUID.
        tag = u16_at(body, 24).ok_or_else(|| invalid("short extensible `fmt ` chunk"))?;
    }
    let float = match tag {
        1 => false,
        3 => true,
        other => {
            return Err(DecodeError::Unsupported(format!(
                "WAV format {other:#06x} (only PCM and float)"
            )));
        }
    };
    Ok(Format {
        float,
        channels,
        sample_rate,
        bits,
    })
}

/// Encodes a sound as a 16-bit PCM WAV file.
pub(crate) fn encode(sound: &Sound) -> Vec<u8> {
    let data_size = u32::try_from(sound.samples().len() * 2).unwrap_or(u32::MAX);
    let channels = sound.channels();
    let rate = sound.sample_rate();
    let mut out = Vec::with_capacity(44 + data_size as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36_u32.saturating_add(data_size)).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    out.extend_from_slice(&(channels * 2).to_le_bytes());
    out.extend_from_slice(&16_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_size.to_le_bytes());
    for &sample in sound.samples() {
        #[allow(clippy::cast_possible_truncation, reason = "clamped to the i16 range")]
        let value = (sample * 32_767.0).round().clamp(-32_768.0, 32_767.0) as i16;
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WAV file with a `fmt ` chunk of `tag`, `bits` and the raw `data`.
    fn wav(tag: u16, channels: u16, bits: u16, data: &[u8]) -> Vec<u8> {
        let mut out = b"RIFF\0\0\0\0WAVE".to_vec();
        // An unknown chunk with an odd size (padded) before the format.
        out.extend_from_slice(b"LIST\x03\0\0\0abc\0");
        out.extend_from_slice(b"fmt \x10\0\0\0");
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&8000_u32.to_le_bytes());
        out.extend_from_slice(&[0; 6]);
        out.extend_from_slice(&bits.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn every_supported_sample_format() {
        let cases: [(u16, u16, Vec<u8>, [f32; 2]); 5] = [
            (1, 8, vec![0, 192], [-1.0, 0.5]),
            (
                1,
                16,
                [-16_384_i16, 32_767]
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect(),
                [-0.5, 0.999_97],
            ),
            (1, 24, vec![0, 0, 0x40, 0, 0, 0xC0], [0.5, -0.5]),
            (
                1,
                32,
                [i32::MIN, 0].iter().flat_map(|v| v.to_le_bytes()).collect(),
                [-1.0, 0.0],
            ),
            (
                3,
                32,
                [0.25_f32, -0.75]
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect(),
                [0.25, -0.75],
            ),
        ];
        for (tag, bits, data, expected) in cases {
            let sound = decode(&wav(tag, 1, bits, &data)).unwrap();
            assert_eq!(sound.sample_rate(), 8000);
            for (actual, wanted) in sound.samples().iter().zip(expected) {
                assert!(
                    (actual - wanted).abs() < 1e-4,
                    "{bits}-bit: {actual} != {wanted}"
                );
            }
        }
    }

    #[test]
    fn extensible_stereo_and_truncated_data() {
        let mut file = wav(0xFFFE, 2, 16, &[0, 0x40, 0, 0xC0, 1]);
        // Extend the fmt chunk to 40 bytes with the PCM sub-format GUID.
        let fmt = file.windows(4).position(|w| w == b"fmt ").unwrap();
        file[fmt + 4] = 40;
        let mut extension = vec![22, 0, 16, 0, 3, 0, 0, 0, 1, 0];
        extension.extend_from_slice(&[0; 14]);
        file.splice(fmt + 24..fmt + 24, extension);
        let sound = decode(&file).unwrap();
        assert_eq!((sound.channels(), sound.frames()), (2, 1));
        assert_eq!(sound.samples(), [0.5, -0.5]);
    }

    #[test]
    fn unsupported_and_broken_files() {
        assert!(matches!(
            decode(&wav(2, 1, 4, &[0])),
            Err(DecodeError::Unsupported(_))
        ));
        assert!(matches!(
            decode(&wav(1, 1, 12, &[0])),
            Err(DecodeError::Unsupported(_))
        ));
        assert!(matches!(
            decode(b"RIFF\0\0\0\0WAVE"),
            Err(DecodeError::Invalid(_))
        ));
        assert!(matches!(decode(b"RIFX"), Err(DecodeError::Invalid(_))));
        assert!(matches!(
            decode(&wav(1, 0, 16, &[])),
            Err(DecodeError::Invalid(_))
        ));
    }

    #[test]
    fn encoding_round_trips() {
        let sound = Sound::from_samples(2, 22_050, vec![0.5, -0.5, 1.0, -1.0, 0.0, 0.25]);
        let bytes = sound.to_wav();
        assert_eq!(bytes.len(), 44 + 12);
        let back = decode(&bytes).unwrap();
        assert_eq!(
            (back.channels(), back.sample_rate(), back.frames()),
            (2, 22_050, 3)
        );
        for (a, b) in back.samples().iter().zip(sound.samples()) {
            assert!((a - b).abs() < 1e-4);
        }
    }
}
