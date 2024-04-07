use revm::{
    db::BenchmarkDB,
    primitives::{Asset, Bytecode, TransactTo, B256, BASE_ASSET_ID, U256},
    Evm,
};

use std::time::Duration;

fn main() {
    // BenchmarkDB is dummy state that implements Database trait.
    let mut evm = Evm::builder()
        .with_db(BenchmarkDB::new_bytecode(Bytecode::new()))
        .modify_tx_env(|tx| {
            // execution globals block hash/gas_limit/coinbase/timestamp..
            tx.caller = "0x0000000000000000000000000000000000000001"
                .parse()
                .unwrap();
            let asset = Asset {
                id: B256::from(BASE_ASSET_ID),
                amount: U256::from(10),
            };
            tx.transferred_assets = vec![asset];
            tx.value = U256::from(10);
            tx.transact_to = TransactTo::Call(
                "0x0000000000000000000000000000000000000000"
                    .parse()
                    .unwrap(),
            );
        })
        .build();

    // Microbenchmark
    let bench_options = microbench::Options::default().time(Duration::from_secs(3));

    microbench::bench(&bench_options, "Simple value transfer", || {
        let _ = evm.transact().unwrap();
    });
}
