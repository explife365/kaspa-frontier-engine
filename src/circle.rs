//! Circle USDC / CCTP facts used by this crate.
//!
//! Igra Galleon (chain 38836) and Igra mainnet (chain 38833) are **not**
//! Circle-supported mint chains. Galleon test USDC and Igra Hyperlane USDC
//! are not cash USDC. Do not deploy a USDC lookalike.

use crate::network::{GALLEON_TEST_USDC, IGRA_MAINNET_HYPERLANE_USDC};

/// Canonical Circle USDC on Ethereum (chain 1).
/// Circle docs / solc checksum: 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48
pub const CIRCLE_USDC_ETHEREUM: &str = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
/// Circle native USDC on Base.
pub const CIRCLE_USDC_BASE: &str = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";
/// Circle native USDC on Arbitrum One.
pub const CIRCLE_USDC_ARBITRUM: &str = "0xaf88d065e77c8cC2239327C5EDb3A432268e5831";
/// Circle native USDC on OP Mainnet.
pub const CIRCLE_USDC_OPTIMISM: &str = "0x0b2C639c533813f4Aa9D7837CAf62653d097Ff85";
/// Circle native USDC on Polygon PoS (not USDC.e).
pub const CIRCLE_USDC_POLYGON: &str = "0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359";
/// Circle native USDC on Avalanche C-Chain.
pub const CIRCLE_USDC_AVALANCHE: &str = "0xB97EF9Ef8734C71904D8002F8b6Bc66Dd9c48a6E";

pub fn same_addr(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

/// Circle-published native USDC for a small set of EVM chains. `None` = not listed here.
pub fn circle_usdc_on_chain(chain_id: u64) -> Option<&'static str> {
    match chain_id {
        1 => Some(CIRCLE_USDC_ETHEREUM),
        8453 => Some(CIRCLE_USDC_BASE),
        42_161 => Some(CIRCLE_USDC_ARBITRUM),
        10 => Some(CIRCLE_USDC_OPTIMISM),
        137 => Some(CIRCLE_USDC_POLYGON),
        43_114 => Some(CIRCLE_USDC_AVALANCHE),
        _ => None,
    }
}

pub fn circle_lists_chain(chain_id: u64) -> bool {
    circle_usdc_on_chain(chain_id).is_some()
}

pub fn is_circle_usdc(chain_id: u64, token: &str) -> bool {
    circle_usdc_on_chain(chain_id).is_some_and(|listed| same_addr(token, listed))
}

pub fn is_galleon_test_usdc(token: &str) -> bool {
    same_addr(token, GALLEON_TEST_USDC)
}

/// Igra Labs Hyperlane warp USDC. Synthetic / bridged, not a Circle mint.
pub fn is_hyperlane_igra_usdc(token: &str) -> bool {
    same_addr(token, IGRA_MAINNET_HYPERLANE_USDC)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::{
        CIRCLE_USDC_ON_GALLEON, CIRCLE_USDC_ON_IGRA_MAINNET, IGRA_GALLEON_CHAIN_ID,
        IGRA_MAINNET_CHAIN_ID, IGRA_MAINNET_HYPERLANE_USDC,
    };

    #[test]
    fn galleon_is_not_a_circle_chain() {
        assert_eq!(IGRA_GALLEON_CHAIN_ID, 38_836);
        assert!(CIRCLE_USDC_ON_GALLEON.is_none());
        assert_eq!(circle_usdc_on_chain(IGRA_GALLEON_CHAIN_ID), None);
        assert!(!circle_lists_chain(IGRA_GALLEON_CHAIN_ID));
        assert!(!same_addr(GALLEON_TEST_USDC, CIRCLE_USDC_ETHEREUM));
        assert!(!is_circle_usdc(IGRA_GALLEON_CHAIN_ID, GALLEON_TEST_USDC));
        assert!(is_galleon_test_usdc(GALLEON_TEST_USDC));
        assert_eq!(
            CIRCLE_USDC_ETHEREUM.to_ascii_lowercase(),
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
        );
        assert!(is_circle_usdc(1, CIRCLE_USDC_ETHEREUM));
        assert!(is_circle_usdc(8453, CIRCLE_USDC_BASE));
        assert!(!is_circle_usdc(1, GALLEON_TEST_USDC));
    }

    #[test]
    fn igra_hyperlane_usdc_is_not_circle() {
        assert_eq!(IGRA_MAINNET_CHAIN_ID, 38_833);
        assert!(CIRCLE_USDC_ON_IGRA_MAINNET.is_none());
        assert!(!circle_lists_chain(IGRA_MAINNET_CHAIN_ID));
        assert_eq!(circle_usdc_on_chain(IGRA_MAINNET_CHAIN_ID), None);
        assert!(is_hyperlane_igra_usdc(IGRA_MAINNET_HYPERLANE_USDC));
        assert!(!is_circle_usdc(
            IGRA_MAINNET_CHAIN_ID,
            IGRA_MAINNET_HYPERLANE_USDC
        ));
        assert!(!same_addr(
            IGRA_MAINNET_HYPERLANE_USDC,
            CIRCLE_USDC_ETHEREUM
        ));
        assert!(!is_galleon_test_usdc(IGRA_MAINNET_HYPERLANE_USDC));
    }
}
