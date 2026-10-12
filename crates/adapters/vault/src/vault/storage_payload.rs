//! Bounded lossless framing inside the encrypted Vault. Canonical plain bytes
//! remain the authority for evidence digests and logical read budgets.
use flate2::{Compression, Decompress, FlushDecompress, Status, write::ZlibEncoder};
use floe_agent_contract::AgentFailure;
use std::io::Write;

pub(super) const PAYLOAD_HEADER_BYTES: usize = 9;
const MAGIC: &[u8; 4] = b"FLC1";
const RAW: u8 = 0;
const ZLIB: u8 = 1;
const MIN_COMPRESSION_SAVING: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct StoragePayloadHeader {
    codec: u8,
    pub(super) logical_bytes: usize,
}

pub(super) fn inspect_payload_header(
    prefix: &[u8],
    stored_bytes: usize,
    maximum_logical_bytes: usize,
) -> Result<StoragePayloadHeader, AgentFailure> {
    let invalid = AgentFailure::VaultUnavailable;
    if prefix.len() != PAYLOAD_HEADER_BYTES || &prefix[..4] != MAGIC {
        return Err(invalid);
    }
    let logical_bytes = u32::from_be_bytes(prefix[5..9].try_into().map_err(|_| invalid)?) as usize;
    let body_bytes = stored_bytes
        .checked_sub(PAYLOAD_HEADER_BYTES)
        .ok_or(invalid)?;
    if logical_bytes == 0
        || logical_bytes > maximum_logical_bytes
        || body_bytes == 0
        || stored_bytes
            > maximum_logical_bytes
                .checked_add(PAYLOAD_HEADER_BYTES)
                .ok_or(invalid)?
    {
        return Err(invalid);
    }
    match prefix[4] {
        RAW if body_bytes == logical_bytes => {}
        ZLIB if body_bytes < logical_bytes => {}
        _ => return Err(invalid),
    }
    Ok(StoragePayloadHeader {
        codec: prefix[4],
        logical_bytes,
    })
}

pub(super) fn encode_payload(
    plaintext: &[u8],
    maximum_logical_bytes: usize,
) -> Result<Vec<u8>, AgentFailure> {
    if plaintext.is_empty() {
        return Err(AgentFailure::InvalidInput);
    }
    if plaintext.len() > maximum_logical_bytes {
        return Err(AgentFailure::BudgetExceeded);
    }
    let length = u32::try_from(plaintext.len()).map_err(|_| AgentFailure::BudgetExceeded)?;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(plaintext)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let compressed = encoder
        .finish()
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let compressed_is_smaller = compressed
        .len()
        .checked_add(MIN_COMPRESSION_SAVING)
        .is_some_and(|size| size < plaintext.len());
    let (codec, body) = if compressed_is_smaller {
        (ZLIB, compressed.as_slice())
    } else {
        (RAW, plaintext)
    };
    let mut encoded = Vec::with_capacity(PAYLOAD_HEADER_BYTES + body.len());
    encoded.extend_from_slice(MAGIC);
    encoded.push(codec);
    encoded.extend_from_slice(&length.to_be_bytes());
    encoded.extend_from_slice(body);
    Ok(encoded)
}

pub(super) fn decode_payload(
    encoded: &[u8],
    expected_logical_bytes: usize,
    maximum_logical_bytes: usize,
) -> Result<Vec<u8>, AgentFailure> {
    let invalid = AgentFailure::VaultUnavailable;
    let header = inspect_payload_header(
        encoded.get(..PAYLOAD_HEADER_BYTES).ok_or(invalid)?,
        encoded.len(),
        maximum_logical_bytes,
    )?;
    if header.logical_bytes != expected_logical_bytes {
        return Err(invalid);
    }
    let body = &encoded[PAYLOAD_HEADER_BYTES..];
    if header.codec == RAW {
        return Ok(body.to_vec());
    }
    // One bounded allocation with a sentinel, never an unbounded decompression sink.
    let mut output = vec![0; header.logical_bytes.checked_add(1).ok_or(invalid)?];
    let mut decoder = Decompress::new(true);
    let status = decoder
        .decompress(body, &mut output, FlushDecompress::Finish)
        .map_err(|_| invalid)?;
    if status != Status::StreamEnd
        || decoder.total_in() != body.len() as u64
        || decoder.total_out() != header.logical_bytes as u64
    {
        return Err(invalid);
    }
    output.truncate(header.logical_bytes);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_and_unicode_compressed_payloads_restore_exact_canonical_bytes() {
        for plain in [
            b"{\"id\":1}".to_vec(),
            "{\"message\":\"보존할 원본 대화 🌿\"}\n"
                .repeat(128)
                .into_bytes(),
        ] {
            let encoded = encode_payload(&plain, plain.len()).unwrap();
            assert_eq!(
                decode_payload(&encoded, plain.len(), plain.len()).unwrap(),
                plain
            );
            if plain.len() > 100 {
                assert_eq!(encoded[4], ZLIB);
                assert!(encoded.len() < plain.len() / 4);
            } else {
                assert_eq!(encoded[4], RAW);
            }
        }
    }

    #[test]
    fn incompressible_data_uses_the_same_raw_frame_and_empty_data_is_invalid() {
        let mut seed = 0x12345678u32;
        let plain: Vec<u8> = (0..8192)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed as u8
            })
            .collect();
        let encoded = encode_payload(&plain, plain.len()).unwrap();
        assert_eq!(encoded[4], RAW);
        assert_eq!(encoded.len(), plain.len() + PAYLOAD_HEADER_BYTES);
        assert_eq!(
            decode_payload(&encoded, plain.len(), plain.len()).unwrap(),
            plain
        );
        assert_eq!(encode_payload(&[], 100), Err(AgentFailure::InvalidInput));
        assert_eq!(
            encode_payload(&[1, 2], 1),
            Err(AgentFailure::BudgetExceeded)
        );
    }

    #[test]
    fn metadata_and_unknown_frames_are_rejected_before_body_decode() {
        let plain = vec![42; 8192];
        let encoded = encode_payload(&plain, plain.len()).unwrap();
        assert!(decode_payload(&encoded, plain.len() - 1, plain.len()).is_err());
        assert!(decode_payload(&encoded, plain.len(), plain.len() - 1).is_err());
        for (index, byte) in [(0, b'X'), (4, 2)] {
            let mut wrong = encoded.clone();
            wrong[index] = byte;
            assert!(decode_payload(&wrong, plain.len(), plain.len()).is_err());
        }
        assert!(decode_payload(&encoded[..8], plain.len(), plain.len()).is_err());
        let raw = encode_payload(b"a", 1).unwrap();
        assert!(inspect_payload_header(&raw[..9], 8, 1).is_err());
    }

    #[test]
    fn truncated_trailing_and_checksum_corrupt_streams_are_rejected() {
        let plain = vec![42; 8192];
        let encoded = encode_payload(&plain, plain.len()).unwrap();
        let mut trailing = encoded.clone();
        trailing.push(0);
        let mut corrupted = encoded.clone();
        *corrupted.last_mut().unwrap() ^= 0xff;
        for invalid in [encoded[..encoded.len() - 1].to_vec(), trailing, corrupted] {
            assert!(decode_payload(&invalid, plain.len(), plain.len()).is_err());
        }
    }

    #[test]
    fn false_length_cannot_expand_beyond_the_declared_bounded_buffer() {
        let plain = vec![42; 8192];
        let mut encoded = encode_payload(&plain, plain.len()).unwrap();
        encoded[5..9].copy_from_slice(&128u32.to_be_bytes());
        assert!(inspect_payload_header(&encoded[..9], encoded.len(), plain.len()).is_ok());
        assert!(decode_payload(&encoded, 128, plain.len()).is_err());
    }
}
