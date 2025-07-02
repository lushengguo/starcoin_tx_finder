mod block_loader;

use block_loader::{
    format_events_output, get_starcoin_block_ws, get_transaction_by_hash_ws,
    get_transaction_events_ws, get_transaction_info_ws,
};

use crate::block_loader::get_transactions_from_block;

#[tokio::main]
async fn main() {
    let tx_hash = "0xe29d7508fe37d756d83e672be53843d10d084f9c69fca1b7e9a34ea8eb96f918";

    if let Some(tx_response) = get_transaction_by_hash_ws(tx_hash).await {
        println!("=== TRANSACTION DATA ===");
        println!("{}", serde_json::to_string_pretty(&tx_response).unwrap());
        println!();
    }

    if let Some(tx_info_response) = get_transaction_info_ws(tx_hash).await {
        println!("=== TRANSACTION INFO ===");
        println!(
            "{}",
            serde_json::to_string_pretty(&tx_info_response).unwrap()
        );
        println!();
    }

    if let Some(events_response) = get_transaction_events_ws(tx_hash).await {
        println!("=== TRANSACTION EVENTS (Raw) ===");
        println!(
            "{}",
            serde_json::to_string_pretty(&events_response).unwrap()
        );
        println!();

        if let Some(formatted_events) = format_events_output(&events_response) {
            println!("=== FORMATTED EVENTS ===");
            for event in formatted_events {
                println!("{}", serde_json::to_string_pretty(&event).unwrap());
            }
        }
    }
}
