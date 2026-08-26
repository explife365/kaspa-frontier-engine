//! Igra Galleon iKAS is minted by Igra when a valid L1 Entry is accepted.
//! This crate cannot print iKAS. Official faucet: https://faucet.igralabs.com

use crate::error::{EngineError, Result};
use crate::network::{GALLEON_ENTRY_MIN_SOMPI, GALLEON_TXID_PREFIX};

/// `0x9` version + `0x2` Entry. See Igra transaction protocol.
pub const ENTRY_PREFIX_BYTE: u8 = 0x92;
pub const ENTRY_PAYLOAD_LEN: usize = 33;

/// 33-byte L1 payload: `[0x92][20-byte L2 addr][u64 LE sompi][u32 BE nonce]`.
pub fn entry_payload(l2: [u8; 20], amount_sompi: u64, nonce_be: u32) -> [u8; ENTRY_PAYLOAD_LEN] {
    let mut out = [0u8; ENTRY_PAYLOAD_LEN];
    out[0] = ENTRY_PREFIX_BYTE;
    out[1..21].copy_from_slice(&l2);
    out[21..29].copy_from_slice(&amount_sompi.to_le_bytes());
    out[29..33].copy_from_slice(&nonce_be.to_be_bytes());
    out
}

pub fn parse_l2_address(addr: &str) -> Result<[u8; 20]> {
    let hex = addr
        .trim()
        .strip_prefix("0x")
        .or_else(|| addr.trim().strip_prefix("0X"))
        .unwrap_or(addr.trim());
    if hex.len() != 40 {
        return Err(EngineError::Message(format!(
            "L2 address must be 20 bytes hex, got {}",
            addr.len()
        )));
    }
    let mut out = [0u8; 20];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(chunk)
            .map_err(|_| EngineError::Message("L2 address is not hex".into()))?;
        out[i] = u8::from_str_radix(s, 16)
            .map_err(|_| EngineError::Message("L2 address is not hex".into()))?;
    }
    Ok(out)
}

pub fn txid_has_galleon_prefix(txid: &str) -> bool {
    txid.trim()
        .to_ascii_lowercase()
        .starts_with(GALLEON_TXID_PREFIX)
}

pub fn entry_amount_ok(amount_sompi: u64) -> bool {
    amount_sompi >= GALLEON_ENTRY_MIN_SOMPI
}

/// SHA-256(prefix + ":" + nonce) leading-zero bits (Igra faucet docs).
pub fn pow_leading_zero_bits(digest: &[u8]) -> u32 {
    let mut bits = 0u32;
    for byte in digest {
        if *byte == 0 {
            bits += 8;
            continue;
        }
        bits += byte.leading_zeros();
        break;
    }
    bits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_20_kas_little_endian_amount() {
        let l2 = parse_l2_address("0x00000000000000000000000000000000000000aa").unwrap();
        let payload = entry_payload(l2, 2_000_000_000, 1);
        assert_eq!(payload[0], 0x92);
        assert_eq!(payload[20], 0xaa);
        assert_eq!(
            &payload[21..29],
            &[0x00, 0x94, 0x35, 0x77, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(&payload[29..33], &[0x00, 0x00, 0x00, 0x01]);
        assert!(entry_amount_ok(100_000_000));
        assert!(!entry_amount_ok(99_999_999));
        assert!(txid_has_galleon_prefix("97b4deadbeef"));
        assert!(!txid_has_galleon_prefix("97b1deadbeef"));
    }

    #[test]
    fn pow_zero_byte_is_eight_bits() {
        assert_eq!(pow_leading_zero_bits(&[0x00, 0x0f]), 12);
        assert_eq!(pow_leading_zero_bits(&[0x80]), 0);
        assert_eq!(pow_leading_zero_bits(&[0x01]), 7);
    }
}
