use kaspa_frontier_engine::network::{self, is_testnet_address};
use kaspa_frontier_engine::{kasplex_mint_inscription, KasplexClient};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arg = env::args().nth(1);
    network::print_dev_sig();
    println!("Kasplex indexer {}", network::KASPLEX_TN10);
    println!(
        "Crate TN10 KRC-20  {} ({}) — live mint+transfer, not USD. Deploy of a new tick burns {} tKAS.",
        network::KASPLEX_FRONTIER_TICK,
        network::KASPLEX_FRONTIER_NAME,
        network::KASPLEX_DEPLOY_FEE_SOMPI / 100_000_000
    );
    println!();

    let client = KasplexClient::new(network::KASPLEX_TN10)?;
    match arg.as_deref() {
        None | Some("--mintable") => {
            let (status, page, open_all) = tokio::join!(
                client.info(),
                client.tokenlist(None),
                client.collect_open_mints(32)
            );
            let status = status?;
            let page = page?;
            let open_all = open_all?;
            let first_page_open = page.open_mints().count();
            println!(
                "indexer  {}  tokens={}  daa_gap={}",
                if status.is_synced() {
                    "synced"
                } else {
                    status.message.as_str()
                },
                status.info.token_total,
                status.info.daa_score_gap
            );
            println!(
                "first page  {} rows  {} open mints  next={}",
                page.result.len(),
                first_page_open,
                page.next.as_deref().unwrap_or("-")
            );
            println!(
                "all pages  {} open mints (paginated, max 32 pages)",
                open_all.len()
            );
            for token in open_all.iter().take(12) {
                println!(
                    "  {}  minted={}  lim={}  remaining={}  reveal={}",
                    token.ticker().unwrap_or("?"),
                    token.minted,
                    token.lim,
                    token.remaining().unwrap_or(0),
                    token.hash_rev
                );
            }
            if let Some(first) = open_all.first().and_then(|t| t.ticker()) {
                println!();
                println!(
                    "example mint envelope  {}",
                    kasplex_mint_inscription(first)?
                );
                println!("  python examples/kasplex_krc20.py --commit-address --tick {first}");
                println!("  python examples/kasplex_krc20.py --mint {first} --from alice");
                println!(
                    "  python examples/kasplex_krc20.py --transfer {first} --from alice --to bob --amt <lim/2>"
                );
            }
        }
        Some(addr) if is_testnet_address(addr) => {
            let (status, held) = tokio::join!(client.info(), client.address_tokenlist(addr));
            let status = status?;
            let held = held?;
            println!(
                "indexer  {}  tokens={}  daa_gap={}",
                if status.is_synced() {
                    "synced"
                } else {
                    status.message.as_str()
                },
                status.info.token_total,
                status.info.daa_score_gap
            );
            println!("address  {addr}");
            if held.is_empty() {
                println!("  (no KRC-20 balances on Kasplex)");
            }
            for row in held {
                println!(
                    "  {}  balance={}  locked={}",
                    row.tick.or(row.ca).unwrap_or_default(),
                    row.balance,
                    row.locked
                );
            }
        }
        Some(tick) => {
            let (status, token) = tokio::join!(client.info(), client.token(tick));
            let status = status?;
            println!(
                "indexer  {}  tokens={}  daa_gap={}",
                if status.is_synced() {
                    "synced"
                } else {
                    status.message.as_str()
                },
                status.info.token_total,
                status.info.daa_score_gap
            );
            match token? {
                Some(token) => {
                    println!(
                        "token {}  mode={}  state={}  minted={} / {}  open_mint={}",
                        token.ticker().unwrap_or(tick),
                        token.mode,
                        token.state,
                        token.minted,
                        token.max,
                        token.is_open_mint()
                    );
                    if token.is_open_mint() {
                        println!("envelope  {}", kasplex_mint_inscription(tick)?);
                    }
                }
                None => println!("no Kasplex token named {tick}"),
            }
        }
    }
    Ok(())
}
