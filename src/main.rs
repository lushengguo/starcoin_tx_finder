mod block_loader;

use block_loader::{
    format_events_part, format_raw_data_part, get_decoded_payload, get_transaction_by_hash_ws,
    get_transaction_events_ws, get_transaction_info_ws,
};

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

    let ws_url = "ws://main.seed.starcoin.org:9870";

    let tx_response = get_transaction_by_hash_ws(ws_url, tx_hash).await;
    let block_number = match tx_response
        .as_ref()
        .and_then(|tx| tx.get("result"))
        .and_then(|r| r.get("block_number"))
        .and_then(|n| n.as_str())
        .and_then(|s| s.parse::<u64>().ok())
    {
        Some(n) => n,
        None => {
            println!("not found in any blocks");
            return;
        }
    };

    if block_number >= 20000 {
        println!(
            "this transaction is not found in the first 20000 blocks, it was found in block number: {}",
            block_number
        );
        // return;
    }

    let tx_info_response = get_transaction_info_ws(ws_url, tx_hash).await;
    let events_response = get_transaction_events_ws(ws_url, tx_hash).await;

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
