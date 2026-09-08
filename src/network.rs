//! Kaspa network IDs and ports. Gemini used mainnet gRPC 16110 and defunct TN11.
//! Develop against testnet-10. Public testnet is TN10; do not follow a TN12 suffix.

pub const TESTNET_10_ID: &str = "testnet-10";
pub const TESTNET_10_NETWORK_NAME: &str = "kaspa-testnet-10";
pub const TESTNET_10_REST: &str = "https://api-tn10.kaspa.org";
pub const TN10_EXPLORER: &str = "https://explorer-tn10.kaspa.org";
pub const TN10_FAUCET: &str = "https://faucet-tn10.kaspanet.io/";
/// Community covenant indexer (not kaspad). Nodes still have no getUtxosByCovenantId.
pub const KASCOV_TN10: &str = "https://kascov.io/data/testnet-10";
/// Kasplex KRC-20 indexer on TN10 (inscriptions, not L1 EVM / not USD).
pub const KASPLEX_TN10: &str = "https://tn10api.kasplex.org/v1";
/// Live crate KRC-20 on TN10: minted and transferred here (alice/bob). Not USD.
/// We did not pay the Kasplex deploy burn; a crate-owned tick needs ~1000 tKAS.
pub const KASPLEX_FRONTIER_TICK: &str = "TMBMN";
pub const KASPLEX_FRONTIER_NAME: &str = "Frontier";
/// Kasplex protocol burn for `op=deploy` (same on TN10 as mainnet, in tKAS).
pub const KASPLEX_DEPLOY_FEE_SOMPI: u64 = 1_000 * SOMPI_PER_KAS;
/// Igra Galleon testnet — EVM DeFi / stables live here, not on kaspad.
pub const IGRA_GALLEON_RPC: &str = "https://galleon-testnet.igralabs.com:8545";
pub const IGRA_GALLEON_CHAIN_ID: u64 = 38_836;
/// Galleon test USDC. Not Circle-issued, not redeemable for dollars.
pub const GALLEON_TEST_USDC: &str = "0xFd89676CBb3D2742c565aFC02986370ef4ba667A";
/// Crate-owned gTEST (permit ERC-20). Not USD. Not Circle USDC.
pub const GALLEON_GTEST: &str = "0xbc5e27ab3ce2edb243593cda2437e5b30e0d5d7d";
/// Circle has not published a USDC contract for Igra Galleon (chain 38836).
pub const CIRCLE_USDC_ON_GALLEON: Option<&str> = None;
/// Igra mainnet EVM (chain 38833). Not kaspad. Not TN10.
pub const IGRA_MAINNET_RPC: &str = "https://rpc.igralabs.com:8545";
pub const IGRA_MAINNET_CHAIN_ID: u64 = 38_833;
/// Hyperlane HypSynthetic USDC on Igra mainnet. Bridged collateral, not Circle-issued.
pub const IGRA_MAINNET_HYPERLANE_USDC: &str = "0xA5b8BF902b2844dA17d4506cc827F7F1681735E7";
/// Circle has not published a native USDC mint for Igra mainnet (chain 38833).
pub const CIRCLE_USDC_ON_IGRA_MAINNET: Option<&str> = None;
/// Wrapped iKAS (WETH9-style) on Galleon. Not kaspad. Not USD.
pub const GALLEON_WRAPPED_IKAS: Option<&str> = Some("0x7331b0a33ac9aa92f506f057bfaa049ea133f77f");
/// Local JSON-RPC shim (REST + kascov). Not kaspad. Default bind.
pub const TN10_INTEGRATOR_RPC: &str = "127.0.0.1:18710";
/// Official Igra faucet (dispenses iKAS; this crate does not mint iKAS).
pub const IGRA_FAUCET: &str = "https://faucet.igralabs.com";
/// Galleon L1 entry lock address. First output + `0x92` payload + txid prefix `97b4`.
pub const GALLEON_ENTRY_ADDRESS: &str =
    "kaspatest:qqmstl2znv9tsfgcmj9shme82my867tapz7pdu4ztwdn6sm9452jj5mm0sxzw";
pub const GALLEON_TXID_PREFIX: &str = "97b4";
pub const GALLEON_ENTRY_MIN_SOMPI: u64 = 100_000_000;
/// Kasplex L2 testnet RPC (chain 167012). Caravel DNS is dead; do not use it.
pub const KASPLEX_L2_RPC: &str = "https://rpc.kasplextest.xyz";
pub const KASPLEX_L2_CHAIN_ID: u64 = 167_012;
pub const ADDRESS_PREFIX_TESTNET: &str = "kaspatest:";
pub const ADDRESS_PREFIX_MAINNET: &str = "kaspa:";
pub const MAINNET_EXPLORER: &str = "https://explorer.kaspa.org";

/// Independent developer of this crate. Optional mainnet KAS — not the Kaspa Dev Fund.
/// Do not send tKAS / `kaspatest:` here.
pub const DEV_DONATION_ADDRESS: &str =
    "kaspa:qpxdemlyx445kt5xteux0qhadaw8lh5m0vnqvcy8fh483t70usgkkeulsx9cm";

/// Default local kaspad ports for testnet-10.
pub const TN10_GRPC: u16 = 16210;
pub const TN10_WRPC_BORSH: u16 = 17210;
pub const TN10_WRPC_JSON: u16 = 18210;
/// host02 TN10 replica forwarded to loopback (see scripts/tn10_host02_tunnel.ps1).
pub const TN10_WRPC_REPLICA_JSON: u16 = 28210;
pub const TN10_P2P: u16 = 16211;

pub fn loopback_wrpc_url(port: u16) -> String {
    format!("ws://127.0.0.1:{port}")
}

/// Default laptop + host02-tunnel replica URLs for N-of-M rehearsal.
pub fn default_dual_owned_node_urls() -> Vec<String> {
    vec![
        loopback_wrpc_url(TN10_WRPC_JSON),
        loopback_wrpc_url(TN10_WRPC_REPLICA_JSON),
    ]
}

/// Crescendo / current mainnet and TN10 target. DAGKnight is not activated.
pub const TARGET_BPS: f64 = 10.0;

/// rusty-kaspa `docs/testnet12.md` is the covenants experiment, not 100 BPS.
pub const TN12_BPS_NOTE: &str =
    "TN12 is the covenants testnet (rusty-kaspa tn12 branch), not 100 BPS. Rothschild -t=5 is 5 transactions/second (TPS); docs ask not to exceed 100 TPS. TPS is not BPS. 100 BPS is a later kaspa.org lore target. Public testnet is TN10 @ 10 BPS.";

/// Sompi per KAS.
pub const SOMPI_PER_KAS: u64 = 100_000_000;

/// ~6s at 10 BPS. Exchanges typically use more; this is a TN10 default.
pub const DEFAULT_DEPOSIT_CONFIRMATIONS: u64 = 60;

/// Post-Crescendo coinbase maturity (DAA). 60 confirmations is not enough.
pub const COINBASE_MATURITY_DAA: u64 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressNetwork {
    Mainnet,
    Testnet,
    Other,
}

pub fn local_kaspad_cmd() -> &'static str {
    r#"kaspad --testnet --netsuffix=10 --utxoindex --disable-upnp --rpclisten=127.0.0.1:16210 --rpclisten-json=127.0.0.1:18210 --appdir %LOCALAPPDATA%\kaspa\tn10"#
}

/// `kaspatest:` starts with `kaspa:`, so testnet must be matched first.
pub fn classify_address(addr: &str) -> AddressNetwork {
    if addr.starts_with(ADDRESS_PREFIX_TESTNET) {
        AddressNetwork::Testnet
    } else if addr.starts_with(ADDRESS_PREFIX_MAINNET) {
        AddressNetwork::Mainnet
    } else {
        AddressNetwork::Other
    }
}

pub fn is_testnet_address(addr: &str) -> bool {
    classify_address(addr) == AddressNetwork::Testnet
}

/// Validate a complete Kaspa CashAddr string, including its eight-symbol checksum.
pub fn is_valid_testnet_address(addr: &str) -> bool {
    cashaddr_valid(addr, "kaspatest")
}

fn cashaddr_valid(addr: &str, expected_prefix: &str) -> bool {
    if addr != addr.to_ascii_lowercase() {
        return false;
    }
    let Some((prefix, payload)) = addr.rsplit_once(':') else {
        return false;
    };
    if prefix != expected_prefix || payload.len() <= 8 {
        return false;
    }
    const CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
    let mut values = Vec::with_capacity(prefix.len() + 1 + payload.len());
    values.extend(prefix.bytes().map(|byte| u64::from(byte & 0x1f)));
    values.push(0);
    for symbol in payload.bytes() {
        let Some(index) = CHARSET.iter().position(|candidate| *candidate == symbol) else {
            return false;
        };
        values.push(index as u64);
    }
    cashaddr_polymod(&values) == 0
}

fn cashaddr_polymod(values: &[u64]) -> u64 {
    const GENERATORS: [u64; 5] = [
        0x98f2bc8e61,
        0x79b76d99e2,
        0xf33e5fb3c4,
        0xae2eabe2a8,
        0x1e4f43e470,
    ];
    let mut checksum = 1_u64;
    for value in values {
        let top = checksum >> 35;
        checksum = ((checksum & 0x07_ffff_ffff) << 5) ^ value;
        for (index, generator) in GENERATORS.iter().enumerate() {
            if ((top >> index) & 1) != 0 {
                checksum ^= generator;
            }
        }
    }
    checksum ^ 1
}

pub fn is_mainnet_address(addr: &str) -> bool {
    classify_address(addr) == AddressNetwork::Mainnet
}

pub fn is_tn10_network_name(name: &str) -> bool {
    name == TESTNET_10_NETWORK_NAME || name == TESTNET_10_ID
}

/// Official rusty-kaspa `docs/testnet12.md`: covenants experiment (`--netsuffix=12`),
/// separate P2P from TN10. kaspa.org lore: 100 BPS is a later HF, not TN12's rate.
/// This crate stays on public TN10 (GHOSTDAG @ 10 BPS).
pub fn is_unsupported_testnet_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("testnet-11")
        || n.contains("testnet-12")
        || n.contains("kaspa-testnet-11")
        || n.contains("kaspa-testnet-12")
}

/// Accept only public TN10. TN11 is retired; TN12 is the covenants testnet, not 100 BPS.
pub fn require_tn10(name: &str) -> crate::error::Result<()> {
    if is_tn10_network_name(name) {
        return Ok(());
    }
    if is_unsupported_testnet_name(name) {
        return Err(crate::error::EngineError::UnsupportedTestnet {
            found: name.to_string(),
        });
    }
    Err(crate::error::EngineError::WrongNetwork {
        expected: TESTNET_10_ID.to_string(),
        found: name.to_string(),
    })
}

pub fn kas_to_sompi(kas: u64) -> Option<u64> {
    kas.checked_mul(SOMPI_PER_KAS)
}

pub fn sompi_to_kas(sompi: u64) -> f64 {
    sompi as f64 / SOMPI_PER_KAS as f64
}

pub fn tn10_tx_url(txid: &str) -> String {
    format!("{TN10_EXPLORER}/txs/{txid}")
}

pub fn tn10_address_url(address: &str) -> String {
    format!("{TN10_EXPLORER}/addresses/{address}")
}

pub fn mainnet_address_url(address: &str) -> String {
    format!("{MAINNET_EXPLORER}/addresses/{address}")
}

/// One-line credit. Safe to print: public address only.
pub fn print_dev_sig() {
    println!("dev sig / optional mainnet KAS: {DEV_DONATION_ADDRESS}");
    println!("  {}", mainnet_address_url(DEV_DONATION_ADDRESS));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kaspatest_is_not_mainnet() {
        assert_eq!(
            classify_address("kaspatest:qz0s2example"),
            AddressNetwork::Testnet
        );
        assert!(is_testnet_address("kaspatest:qz0s2example"));
        assert!(!is_mainnet_address("kaspatest:qz0s2example"));
        assert!(is_mainnet_address(DEV_DONATION_ADDRESS));
        assert!(DEV_DONATION_ADDRESS.starts_with(ADDRESS_PREFIX_MAINNET));
        assert!(!is_testnet_address(DEV_DONATION_ADDRESS));
        assert!(!is_testnet_address("kaspa:qz0s2example"));
        assert_eq!(classify_address("not-an-address"), AddressNetwork::Other);
    }

    #[test]
    fn validates_full_testnet_address_checksum() {
        let valid = "kaspatest:qptv6u8kel95drh2p2z492cyksk8lpetep286fngqu5j9nk57g642lzf748kt";
        assert!(is_valid_testnet_address(valid));
        let mut broken = valid.to_string();
        broken.pop();
        broken.push('q');
        assert!(!is_valid_testnet_address(&broken));
        assert!(!is_valid_testnet_address("kaspatest:abc"));
        assert!(!is_valid_testnet_address(DEV_DONATION_ADDRESS));
    }

    #[test]
    fn tn10_network_names() {
        assert!(is_tn10_network_name("kaspa-testnet-10"));
        assert!(is_tn10_network_name("testnet-10"));
        assert!(!is_tn10_network_name("kaspa-mainnet"));
        assert!(!is_tn10_network_name("kaspa-testnet-11"));
        assert!(!is_tn10_network_name("kaspa-testnet-12"));
        assert!(is_unsupported_testnet_name("testnet-12"));
        assert!(is_unsupported_testnet_name("kaspa-testnet-11"));
        assert!(!is_unsupported_testnet_name("testnet-10"));
        assert!(!is_unsupported_testnet_name("kaspa-testnet-10"));
        assert!(local_kaspad_cmd().contains("--netsuffix=10"));
        assert!(!local_kaspad_cmd().contains("--netsuffix=12"));
        assert!(local_kaspad_cmd().contains("127.0.0.1:18210"));
        assert!(local_kaspad_cmd().contains(r"%LOCALAPPDATA%\kaspa\tn10"));
        assert!(!local_kaspad_cmd().contains("rusty-kaspa"));
        assert!(TN12_BPS_NOTE.contains("not 100 BPS"));
        assert!(TN12_BPS_NOTE.contains("covenants"));
        assert!(TN12_BPS_NOTE.contains("TPS is not BPS"));
        assert!(TN12_BPS_NOTE.contains("100 TPS"));
        assert!(require_tn10("kaspa-testnet-10").is_ok());
        assert!(require_tn10("testnet-10").is_ok());
        match require_tn10("kaspa-testnet-12") {
            Err(crate::error::EngineError::UnsupportedTestnet { found }) => {
                assert!(found.contains("12"));
            }
            other => panic!("expected UnsupportedTestnet, got {other:?}"),
        }
        match require_tn10("kaspa-mainnet") {
            Err(crate::error::EngineError::WrongNetwork { found, .. }) => {
                assert_eq!(found, "kaspa-mainnet");
            }
            other => panic!("expected WrongNetwork, got {other:?}"),
        }
    }

    #[test]
    fn sompi_roundtrip() {
        assert_eq!(kas_to_sompi(1), Some(100_000_000));
        assert_eq!(sompi_to_kas(50_000_000), 0.5);
        assert!(kas_to_sompi(u64::MAX).is_none());
    }

    #[test]
    fn explorer_urls() {
        assert_eq!(
            tn10_tx_url("abc"),
            "https://explorer-tn10.kaspa.org/txs/abc"
        );
        assert!(mainnet_address_url(DEV_DONATION_ADDRESS)
            .starts_with("https://explorer.kaspa.org/addresses/kaspa:"));
        assert!(KASCOV_TN10.starts_with("https://"));
        assert!(KASPLEX_TN10.starts_with("https://"));
        assert!(IGRA_GALLEON_RPC.starts_with("https://"));
        assert!(KASPLEX_L2_RPC.starts_with("https://"));
        assert_eq!(IGRA_GALLEON_CHAIN_ID, 38_836);
        assert_eq!(
            GALLEON_TEST_USDC,
            "0xFd89676CBb3D2742c565aFC02986370ef4ba667A"
        );
        assert_eq!(GALLEON_GTEST, "0xbc5e27ab3ce2edb243593cda2437e5b30e0d5d7d");
        assert_ne!(
            GALLEON_GTEST.to_ascii_lowercase(),
            GALLEON_TEST_USDC.to_ascii_lowercase()
        );
        assert!(CIRCLE_USDC_ON_GALLEON.is_none());
        assert_eq!(IGRA_MAINNET_CHAIN_ID, 38_833);
        assert!(IGRA_MAINNET_RPC.starts_with("https://"));
        assert_eq!(
            IGRA_MAINNET_HYPERLANE_USDC,
            "0xA5b8BF902b2844dA17d4506cc827F7F1681735E7"
        );
        assert!(CIRCLE_USDC_ON_IGRA_MAINNET.is_none());
        assert_ne!(
            IGRA_MAINNET_HYPERLANE_USDC.to_ascii_lowercase(),
            GALLEON_TEST_USDC.to_ascii_lowercase()
        );
        assert_eq!(
            GALLEON_WRAPPED_IKAS,
            Some("0x7331b0a33ac9aa92f506f057bfaa049ea133f77f")
        );
        assert_eq!(KASPLEX_L2_CHAIN_ID, 167_012);
        assert!(IGRA_FAUCET.starts_with("https://"));
        assert!(GALLEON_ENTRY_ADDRESS.starts_with("kaspatest:"));
        assert_eq!(GALLEON_TXID_PREFIX, "97b4");
        assert_eq!(KASPLEX_FRONTIER_TICK, "TMBMN");
        assert_eq!(KASPLEX_DEPLOY_FEE_SOMPI, 1_000 * SOMPI_PER_KAS);
    }

    #[test]
    fn dual_owned_node_urls_use_loopback_ports() {
        let urls = default_dual_owned_node_urls();
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0], loopback_wrpc_url(TN10_WRPC_JSON));
        assert_eq!(urls[1], loopback_wrpc_url(TN10_WRPC_REPLICA_JSON));
    }
}
