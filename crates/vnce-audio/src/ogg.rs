//! OGG Vorbis decoding with `lewton`.

use std::io::Cursor;

use lewton::inside_ogg::OggStreamReader;

use crate::sound::{DecodeError, Sound};

/// Decodes a whole OGG Vorbis stream.
pub(crate) fn decode(bytes: &[u8]) -> Result<Sound, DecodeError> {
    let invalid = |error: lewton::VorbisError| DecodeError::Invalid(error.to_string());
    let mut reader = OggStreamReader::new(Cursor::new(bytes)).map_err(invalid)?;
    let channels = u16::from(reader.ident_hdr.audio_channels);
    let sample_rate = reader.ident_hdr.audio_sample_rate;
    if channels == 0 || sample_rate == 0 {
        return Err(DecodeError::Invalid(
            "zero channels or sample rate".to_owned(),
        ));
    }
    let mut samples = Vec::new();
    // A broken packet ends the sound; everything decoded before it is kept.
    while let Ok(Some(packet)) = reader.read_dec_packet_itl() {
        samples.extend(packet.iter().map(|&s| f32::from(s) / 32_768.0));
    }
    Ok(Sound::from_samples(channels, sample_rate, samples))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broken_streams_are_errors() {
        assert!(matches!(decode(b"OggS"), Err(DecodeError::Invalid(_))));
        let mut page = b"OggS\0\x02".to_vec();
        page.extend_from_slice(&[0; 40]);
        assert!(decode(&page).is_err());
        assert!(matches!(
            Sound::decode(b"OggS broken"),
            Err(DecodeError::Invalid(_))
        ));
    }
}
