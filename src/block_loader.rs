use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::connect_async;
use url::Url;

pub async fn get_starcoin_block_ws(
    block_number: u64,
    full_details: bool,
) -> Option<serde_json::Value> {
    let ws_url = "ws://main.seed.starcoin.org:9870";
    let url = Url::parse(ws_url).ok()?;

    let block_option = if full_details {
        json!({ "Full": true })
    } else {
        json!({ "Header": true })
    };

    let request_payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "chain.get_block_by_number",
        "params": [block_number, block_option]
    });

    let (mut ws_stream, _) = connect_async(url).await.ok()?;
    let msg = tokio_tungstenite::tungstenite::Message::Text(request_payload.to_string());
    ws_stream.send(msg).await.ok()?;

    while let Some(Ok(msg)) = ws_stream.next().await {
        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
            return serde_json::from_str(&text).ok();
        }
    }

    None
}

pub async fn get_transaction_events_ws(transaction_hash: &str) -> Option<serde_json::Value> {
    let ws_url = "ws://main.seed.starcoin.org:9870";
    let url = Url::parse(ws_url).ok()?;

    let request_payload = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "chain.get_events_by_txn_hash",
        "params": [transaction_hash]
    });

    let (mut ws_stream, _) = connect_async(url).await.ok()?;
    let msg = tokio_tungstenite::tungstenite::Message::Text(request_payload.to_string());
    ws_stream.send(msg).await.ok()?;

    while let Some(Ok(msg)) = ws_stream.next().await {
        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
            return serde_json::from_str(&text).ok();
        }
    }

    None
}

pub async fn get_transaction_by_hash_ws(transaction_hash: &str) -> Option<serde_json::Value> {
    let ws_url = "ws://main.seed.starcoin.org:9870";
    let url = Url::parse(ws_url).ok()?;

    let request_payload = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "chain.get_transaction",
        "params": [transaction_hash]
    });

    let (mut ws_stream, _) = connect_async(url).await.ok()?;
    let msg = tokio_tungstenite::tungstenite::Message::Text(request_payload.to_string());
    ws_stream.send(msg).await.ok()?;

    while let Some(Ok(msg)) = ws_stream.next().await {
        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
            return serde_json::from_str(&text).ok();
        }
    }

    None
}

pub async fn get_transaction_info_ws(transaction_hash: &str) -> Option<serde_json::Value> {
    let ws_url = "ws://main.seed.starcoin.org:9870";
    let url = Url::parse(ws_url).ok()?;

    let request_payload = json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "chain.get_transaction_info",
        "params": [transaction_hash]
    });

    let (mut ws_stream, _) = connect_async(url).await.ok()?;
    let msg = tokio_tungstenite::tungstenite::Message::Text(request_payload.to_string());
    ws_stream.send(msg).await.ok()?;

    while let Some(Ok(msg)) = ws_stream.next().await {
        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
            return serde_json::from_str(&text).ok();
        }
    }

    None
}

pub fn get_transactions_from_block(block_response: &serde_json::Value) -> Option<Vec<String>> {
    if let Some(result) = block_response.get("result") {
        if let Some(body) = result.get("body") {
            if let Some(transactions) = body.get("Full") {
                if let Some(tx_array) = transactions.as_array() {
                    return Some(
                        tx_array
                            .iter()
                            .filter_map(|tx| tx.get("transaction_hash"))
                            .filter_map(|hash| hash.as_str().map(String::from))
                            .collect(),
                    );
                }
            }
        }
    }
    None
}

pub fn parse_event_data(event: &serde_json::Value) -> serde_json::Value {
    let mut parsed_event = serde_json::json!({});

    if let Some(data) = event.get("data") {
        parsed_event["Data"] = data.clone();
    }

    if let Some(type_tag) = event.get("type_tag") {
        if let Some(type_str) = type_tag.as_str() {
            if let Some(module_start) = type_str.find("::") {
                let after_address = &type_str[module_start + 2..];
                if let Some(module_end) = after_address.find("::") {
                    let module_name = &after_address[..module_end];
                    parsed_event["Module"] = serde_json::Value::String(module_name.to_string());

                    let event_name_with_generics = &after_address[module_end + 2..];
                    parsed_event["Name"] =
                        serde_json::Value::String(event_name_with_generics.to_string());
                }
            }
        }
    }

    if let Some(event_key) = event.get("event_key") {
        if let Some(key_str) = event_key.as_str() {
            if key_str.len() >= 42 {
                let salt_part = &key_str[2..18];
                let address_part = &key_str[18..];

                if let Ok(salt_bytes) = hex::decode(salt_part) {
                    if salt_bytes.len() >= 4 {
                        let salt = u32::from_le_bytes([
                            salt_bytes[0],
                            salt_bytes[1],
                            salt_bytes[2],
                            salt_bytes[3],
                        ]);
                        parsed_event["Key"] = serde_json::json!({
                            "address": format!("0x{}", address_part),
                            "salt": salt.to_string()
                        });
                    }
                }
            }
        }
    }

    if let Some(seq) = event.get("event_seq_number") {
        if let Some(seq_str) = seq.as_str() {
            if let Ok(seq_num) = seq_str.parse::<u64>() {
                parsed_event["Seq"] = serde_json::Value::String(seq_num.to_string());
            }
        }
    }

    parsed_event
}

pub fn format_events_output(events_response: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
    if let Some(result) = events_response.get("result") {
        if let Some(events_array) = result.as_array() {
            let formatted_events: Vec<serde_json::Value> = events_array
                .iter()
                .map(|event| parse_event_data(event))
                .collect();
            return Some(formatted_events);
        }
    }
    None
}
