use kaspa_frontier_engine::Krc20StateEngine;

fn main() {
    let mut engine = Krc20StateEngine::new();
    engine
        .apply(
            "txid-deploy",
            "kaspatest:issuer",
            r#"{"p":"krc-20","op":"deploy","tick":"KUSDT","max":"1000000000","lim":"1000"}"#,
        )
        .unwrap();
    engine
        .apply(
            "txid-mint",
            "kaspatest:alice",
            r#"{"p":"krc-20","op":"mint","tick":"KUSDT","amt":"1000"}"#,
        )
        .unwrap();
    engine
        .apply(
            "txid-transfer",
            "kaspatest:alice",
            r#"{"p":"krc-20","op":"transfer","tick":"KUSDT","amt":"250","to":"kaspatest:bob"}"#,
        )
        .unwrap();
    println!(
        "alice={} bob={} minted={:?}",
        engine.get_balance("KUSDT", "kaspatest:alice"),
        engine.get_balance("KUSDT", "kaspatest:bob"),
        engine.minted("KUSDT")
    );
    println!("This is an in-memory indexer demo, not Kasplex L1 balances.");
    kaspa_frontier_engine::network::print_dev_sig();
}
