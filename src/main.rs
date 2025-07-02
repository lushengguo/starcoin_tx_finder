mod block_loader;

use block_loader::{
    format_events_part, format_raw_data_part, get_decoded_payload, get_starcoin_block_ws,
    get_transaction_by_hash_ws, get_transaction_events_ws, get_transaction_info_ws,
};

use crate::block_loader::get_transactions_from_block;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: {} <transaction_hash>", args[0]);
        eprintln!(
            "Example: {} 0xe29d7508fe37d756d83e672be53843d10d084f9c69fca1b7e9a34ea8eb96f918",
            args[0]
        );
        std::process::exit(1);
    }

    let tx_hash = &args[1];

    let tx_response = get_transaction_by_hash_ws(tx_hash).await;
    let tx_info_response = get_transaction_info_ws(tx_hash).await;
    let events_response = get_transaction_events_ws(tx_hash).await;

    let mut json: serde_json::Value = serde_json::json!({
        "events": serde_json::Value::Null,
        "raw_data": serde_json::Value::Null,
        "decoded_payload": serde_json::Value::Null
    });

    if let (Some(tx), Some(info), Some(events)) = (tx_response, tx_info_response, events_response) {
        if let Some(complete_format) = format_raw_data_part(&tx, &info, &events) {
            json["raw_data"] = serde_json::to_value(&complete_format).unwrap();
        }

        let decoded_payload = get_decoded_payload(&tx);
        if let Some(payload) = decoded_payload {
            json["decoded_payload"] = serde_json::to_value(&payload).unwrap();
        }

        if let Some(formatted_events) = format_events_part(&events) {
            json["events"] = serde_json::to_value(&formatted_events).unwrap();
        }
    }

    println!("{}", serde_json::to_string_pretty(&json).unwrap());
}
